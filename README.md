# ReelMaker

Turn a landscape video into a 9:16 reel. Import any video, slide a 15 / 30 / 60 second window along the timeline, pick how the frame fills the vertical canvas, and export a 1080×1920 MP4.

- **Blur bars** – the full frame stays centred; a blurred copy fills the top and bottom.
- **Zoom & crop** – crop to 9:16 (drag the box to reframe, zoom 100–200%).
- The live 9:16 preview runs the **same ffmpeg filter graph as the export**, so what you see is what you get.

Native Rust app: [egui](https://github.com/emilk/egui) UI, [libmpv](https://mpv.io) playback, ffmpeg/ffprobe for probing and export.

`mockup.html` is the original UI sketch (kept for reference). Design and plan live in `docs/superpowers/`.

## Using it

Run `reelmaker` (Windows: `reelmaker.exe`). **Import video** or drop a file on the window, choose a window length and output style, then **Export reel**. You can also pass a file on the command line: `reelmaker clip.mp4`.

Release bundles contain everything needed (the app, libmpv, ffmpeg and ffprobe). A bundle is large (roughly 450 MB unpacked on Windows) because it carries two static ffmpeg builds plus libmpv.

> The bundled ffmpeg is a **GPL** build (libx264). That is fine for personal use; review the licensing before distributing builds publicly. Builds are **unsigned**, so Windows SmartScreen and macOS Gatekeeper will warn on first run.

## Building

### Windows

Prerequisites: Rust (MSVC toolchain) and Visual Studio Build Tools with the C++ workload.

```powershell
./scripts/build.ps1
```

This downloads libmpv and ffmpeg into `vendor/` (git-ignored), generates the MSVC import library `mpv.lib`, builds in release mode, and writes `dist/reelmaker-windows-x86_64.zip`. It then unpacks the zip and runs the bundled exe with `--version` as a smoke test.

For development, `cargo run` / `cargo test` work once `vendor/mpv-dev` exists (run the script once, or set `MPV_LIB_DIR` to a folder with `mpv.lib` and `libmpv-2.dll`) and `ffmpeg`/`ffprobe` are on `PATH`. A small cargo runner (`scripts/runner.cmd`) puts the libmpv DLL on `PATH` for you.

### Linux

The Linux bundle is built on Ubuntu 24.04 (the `libmpv2` crate needs libmpv client API 2.x, i.e. mpv ≥ 0.33, which Ubuntu 22.04's libmpv-dev lacks), so it needs **glibc ≥ 2.39** (Ubuntu 24.04+, Fedora 40+, Debian 13+).

```bash
sudo apt install libmpv-dev ffmpeg patchelf pkg-config xz-utils libgtk-3-dev libxkbcommon-dev \
  libwayland-dev libx11-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libssl-dev
./scripts/build.sh      # -> dist/reelmaker-linux-<arch>.tar.gz
```

### macOS

```bash
brew install mpv ffmpeg dylibbundler
./scripts/build.sh      # -> dist/reelmaker-macos-<arch>.zip
```

The Linux and macOS scripts are best-effort and verified through CI (`.github/workflows/release.yml`), which builds all three platforms on native runners and attaches the artifacts to a GitHub release when a `v*` tag is pushed.

## Tests

```bash
cargo test
```

Needs `ffmpeg` and `ffprobe` on `PATH` (tests generate small clips) and libmpv available as described above.
