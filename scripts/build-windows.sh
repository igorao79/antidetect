#!/usr/bin/env bash
# Builds Owl's Windows installer (NSIS, x64) on this Mac.
#
# Upstream builds Windows natively on a windows-latest runner
# (`tauri build --target x86_64-pc-windows-msvc`); the fork has no Windows
# machine, so this cross-compiles the same target with cargo-xwin (the MSVC
# CRT and Windows SDK, clang-cl and lld-link) and bundles with makensis, the
# way Tauri documents for building Windows apps elsewhere. The installer is
# not code-signed, so SmartScreen warns on first run.
#
# The build is large and the internal disk is small, so the target directory
# and the SDK live on an APFS disk image (exFAT, the external disk's own
# format, has no symlinks, which cargo-xwin needs):
#   OWL_WIN_IMAGE  image path, created on first run
#                  (default /Volumes/Transcend/owl-windows-build.sparsebundle)
#   OWL_WIN_OUT    where the installer is copied (default /Volumes/Transcend/owl-windows)
# STABLE_RELEASE=1 names the build v<version> instead of dev-<version>.
# Extra arguments go to `tauri build`.
#
# Needs: brew install makensis llvm@22 lld@22; cargo install cargo-xwin --locked;
# rustup target add x86_64-pc-windows-msvc.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TARGET="x86_64-pc-windows-msvc"
IMAGE="${OWL_WIN_IMAGE:-/Volumes/Transcend/owl-windows-build.sparsebundle}"
VOLUME="/Volumes/OwlWinBuild"
OUT="${OWL_WIN_OUT:-/Volumes/Transcend/owl-windows}"
IMAGE_SIZE="150g"

fail() {
  echo "build-windows: $*" >&2
  exit 1
}

command -v makensis >/dev/null || fail "makensis is missing (brew install makensis)"
command -v cargo-xwin >/dev/null || fail "cargo-xwin is missing (cargo install cargo-xwin --locked)"
rustup target list --installed | grep -qx "$TARGET" ||
  fail "Rust target missing (rustup target add $TARGET)"

LLVM_BIN="$(brew --prefix llvm@22)/bin"
LLD_BIN="$(brew --prefix lld@22)/bin"
[ -x "$LLVM_BIN/clang-cl" ] || fail "clang-cl is missing (brew install llvm@22)"
[ -x "$LLD_BIN/lld-link" ] || fail "lld-link is missing (brew install lld@22)"

if [ ! -d "$VOLUME" ]; then
  [ -d "$(dirname "$IMAGE")" ] || fail "$(dirname "$IMAGE") is not mounted"
  if [ ! -d "$IMAGE" ]; then
    hdiutil create -size "$IMAGE_SIZE" -type SPARSEBUNDLE -fs APFS \
      -volname OwlWinBuild "$IMAGE" >/dev/null
  fi
  hdiutil attach "$IMAGE" -nobrowse >/dev/null
fi
[ -d "$VOLUME" ] || fail "build image did not mount at $VOLUME"

export CARGO_TARGET_DIR="$VOLUME/target"
export XWIN_CACHE_DIR="$VOLUME/xwin-cache"
export XWIN_ACCEPT_LICENSE=1
export PATH="$LLVM_BIN:$LLD_BIN:$PATH"
# makensis 3.13 dies with std::bad_alloc on its first Exch under the C locale.
export LC_ALL="en_US.UTF-8" LANG="en_US.UTF-8"
# The app's own lookups would go through a proxy set here (see AGENTS.md).
unset HTTP_PROXY HTTPS_PROXY http_proxy https_proxy ALL_PROXY all_proxy

XWIN_ENV="$(cargo xwin env --target "$TARGET")" || fail "cargo xwin env failed"
eval "$XWIN_ENV"

cd "$ROOT"
pnpm tauri build --target "$TARGET" --bundles nsis "$@"

BUNDLE_DIR="$CARGO_TARGET_DIR/$TARGET/release/bundle/nsis"
INSTALLER="$(ls -t "$BUNDLE_DIR"/*-setup.exe 2>/dev/null | head -1 || true)"
[ -n "$INSTALLER" ] || fail "no installer in $BUNDLE_DIR"

mkdir -p "$OUT"
cp "$INSTALLER" "$OUT/"
COPY="$OUT/$(basename "$INSTALLER")"
echo "Installer: $COPY"
shasum -a 256 "$COPY"
