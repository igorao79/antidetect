#!/usr/bin/env bash
# Builds Owl for macOS and publishes it as a GitHub release of the fork, the
# channel installed Owl apps update from (`RELEASES_REPO` in
# src-tauri/src/app_auto_updater.rs). An app checks at start and every three
# hours, downloads the .dmg, checks it against SHA256SUMS.txt and offers to
# restart into the new version.
#
# A release is signed with the Developer ID certificate in this Mac's keychain
# and notarized by Apple, so macOS opens it without a warning and keeps its
# camera and microphone grants across updates. The signing identity and the
# App Store Connect API key for notarization come from ~/.config/owl/release.env
# (kept out of the repository):
#
#   APPLE_SIGNING_IDENTITY="Developer ID Application: <name> (<team id>)"
#   APPLE_API_ISSUER=<issuer id>
#   APPLE_API_KEY=<key id>
#   APPLE_API_KEY_PATH=$HOME/.appstoreconnect/private_keys/AuthKey_<key id>.p8
#
# Bump the version in package.json, src-tauri/Cargo.toml and
# src-tauri/tauri.conf.json, commit and push, then have GitHub's macOS runner
# run this script (.github/workflows/release-macos.yml, signing material from
# the repository's secrets):
#
#   gh workflow run release-macos.yml -R igorao79/antidetect
#
# or run it on this Mac:
#
#   scripts/release-macos.sh
set -euo pipefail
cd "$(dirname "$0")/.."

release_env="$HOME/.config/owl/release.env"
# shellcheck disable=SC1090
[ -f "$release_env" ] && set -a && . "$release_env" && set +a
for name in APPLE_SIGNING_IDENTITY APPLE_API_ISSUER APPLE_API_KEY APPLE_API_KEY_PATH; do
  if [ -z "${!name:-}" ]; then
    echo "$name is not set (see $release_env): a release must be signed and notarized." >&2
    exit 1
  fi
done
test -f "$APPLE_API_KEY_PATH"

version=$(node -p 'require("./src-tauri/tauri.conf.json").version')
# The repository the app reads; `gh` would otherwise pick upstream's.
repo=$(sed -n 's/^const RELEASES_REPO: &str = "\(.*\)";/\1/p' src-tauri/src/app_auto_updater.rs)
test -n "$repo"
tag="v$version"
if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
  echo "Commit the working tree first: a release is built from a pushed commit." >&2
  exit 1
fi
commit=$(git rev-parse HEAD)
if ! git branch -r --contains "$commit" | grep -q .; then
  echo "Push $commit first." >&2
  exit 1
fi
if gh release view "$tag" -R "$repo" > /dev/null 2>&1; then
  echo "$tag is already released; bump the version first." >&2
  exit 1
fi

# STABLE_RELEASE makes the build a stable one, which takes updates; without it
# a build calls itself dev-<version> and never updates. BUILD_TAG names the
# version outright: on GitHub's runner build.rs would otherwise take
# GITHUB_REF_NAME, the branch, and the app and donut-proxy would call
# themselves `main-antidetect`.
env -u HTTP_PROXY -u HTTPS_PROXY -u http_proxy -u https_proxy -u ALL_PROXY -u all_proxy \
  STABLE_RELEASE=1 BUILD_TAG="$tag" CARGO_INCREMENTAL=0 pnpm tauri build
proxy_version=$(src-tauri/target/release/donut-proxy --version)
if [ "$proxy_version" != "donut-proxy $tag" ]; then
  echo "The build calls itself \"$proxy_version\", not $tag." >&2
  exit 1
fi

bundle=src-tauri/target/release/bundle/dmg
dmg="Owl_${version}_aarch64.dmg"
app=src-tauri/target/release/bundle/macos/Owl.app
test -f "$bundle/$dmg"

# Tauri signs and notarizes the app and staples its ticket before it builds
# the disk image; the image is signed and notarized as well, so a downloaded
# copy opens without a warning before it is even mounted.
codesign --verify --deep --strict "$app"
signed_by=$(codesign -dvv "$app" 2>&1)
grep -q "Authority=Developer ID Application" <<< "$signed_by"
xcrun stapler validate "$app"
codesign --force --timestamp --sign "$APPLE_SIGNING_IDENTITY" "$bundle/$dmg"
notarized=$(xcrun notarytool submit "$bundle/$dmg" --wait \
  --key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY" --issuer "$APPLE_API_ISSUER" 2>&1)
echo "$notarized"
echo "$notarized" | grep -q "status: Accepted"
xcrun stapler staple "$bundle/$dmg"
spctl --assess --type open --context context:primary-signature -v "$bundle/$dmg"
# The checksum the updater checks is taken of the stapled image.
(cd "$bundle" && shasum -a 256 "$dmg" > SHA256SUMS.txt)

# Notes list the commits since the previous Owl release.
# Upstream's tags share these names, so the commit comes from the release.
previous=$(gh release list -R "$repo" --limit 1 --json tagName -q '.[0].tagName' 2> /dev/null || true)
since=""
[ -n "$previous" ] && since=$(gh release view "$previous" -R "$repo" --json targetCommitish -q .targetCommitish)
if [ -n "$since" ] && git rev-parse -q --verify "$since^{commit}" > /dev/null; then
  notes=$(git log --no-merges --pretty='- %s' "$since..$commit")
else
  notes=$(git log --no-merges --pretty='- %s' -20 "$commit")
fi
gh release create "$tag" -R "$repo" "$bundle/$dmg" "$bundle/SHA256SUMS.txt" \
  --target "$commit" --title "Owl $version" --notes "$notes"
echo "Published $tag on $repo"
