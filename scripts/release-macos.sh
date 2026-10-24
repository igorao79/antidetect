#!/usr/bin/env bash
# Builds Owl for macOS and publishes it as a GitHub release of the fork, the
# channel installed Owl apps update from (`RELEASES_REPO` in
# src-tauri/src/app_auto_updater.rs). An app checks at start and every three
# hours, downloads the .dmg, checks it against SHA256SUMS.txt and offers to
# restart into the new version.
#
# Bump the version in package.json, src-tauri/Cargo.toml and
# src-tauri/tauri.conf.json, commit and push, then run:
#
#   scripts/release-macos.sh
set -euo pipefail
cd "$(dirname "$0")/.."

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

# STABLE_RELEASE makes the build a `v<version>` one, which takes updates;
# without it a build calls itself dev-<version> and never updates.
env -u HTTP_PROXY -u HTTPS_PROXY -u http_proxy -u https_proxy -u ALL_PROXY -u all_proxy \
  STABLE_RELEASE=1 CARGO_INCREMENTAL=0 pnpm tauri build

bundle=src-tauri/target/release/bundle/dmg
dmg="Owl_${version}_aarch64.dmg"
test -f "$bundle/$dmg"
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
