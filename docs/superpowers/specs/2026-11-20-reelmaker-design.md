# ReelMaker – Design

Date: 2026-11-20. Status: awaiting approval.
Reference UI: `mockup.html` (layout approved by user; behaviour below is derived from it).

## Intent
Turn a landscape video into a 9:16 reel (a 15/30/60s clip) quickly, on the desktop, from any video file. User picks the clip by sliding a fixed-length window along the timeline, picks how the landscape frame fills the vertical canvas, and exports.

Why native + ffmpeg: user wants performance, any input format, and a battle-tested media engine, shipped as a native executable on Windows, macOS and Linux.

## Goals
- Import any file ffmpeg can read.
- Play/scrub video with a live 9:16 preview that matches the export geometry.
- Sliding window on the timeline; length presets Short 15s / Medium 30s / Long 60s.
- Two output styles: **Blur bars** (full frame centred, blurred copy fills top/bottom) and **Zoom & crop** (crop to 9:16 and fill; horizontal reframe by dragging the crop box, zoom slider 100–200%).
- Export 1080x1920 MP4 (H.264 + AAC, 30fps) with a progress bar.
- One build pipeline producing artifacts for Windows, macOS, Linux.

## Non-goals (v1)
- Preview audio (export keeps audio; the player is silent in v1).
- Keyframed/moving reframe (reframe is one fixed x per reel).
- Custom window length, edge-resizing, captions, multiple reels per project, batch export, project save files.

## Anti-goals
- Must not need the user to install ffmpeg manually in released builds.
- Must not re-encode or modify the source file.
- A preview that visibly disagrees with the export framing (blur look may be approximate; geometry must be exact).

## Constraints
- Language: Rust. UI: `egui`/`eframe` (builder's call on exact crate versions).
- Media engine: ffmpeg + ffprobe run as child processes (sidecar binaries), no linking against libav. This keeps packaging to "one extra binary per OS".
- Released ffmpeg builds are GPL (libx264): fine for personal use, flag before public distribution.
- Cross-compiling GUI apps is painful: official builds come from GitHub Actions native runners (windows, macos, ubuntu), not local cross-compilation.

## Approaches considered
Shell: Electron (reuses mockup, big, user declined, wants native) / C# WPF (Windows-only) / **Rust + egui (chosen)**.
Playback: libmpv embedded (live filter-graph preview, audio, but per-OS shared-lib packaging) / **ffmpeg decodes frames to the UI (chosen: only ffmpeg binary to ship; costs us seek/sync code and v1 has no preview audio)**.
User chose Rust and the ffmpeg-frames playback.

## Design

### Components
1. **Probe** – `ffprobe -show_streams -show_format -of json` → duration, w/h, fps, rotation.
2. **Frame source** – spawns `ffmpeg -ss <t> -i <file> -vf scale=<preview w>:-2 -r <fps> -f rawvideo -pix_fmt rgb24 pipe:1`, reads fixed-size frames on a thread, hands them to the UI through a channel. Playback is paced by a wall clock. Seek/scrub = kill and respawn at the new time (debounced while dragging). Paused scrub shows a single frame.
3. **Timeline thumbnails** – one ffmpeg call at import generating ~24 evenly spaced small frames (`fps=N/duration`, tiled or separate), drawn behind the window.
4. **Preview renderer** – shows the landscape frame in the main stage, plus the 9:16 live preview built from the same frame: Blur bars = background fill (downscaled, blurred on CPU at small size, or equivalent) + centred frame; Zoom & crop = sub-rect of the frame from `cropX` and `zoom`. Geometry maths must match the export filter graph exactly.
5. **Window model** – `{start, length}` with `length ∈ {15,30,60}` clamped to video duration; drag moves `start`, clamped to `[0, duration-length]`. Loop-window plays only the selection.
6. **Exporter** – one ffmpeg run, progress read from `-progress pipe:1` (`out_time_us` / window length), cancellable (kill child), writes to a user-chosen path. Does not overwrite without confirmation.
7. **Build/packaging** – `.github/workflows/release.yml`: matrix windows/macos/ubuntu; `cargo build --release`; download a static ffmpeg+ffprobe for that OS into the bundle; zip (Windows .zip containing exe, macOS .app/.zip, Linux tar.gz). Plus local scripts `scripts/build.ps1` and `scripts/build.sh` that build for the host OS and fetch ffmpeg. Binary lookup order: next to the exe → `PATH`.

### Export filter graphs (1080x1920, window `-ss start -t len`)
- Blur bars:
  `[0:v]split[a][b];[a]scale=1080:1920:force_original_aspect_ratio=increase,crop=1080:1920,boxblur=<s>:5[bg];[b]scale=1080:-2[fg];[bg][fg]overlay=(W-w)/2:(H-h)/2`
- Zoom & crop (z = zoom factor, cropX in 0..1 = centre of crop box over source width):
  `crop=w=ih*9/16/z:h=ih/z:x=clip(cropX*iw-w/2,0,iw-w):y=(ih-h)/2,scale=1080:1920`
- Codec: `-c:v libx264 -crf 18 -preset medium -pix_fmt yuv420p -r 30 -c:a aac -b:a 160k -movflags +faststart`.

### UI (from mockup, unchanged)
Header (filename, Import, Export). Left: stage with 9:16 crop box (zoom mode only), transport (Play, Loop window, time), timeline with window + presets. Right: Output style card (mode, blur or zoom slider), live 9:16 preview, export summary. Mockup's "Use demo clip" button is dev-only and is dropped.

### Errors
Unreadable file → message, stay on empty state. Missing ffmpeg → blocking dialog naming the path searched. Video shorter than chosen preset → window = whole video. Export failure → show ffmpeg's last stderr lines.

## Decided by user
Rust; native; ffmpeg as the engine; frame-pipe playback; build scripts for Windows/macOS/Linux; mockup layout; repo `github.com/elderly-steamboat/reelmaker`.

## My calls (not yet confirmed by user)
- egui/eframe as the UI toolkit.
- Fixed presets 15/30/60s, fixed reframe, silent preview, CRF 18 H.264/AAC, 1080x1920@30.
- ffmpeg as sidecar processes, not linked libraries.
- CI-built releases via GitHub Actions; ffmpeg static builds fetched at build time.

## Left to the builder
Crate choices and versions, frame pacing/seek debouncing details, how to render the blur preview cheaply, thumbnail extraction details, exact CI download sources and macOS signing (unsigned in v1), file-dialog crate, project layout.
