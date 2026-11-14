#!/usr/bin/env bash
# Builds Owl for macOS and publishes it as a GitHub release of the fork, the
# channel installed Owl apps update from (`RELEASES_REPO` in
# src-tauri/src/app_auto_updater.rs). An app checks at start and every three
# hours, downloads the .dmg of its own architecture, checks it against
# SHA256SUMS.txt and offers to restart into the new version.
#
# A release carries two disk images: Owl_<version>_aarch64.dmg for Apple
# Silicon and Owl_<version>_x64.dmg for Intel Macs, the x64 one cross-compiled
# on Apple Silicon (`x86_64-apple-darwin`).
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
# src-tauri/tauri.conf.json, commit and push, then have GitHub's macOS runners
# build both images side by side and publish them
# (.github/workflows/release-macos.yml, signing material from the repository's
# secrets):
#
#   gh workflow run release-macos.yml -R igorao79/antidetect
#
# or run it on this Mac, which builds both one after the other:
#
#   scripts/release-macos.sh                 both builds, then publish
#   scripts/release-macos.sh build <target>  one signed, notarized image in release-out/
#   scripts/release-macos.sh publish         release what release-out/ holds
set -euo pipefail
cd "$(dirname "$0")/.."

TARGETS="aarch64-apple-darwin x86_64-apple-darwin"
OUT_DIR=release-out

version=$(node -p 'require("./src-tauri/tauri.conf.json").version')
# The repository the app reads; `gh` would otherwise pick upstream's.
repo=$(sed -n 's/^const RELEASES_REPO: &str = "\(.*\)";/\1/p' src-tauri/src/app_auto_updater.rs)
test -n "$repo"
tag="v$version"
commit=$(git rev-parse HEAD)

# Tauri names an image by its own architecture word, the updater looks for it.
dmg_arch() {
  case "$1" in
    aarch64-apple-darwin) echo aarch64 ;;
    x86_64-apple-darwin) echo x64 ;;
    *) echo "Unknown target $1 (one of: $TARGETS)." >&2; return 1 ;;
  esac
}

# The architecture as `lipo` and `arch` name it.
cpu_arch() {
  case "$1" in
    aarch64-apple-darwin) echo arm64 ;;
    x86_64-apple-darwin) echo x86_64 ;;
  esac
}

ensure_releasable() {
  if [ -n "$(git status --porcelain --untracked-files=no)" ]; then
    echo "Commit the working tree first: a release is built from a pushed commit." >&2
    exit 1
  fi
  if ! git branch -r --contains "$commit" | grep -q .; then
    echo "Push $commit first." >&2
    exit 1
  fi
  if gh release view "$tag" -R "$repo" > /dev/null 2>&1; then
    echo "$tag is already released; bump the version first." >&2
    exit 1
  fi
}

load_signing_env() {
  local release_env="$HOME/.config/owl/release.env"
  # shellcheck disable=SC1090
  [ -f "$release_env" ] && set -a && . "$release_env" && set +a
  local name
  for name in APPLE_SIGNING_IDENTITY APPLE_API_ISSUER APPLE_API_KEY APPLE_API_KEY_PATH; do
    if [ -z "${!name:-}" ]; then
      echo "$name is not set (see $release_env): a release must be signed and notarized." >&2
      exit 1
    fi
  done
  test -f "$APPLE_API_KEY_PATH"
}

# The sidecar must name the release; on GitHub's runner build.rs would
# otherwise take GITHUB_REF_NAME, the branch. A binary for another CPU runs
# only where Rosetta does, so without it the tag is looked for in the file.
check_proxy_version() {
  local proxy="$1" cpu="$2"
  local archs
  archs=$(lipo -archs "$proxy")
  if [ "$archs" != "$cpu" ]; then
    echo "$proxy is built for $archs, not $cpu." >&2
    exit 1
  fi
  if arch "-$cpu" /usr/bin/true 2> /dev/null; then
    local proxy_version
    proxy_version=$("$proxy" --version)
    if [ "$proxy_version" != "donut-proxy $tag" ]; then
      echo "The build calls itself \"$proxy_version\", not $tag." >&2
      exit 1
    fi
  elif ! grep -aqF "$tag" "$proxy"; then
    echo "$proxy does not carry $tag." >&2
    exit 1
  fi
}

# Tauri signs and notarizes the app and staples its ticket before it builds
# the disk image; the image is signed and notarized as well, so a downloaded
# copy opens without a warning before it is even mounted.
notarize() {
  local app="$1" image="$2"
  codesign --verify --deep --strict "$app"
  local signed_by
  signed_by=$(codesign -dvv "$app" 2>&1)
  grep -q "Authority=Developer ID Application" <<< "$signed_by"
  # Right after Apple accepts the app its ticket is not always served yet,
  # and Tauri does not report a staple that found none, so it is stapled
  # again here.
  local attempt
  for attempt in 1 2 3 4 5; do
    xcrun stapler validate "$app" > /dev/null 2>&1 && break
    [ "$attempt" = 5 ] && { xcrun stapler validate "$app"; exit 1; }
    sleep 20
    xcrun stapler staple "$app" || true
  done
  codesign --force --timestamp --sign "$APPLE_SIGNING_IDENTITY" "$image"
  local notarized
  notarized=$(xcrun notarytool submit "$image" --wait \
    --key "$APPLE_API_KEY_PATH" --key-id "$APPLE_API_KEY" --issuer "$APPLE_API_ISSUER" 2>&1)
  echo "$notarized"
  echo "$notarized" | grep -q "status: Accepted"
  # Apple's ticket service drops a connection now and then (0.33.8's first
  # run lost its build to one), so a failed staple is tried again.
  for attempt in 1 2 3 4 5; do
    xcrun stapler staple "$image" && break
    [ "$attempt" = 5 ] && exit 1
    sleep 20
  done
  spctl --assess --type open --context context:primary-signature -v "$image"
}

build_target() {
  local target="$1"
  local arch cpu host out
  arch=$(dmg_arch "$target")
  cpu=$(cpu_arch "$target")
  host=$(rustc -vV | sed -n 's/^host: //p')
  # The host's own build stays in target/release, where the cached
  # dependencies are; `--target` would compile them all again elsewhere.
  local target_args=()
  out=src-tauri/target/release
  if [ "$target" != "$host" ]; then
    target_args=(--target "$target")
    out="src-tauri/target/$target/release"
  fi
  # STABLE_RELEASE makes the build a stable one, which takes updates; without
  # it a build calls itself dev-<version> and never updates. BUILD_TAG names
  # the version outright. TARGET tells the sidecar build (before Tauri's own).
  env -u HTTP_PROXY -u HTTPS_PROXY -u http_proxy -u https_proxy -u ALL_PROXY -u all_proxy \
    STABLE_RELEASE=1 BUILD_TAG="$tag" CARGO_INCREMENTAL=0 TARGET="$target" \
    pnpm tauri build ${target_args[@]+"${target_args[@]}"}
  check_proxy_version "$out/donut-proxy" "$cpu"

  local dmg="Owl_${version}_${arch}.dmg"
  local image="$out/bundle/dmg/$dmg"
  test -f "$image"
  notarize "$out/bundle/macos/Owl.app" "$image"
  mkdir -p "$OUT_DIR"
  cp "$image" "$OUT_DIR/$dmg"
  echo "Built $OUT_DIR/$dmg"
}

publish() {
  local target images=()
  for target in $TARGETS; do
    local image="$OUT_DIR/Owl_${version}_$(dmg_arch "$target").dmg"
    if [ ! -f "$image" ]; then
      echo "$image is missing: a release carries every architecture." >&2
      exit 1
    fi
    images+=("$image")
  done
  # The checksums the updater checks, of the stapled images.
  (
    cd "$OUT_DIR"
    if command -v sha256sum > /dev/null; then
      sha256sum Owl_"${version}"_*.dmg
    else
      shasum -a 256 Owl_"${version}"_*.dmg
    fi
  ) > "$OUT_DIR/SHA256SUMS.txt"

  # Notes list the commits since the previous Owl release.
  # Upstream's tags share these names, so the commit comes from the release.
  local previous since="" notes
  previous=$(gh release list -R "$repo" --limit 1 --json tagName -q '.[0].tagName' 2> /dev/null || true)
  [ -n "$previous" ] && since=$(gh release view "$previous" -R "$repo" --json targetCommitish -q .targetCommitish)
  if [ -n "$since" ] && git rev-parse -q --verify "$since^{commit}" > /dev/null; then
    notes=$(git log --no-merges --pretty='- %s' "$since..$commit")
  else
    notes=$(git log --no-merges --pretty='- %s' -20 "$commit")
  fi
  gh release create "$tag" -R "$repo" "${images[@]}" "$OUT_DIR/SHA256SUMS.txt" \
    --target "$commit" --title "Owl $version" --notes "$notes"
  echo "Published $tag on $repo"
}

mode="${1:-all}"
case "$mode" in
  build)
    target="${2:?Name the target: one of $TARGETS}"
    dmg_arch "$target" > /dev/null
    ensure_releasable
    load_signing_env
    build_target "$target"
    ;;
  publish)
    ensure_releasable
    publish
    ;;
  all)
    ensure_releasable
    load_signing_env
    rm -rf "$OUT_DIR"
    for target in $TARGETS; do
      build_target "$target"
    done
    publish
    ;;
  *)
    echo "Usage: $0 [build <target> | publish]" >&2
    exit 1
    ;;
esac
