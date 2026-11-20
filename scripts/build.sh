#!/usr/bin/env bash
# Builds a distributable ReelMaker bundle for the host OS (Linux or macOS).
#
#   Linux : needs libmpv-dev, pkg-config, patchelf, curl, xz-utils (+ the GUI build deps, see README).
#   macOS : needs Homebrew with `mpv ffmpeg dylibbundler`.
#
# Output: dist/reelmaker-<os>-<arch>.tar.gz (Linux) or .zip (macOS), containing the app, libmpv and its
# non-system dependencies, and ffmpeg/ffprobe. The ffmpeg builds are GPL (libx264): fine for personal
# use, review before public distribution.
#
# NOTE: this script has not been run by its author on Linux/macOS; CI (.github/workflows/release.yml)
# is the intended way to produce and verify those artifacts.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OS="$(uname -s)"
ARCH="$(uname -m)"
VENDOR="$ROOT/vendor"
DIST="$ROOT/dist"
mkdir -p "$VENDOR" "$DIST"

case "$OS" in
  Linux)  NAME="reelmaker-linux-$ARCH" ;;
  Darwin) NAME="reelmaker-macos-$ARCH" ;;
  *) echo "unsupported OS: $OS (use scripts/build.ps1 on Windows)" >&2; exit 1 ;;
esac
BUNDLE="$DIST/$NAME"
rm -rf "$BUNDLE"; mkdir -p "$BUNDLE"

need() { command -v "$1" >/dev/null 2>&1 || { echo "missing required tool: $1 ($2)" >&2; exit 1; }; }
need cargo "install Rust from https://rustup.rs"

# ---------------------------------------------------------------- Linux
build_linux() {
  need pkg-config "apt install pkg-config"; need patchelf "apt install patchelf"; need curl "apt install curl"
  pkg-config --exists mpv || { echo "libmpv not found: apt install libmpv-dev" >&2; exit 1; }

  local ff="$VENDOR/ffmpeg-linux"
  if [ ! -x "$ff/ffmpeg" ]; then
    local asset="linux64"; [ "$ARCH" = "aarch64" ] && asset="linuxarm64"
    mkdir -p "$ff"
    curl -fL "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-$asset-gpl.tar.xz" -o "$VENDOR/ffmpeg.tar.xz"
    tar -xf "$VENDOR/ffmpeg.tar.xz" -C "$ff" --strip-components=2 --wildcards '*/bin/ffmpeg' '*/bin/ffprobe'
  fi

  (cd "$ROOT" && cargo build --release)
  cp "$ROOT/target/release/reelmaker" "$BUNDLE/"
  cp "$ff/ffmpeg" "$ff/ffprobe" "$BUNDLE/"

  # Copy libmpv and its non-system shared-library dependencies next to the executable.
  local mpv; mpv="$(ldd "$BUNDLE/reelmaker" | awk '/libmpv/ {print $3}')"
  [ -n "$mpv" ] || { echo "reelmaker does not link libmpv?" >&2; exit 1; }
  ldd "$mpv" "$BUNDLE/reelmaker" | awk '/=> \// {print $3}' | sort -u | while read -r lib; do
    case "$(basename "$lib")" in
      # system / driver provided: must come from the user's machine
      libc.so*|libm.so*|libdl.so*|libpthread.so*|librt.so*|libutil.so*|ld-linux*|libresolv*|libnsl*) continue ;;
      libGL.so*|libGLX*|libEGL*|libOpenGL*|libGLdispatch*|libdrm*|libgbm*|libvulkan*) continue ;;
      libX11*|libxcb*|libXau*|libXdmcp*|libXext*|libXrandr*|libXi*|libXcursor*|libxkbcommon*|libwayland*) continue ;;
      libasound*|libpulse*|libstdc++*|libgcc_s*) continue ;;
    esac
    cp -n "$lib" "$BUNDLE/" 2>/dev/null || true
  done
  # Bundled libs must find each other beside themselves.
  for f in "$BUNDLE"/*.so*; do [ -e "$f" ] && patchelf --set-rpath '$ORIGIN' "$f" || true; done
  patchelf --set-rpath '$ORIGIN' "$BUNDLE/reelmaker"
  cp "$ROOT/README.md" "$BUNDLE/"

  tar -C "$DIST" -czf "$DIST/$NAME.tar.gz" "$NAME"
  echo "Created $DIST/$NAME.tar.gz"
  smoke "$DIST/$NAME.tar.gz"
}

# ---------------------------------------------------------------- macOS
build_macos() {
  need brew "https://brew.sh"; need dylibbundler "brew install dylibbundler"
  local mpvprefix; mpvprefix="$(brew --prefix mpv)"
  export LIBRARY_PATH="$mpvprefix/lib:${LIBRARY_PATH:-}"
  export RUSTFLAGS="-L $mpvprefix/lib ${RUSTFLAGS:-}"

  (cd "$ROOT" && cargo build --release)
  cp "$ROOT/target/release/reelmaker" "$BUNDLE/"
  cp "$(brew --prefix ffmpeg)/bin/ffmpeg" "$(brew --prefix ffmpeg)/bin/ffprobe" "$BUNDLE/"
  chmod u+w "$BUNDLE"/*

  # Pull every non-system dylib into libs/ and rewrite install names to @executable_path/libs.
  mkdir -p "$BUNDLE/libs"
  dylibbundler -of -b -cd -d "$BUNDLE/libs" -p "@executable_path/libs/" \
    -x "$BUNDLE/reelmaker" -x "$BUNDLE/ffmpeg" -x "$BUNDLE/ffprobe"
  cp "$ROOT/README.md" "$BUNDLE/"

  (cd "$DIST" && zip -qr "$NAME.zip" "$NAME")
  echo "Created $DIST/$NAME.zip"
  smoke "$DIST/$NAME.zip"
}

# Unpack the artifact somewhere else and make sure the bundled exe starts without the build tree.
smoke() {
  local archive="$1" tmp; tmp="$(mktemp -d)"
  case "$archive" in *.zip) unzip -q "$archive" -d "$tmp" ;; *) tar -xzf "$archive" -C "$tmp" ;; esac
  local out; out="$(env -i PATH=/usr/bin:/bin "$tmp/$NAME/reelmaker" --version)"
  case "$out" in reelmaker*) echo "Smoke test OK: $out" ;; *) echo "smoke test failed: '$out'" >&2; exit 1 ;; esac
  env -i PATH=/usr/bin:/bin "$tmp/$NAME/ffmpeg" -version | head -1
  rm -rf "$tmp"
}

case "$OS" in Linux) build_linux ;; Darwin) build_macos ;; esac
