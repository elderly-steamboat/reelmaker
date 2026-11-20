# ReelMaker – Design

Date: 2026-11-20. Status: awaiting approval.
Reference UI: `mockup.html` (layout approved by user; behaviour below is derived from it).

## Intent
Turn a landscape video into a 9:16 reel (a 15/30/60s clip) quickly, on the desktop, from any video file. User picks the clip by sliding a fixed-length window along the timeline, picks how the landscape frame fills the vertical canvas, and exports.

Why native + ffmpeg: user wants performance, any input format, and a battle-tested media engine, shipped as a native executable on Windows, macOS and Linux.

## Goals
- Import any file ffmpeg can read.
- Play/scrub video (with audio) with a live 9:16 preview that runs the same filter graph as the export, so preview = output.
- Sliding window on the timeline; length presets Short 15s / Medium 30s / Long 60s.
- Two output styles: **Blur bars** (full frame centred, blurred copy fills top/bottom) and **Zoom & crop** (crop to 9:16 and fill; horizontal reframe by dragging the crop box, zoom slider 100–200%).
- Export 1080x1920 MP4 (H.264 + AAC, 30fps) with a progress bar.
- One build pipeline producing artifacts for Windows, macOS, Linux.

## Non-goals (v1)
- Keyframed/moving reframe (reframe is one fixed x per reel).
- Custom window length, edge-resizing, captions, multiple reels per project, batch export, project save files.

## Anti-goals
- Must not need the user to install ffmpeg manually in released builds.
- Must not re-encode or modify the source file.
- A preview that visibly disagrees with the export (it must use the same filter graph).

## Constraints
- Language: Rust. UI: `egui`/`eframe` (builder's call on exact crate versions).
- Playback: libmpv (shared library, loaded at runtime, rendered through mpv's OpenGL render API into egui). Probe, thumbnails and export: ffmpeg + ffprobe as child processes (sidecar binaries), no linking against libav.
- Released ffmpeg builds are GPL (libx264): fine for personal use, flag before public distribution.
- Cross-compiling GUI apps with native libs is painful: official builds come from GitHub Actions native runners (windows, macos, ubuntu), not local cross-compilation. Each OS bundles its own libmpv (`mpv-2.dll` / `libmpv.dylib` / `libmpv.so`).

## Approaches considered
Shell: Electron (reuses mockup, big, user declined, wants native) / C# WPF (Windows-only) / **Rust + egui (chosen)**.
Playback: **libmpv embedded (chosen: plays any format with audio, runs the export filter graph live, handles seek/sync; costs per-OS shared-lib packaging)** / ffmpeg frame-pipe (rejected: simpler packaging but we would build seek, A/V sync and lose preview audio).
User chose Rust and libmpv playback (first picked the frame-pipe, then switched to libmpv).

## Design

### Components
1. **Probe** – `ffprobe -show_streams -show_format -of json` → duration, w/h, fps, rotation.
2. **Player** – libmpv handle(s) created via `libmpv` bindings. Play/pause, seek, loop `ab-loop` over the window, audio on. Frames are drawn into egui through mpv's OpenGL render context (FBO → texture). Scrub = `seek` (mpv handles it).
3. **Timeline thumbnails** – one ffmpeg call at import generating ~24 evenly spaced small frames (`fps=N/duration`, tiled or separate), drawn behind the window.
4. **Preview renderer** – two views of the same media: the landscape stage (plain playback) and the 9:16 preview, which is a second mpv instance kept in sync (same time, play state) with the export filter graph applied via `vf`/`lavfi-complex` at preview resolution. Blur strength, mode, zoom and cropX changes update that filter live. Builder may instead use one mpv and render the 9:16 view from the same texture if the graph can be reproduced exactly.
5. **Window model** – `{start, length}` with `length ∈ {15,30,60}` clamped to video duration; drag moves `start`, clamped to `[0, duration-length]`. Loop-window plays only the selection.
6. **Exporter** – one ffmpeg run, progress read from `-progress pipe:1` (`out_time_us` / window length), cancellable (kill child), writes to a user-chosen path. Does not overwrite without confirmation.
7. **Build/packaging** – `.github/workflows/release.yml`: matrix windows/macos/ubuntu; `cargo build --release`; fetch ffmpeg+ffprobe and libmpv for that OS into the bundle; zip (Windows .zip containing exe, macOS .app/.zip, Linux tar.gz). Plus local scripts `scripts/build.ps1` and `scripts/build.sh` that build for the host OS and fetch ffmpeg. Lookup order for ffmpeg/ffprobe/libmpv: next to the exe → system (`PATH` / standard lib dirs).

### Export filter graphs (1080x1920, window `-ss start -t len`)
- Blur bars:
  `[0:v]split[a][b];[a]scale=1080:1920:force_original_aspect_ratio=increase,crop=1080:1920,boxblur=<s>:5[bg];[b]scale=1080:-2[fg];[bg][fg]overlay=(W-w)/2:(H-h)/2`
- Zoom & crop (z = zoom factor, cropX in 0..1 = centre of crop box over source width):
  `crop=w=ih*9/16/z:h=ih/z:x=clip(cropX*iw-w/2,0,iw-w):y=(ih-h)/2,scale=1080:1920`
- Codec: `-c:v libx264 -crf 18 -preset medium -pix_fmt yuv420p -r 30 -c:a aac -b:a 160k -movflags +faststart`.

### UI (from mockup, unchanged)
Header (filename, Import, Export). Left: stage with 9:16 crop box (zoom mode only), transport (Play, Loop window, time), timeline with window + presets. Right: Output style card (mode, blur or zoom slider), live 9:16 preview, export summary. Mockup's "Use demo clip" button is dev-only and is dropped.

### Errors
Unreadable file → message, stay on empty state. Missing ffmpeg or libmpv → blocking dialog naming what was searched. Video shorter than chosen preset → window = whole video. Export failure → show ffmpeg's last stderr lines.

## Decided by user
Rust; native; ffmpeg for probe/export; libmpv for playback; build scripts for Windows/macOS/Linux; mockup layout; repo `github.com/elderly-steamboat/reelmaker`.

## My calls (not yet confirmed by user)
- egui/eframe as the UI toolkit.
- Fixed presets 15/30/60s, fixed reframe, CRF 18 H.264/AAC, 1080x1920@30.
- ffmpeg/ffprobe as sidecar processes (not linked); libmpv loaded as a shared library.
- CI-built releases via GitHub Actions; ffmpeg and libmpv builds fetched at build time.

## Left to the builder
Crate choices and versions, mpv GL render-context wiring inside egui, one vs two mpv instances for the 9:16 view, thumbnail extraction details, exact CI download sources for ffmpeg/libmpv and macOS signing (unsigned in v1), file-dialog crate, project layout.
