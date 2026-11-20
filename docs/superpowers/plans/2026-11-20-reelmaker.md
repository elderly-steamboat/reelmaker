# ReelMaker Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Native Rust desktop app that turns a landscape video into a 9:16 reel (15/30/60s window, blur-bars or zoom-crop) and exports it with ffmpeg.

**Architecture:** One Rust crate. Pure-logic modules (window, filters, probe, tools, export arg-building, timeline geometry, app state) are unit-tested; ffmpeg/ffprobe run as child processes and are covered by integration tests against generated clips; libmpv (via `libmpv2` + its OpenGL render API) plays the video inside an `eframe`/`egui` window. One function in `filters.rs` builds the 9:16 filter graph and is used by both the exporter and the live preview, so preview = output.

**Tech Stack:** Rust (stable), `eframe`/`egui` (glow backend), `libmpv2`, `serde_json`, `rfd` (file dialogs), `anyhow`/`thiserror`, `tempfile` (tests). ffmpeg + ffprobe on `PATH` for dev and tests.

**Spec:** `docs/superpowers/specs/2026-11-20-reelmaker-design.md` (UI reference: `mockup.html`)

## Global Constraints

- Language Rust; UI `egui`/`eframe`.
- Playback is libmpv; probe, thumbnails, export are ffmpeg/ffprobe child processes, never linked libav.
- Window presets exactly Short 15s / Medium 30s / Long 60s; window length clamped to video duration.
- Blur strength 4–40 (default 22); zoom 100–200% (default 100%); cropX 0..1 (default 0.5).
- Export: 1080x1920 MP4, `-c:v libx264 -crf 18 -preset medium -pix_fmt yuv420p -r 30 -c:a aac -b:a 160k -movflags +faststart`.
- Export filter graphs exactly as in the spec (Blur bars, Zoom & crop); the preview uses the same function at a smaller canvas.
- Never modify or overwrite the source file; never overwrite an existing output without confirmation.
- ffmpeg/ffprobe lookup order: next to the exe, then `PATH`.
- Official builds come from GitHub Actions native runners (windows, macos, ubuntu) plus local `scripts/build.ps1` / `scripts/build.sh`; unsigned in v1.
- Released ffmpeg is GPL: fine for personal use, flag before public distribution.
- Dev/test machines need `ffmpeg` and `ffprobe` on `PATH` and a libmpv dev library (see Task 8).

## Review Focus

Spec is silent on these; each has a test in the task named.

1. Video shorter than the chosen preset (e.g. 10s clip, Long) → window is the whole video, no panic (Task 2).
2. Sources that are not 16:9: portrait, narrower than 9:16, square, or a phone clip with rotation metadata → export is still 1080x1920 and zoom crop never exceeds the frame (Tasks 3, 4, 6).
3. Video with no audio track → export succeeds, no audio stream (Task 6).
4. Output path equals the source, or already exists, or user cancels mid-export → source untouched, no overwrite without confirmation, no partial file left behind (Tasks 6, 11).
5. Unreadable / non-video / zero-duration file → clear error, app stays on the empty state (Tasks 4, 9).

---

### Task 1: Project scaffold and shared types

**Files:**
- Create: `Cargo.toml`, `src/main.rs`, `src/model.rs`, `tests/common/mod.rs`
- Test: `src/model.rs` (inline `#[cfg(test)]`)

**Interfaces:**
- Produces (`model.rs`):
  - `pub enum Mode { BlurBars, ZoomCrop }`
  - `pub enum Preset { Short, Medium, Long }` with `pub fn seconds(self) -> f64` (15/30/60) and `pub const ALL: [Preset; 3]`
  - `pub struct Style { pub mode: Mode, pub blur: u32, pub zoom: f64, pub crop_x: f64 }` with `Default` (BlurBars, 22, 1.0, 0.5) and `pub fn clamped(self) -> Style` (blur 4..=40, zoom 1.0..=2.0, crop_x 0..=1)
- Produces (`tests/common/mod.rs`): `pub fn make_clip(dir: &Path, w: u32, h: u32, secs: u32, audio: bool) -> PathBuf` — runs `ffmpeg -f lavfi -i testsrc2=size={w}x{h}:rate=30:duration={secs}` (+ `-f lavfi -i sine=duration={secs}` when `audio`) into `dir/clip.mp4`; panics with "ffmpeg not found on PATH" if missing. Also `pub fn make_clip_rotated(dir, w, h, secs) -> PathBuf` that sets display-matrix rotation 90 in the container metadata.

- [ ] **Step 1: Write the failing tests** in `model.rs`: `preset_seconds` asserts 15.0/30.0/60.0; `style_default` asserts (BlurBars, 22, 1.0, 0.5); `style_clamped` asserts `Style{blur:100, zoom:3.0, crop_x:-1.0, ..}` → blur 40, zoom 2.0, crop_x 0.0 and blur 1 → 4.
- [ ] **Step 2:** `cargo new --name reelmaker .` equivalent (keep existing `mockup.html`, `docs/`, `.gitignore`); add dependencies listed in Tech Stack; `cargo test` → FAIL (types missing).
- [ ] **Step 3: Implement** `model.rs` per Interfaces; `main.rs` opens an empty `eframe` window titled "ReelMaker".
- [ ] **Step 4:** `cargo test` → PASS; `cargo run` shows an empty window.
- [ ] **Step 5: Commit** `feat: scaffold crate and shared model types`.

---

### Task 2: Reel window model

**Files:** Create `src/window.rs`; test inline.

**Interfaces:**
- Consumes: `Preset` (Task 1).
- Produces: `pub struct ReelWindow { pub start: f64, pub length: f64 }` with
  `pub fn new(duration: f64, preset: Preset) -> Self`,
  `pub fn set_preset(&mut self, duration: f64, preset: Preset)` (keeps `start`, then re-clamps),
  `pub fn drag_to(&mut self, duration: f64, start: f64)`,
  `pub fn end(&self) -> f64`.
  Invariants: `length = min(preset.seconds(), duration)`; `0 <= start <= duration - length`.

- [ ] **Step 1: Write failing tests:** `new_medium_on_long_video` (180s, Medium → start 0, length 30); **`short_video_uses_whole_video`** (10s, Long → length 10, start 0, `end()==10`); `drag_clamps_high` (180s, Medium, drag_to(500) → start 150); `drag_clamps_low` (drag_to(-5) → 0); `set_preset_reclamps` (start 170, Short→Long on 180s → length 60, start 120).
- [ ] **Step 2:** `cargo test window` → FAIL.
- [ ] **Step 3: Implement** to satisfy invariants.
- [ ] **Step 4:** `cargo test window` → PASS.
- [ ] **Step 5: Commit** `feat: reel window model`.

---

### Task 3: Filter graph builder (shared by preview and export)

**Files:** Create `src/filters.rs`; test inline.

**Interfaces:**
- Consumes: `Style`, `Mode`.
- Produces: `pub struct Canvas { pub w: u32, pub h: u32 }` with `pub const EXPORT: Canvas = Canvas{w:1080,h:1920}`;
  `pub fn reel_graph(style: &Style, canvas: Canvas, input: &str, output: &str) -> String` — `input`/`output` are bare labels (`"0:v"`/`"vout"` for ffmpeg, `"vid1"`/`"vo"` for mpv's `lavfi-complex`); result always has the form `[input]…[output]`.

- [ ] **Step 1: Write failing tests** (string assertions):
  - `blur_graph_matches_spec`: EXPORT canvas, blur 22 → contains `split[a][b]`, `scale=1080:1920:force_original_aspect_ratio=increase,crop=1080:1920,boxblur=22:5`, `scale=1080:-2`, `overlay=(W-w)/2:(H-h)/2`, starts with `[0:v]`, ends with `[vout]`.
  - `blur_scales_with_canvas`: Canvas 270x480, blur 22 → `boxblur=5` (radius × canvas.w/1080, rounded, min 1) and `scale=270:480:`.
  - `zoom_graph_matches_spec`: zoom 1.5, crop_x 0.25 → contains `ih*9/16/1.5`, `scale=1080:1920`, `0.25*iw`.
  - **`zoom_never_exceeds_frame`**: graph contains `min(iw` so a source narrower than 9:16 crops at full width (`h = w*16/9`).
  - `labels_for_mpv`: input `vid1`, output `vo` → starts `[vid1]`, ends `[vo]`.
- [ ] **Step 2:** FAIL.
- [ ] **Step 3: Implement** `reel_graph`. Zoom form: `crop=w='min(iw,ih*9/16/Z)':h='min(iw,ih*9/16/Z)*16/9':x='clip(CX*iw-w/2,0,iw-w)':y='(ih-h)/2',scale=W:H`. Quote expressions with single quotes so commas survive. Format floats without trailing noise (`{}`).
- [ ] **Step 4:** PASS. (Real ffmpeg acceptance of these strings is proven in Task 6.)
- [ ] **Step 5: Commit** `feat: reel filter graph builder`.

---

### Task 4: ffprobe wrapper

**Files:** Create `src/probe.rs`, `tests/fixtures/probe_landscape.json`, `tests/fixtures/probe_rotated.json`, `tests/fixtures/probe_noaudio.json`; tests inline + `tests/probe_it.rs`.

**Interfaces:**
- Produces: `pub struct MediaInfo { pub duration: f64, pub width: u32, pub height: u32, pub fps: f64, pub has_audio: bool }` (width/height are display dimensions, i.e. swapped when rotation is ±90/270);
  `pub enum ProbeError { Unreadable(String), NoVideo, BadDuration }`;
  `pub fn parse_probe(json: &str) -> Result<MediaInfo, ProbeError>`;
  `pub fn probe(ffprobe: &Path, file: &Path) -> Result<MediaInfo, ProbeError>` runs `ffprobe -v error -show_streams -show_format -of json <file>`.

- [ ] **Step 1: Write failing tests:** `parse_landscape` (1920x1080, 30000/1001 fps → width 1920, height 1080, fps≈29.97, has_audio true); **`parse_rotated_swaps_dims`** (stored 1920x1080 with display-matrix rotation -90 → 1080x1920); `parse_no_audio` (has_audio false); `parse_no_video_stream` → `NoVideo`; **`zero_duration_rejected`** → `BadDuration`; `garbage_json` → `Unreadable`. Integration: `probe` on a non-video file (a `.txt`) → `Err(Unreadable(_))`; on `make_clip(…,1280,720,5,true)` → duration ≈5.0 (±0.1).
- [ ] **Step 2:** FAIL.
- [ ] **Step 3: Implement.** Rotation from `side_data_list[].rotation` or `tags.rotate`; `fps` from `avg_frame_rate` fraction; duration from `format.duration`.
- [ ] **Step 4:** `cargo test probe` → PASS.
- [ ] **Step 5: Commit** `feat: ffprobe wrapper`.

---

### Task 5: Locate ffmpeg/ffprobe

**Files:** Create `src/tools.rs`; test inline using `tempfile`.

**Interfaces:**
- Produces: `pub struct Tools { pub ffmpeg: PathBuf, pub ffprobe: PathBuf }`;
  `pub struct ToolsError { pub searched: Vec<PathBuf> }` (Display lists every searched dir);
  `pub fn locate(exe_dir: &Path, path_var: &OsStr) -> Result<Tools, ToolsError>`; names `ffmpeg(.exe)` / `ffprobe(.exe)` per platform.

- [ ] **Step 1: Write failing tests:** `prefers_exe_dir` (fake files in both exe_dir and a PATH dir → exe_dir copy chosen); `falls_back_to_path`; `missing_lists_searched_dirs` (error `searched` contains exe_dir and each PATH entry); `needs_both` (only ffmpeg present → error).
- [ ] **Step 2:** FAIL. **Step 3: Implement** with `std::env::split_paths`. **Step 4:** PASS.
- [ ] **Step 5: Commit** `feat: locate ffmpeg tools`.

---

### Task 6: Exporter

**Files:** Create `src/export.rs`, `tests/export_it.rs`; unit tests inline.

**Interfaces:**
- Consumes: `Style`, `filters::{reel_graph, Canvas}`, `Tools`, `make_clip*` helpers.
- Produces:
  - `pub struct ExportJob { pub input: PathBuf, pub output: PathBuf, pub start: f64, pub length: f64, pub style: Style, pub has_audio: bool, pub overwrite: bool }`
  - `pub enum ExportError { SameAsSource, OutputDirMissing, Exists, Spawn(String) }`
  - `pub fn validate(job: &ExportJob) -> Result<(), ExportError>` (compares canonicalized paths; `Exists` when output exists and `!overwrite`)
  - `pub fn build_args(job: &ExportJob) -> Vec<OsString>` — `-ss start -t length -i input -filter_complex <reel_graph(EXPORT,"0:v","vout")> -map [vout] -map 0:a? <codec flags from Global Constraints> -progress pipe:1 -nostats` then `-y` or `-n`, then output
  - `pub fn parse_progress_line(line: &str) -> Option<f64>` — `out_time_us=N` → seconds; `N/A`/other keys → `None`
  - `pub enum ExportEvent { Progress(f64 /*0..=1*/), Done, Failed(String /*last 10 stderr lines*/), Cancelled }`
  - `pub struct ExportHandle { pub events: mpsc::Receiver<ExportEvent> }` with `pub fn cancel(&self)`
  - `pub fn start(tools: &Tools, job: ExportJob) -> Result<ExportHandle, ExportError>` (runs `validate` first; reader thread parses stdout progress, collects stderr; on cancel kills child and deletes the output file)

- [ ] **Step 1: Write failing unit tests:** `progress_parses_us` (`"out_time_us=2500000"` → 2.5); `progress_ignores_na`; `args_contain_codec_flags` (every flag from Global Constraints present, `-n` when `overwrite=false`, `-y` when true); `args_input_before_filter`; `validate_same_as_source`; `validate_exists_without_overwrite`.
- [ ] **Step 2: Write failing integration tests** (`tests/export_it.rs`, real ffmpeg, temp dirs): for each of `(1920,1080)`, `(1080,1920)`, `(720,720)`, `(400,1000)` (narrower than 9:16) × both `Mode`s, export a 3s window of a 4s clip → output probes (via `probe`) as **1080x1920**, duration ≈3s, h264. Also: **`no_audio_source_exports`** (`make_clip(..,false)` → success, `has_audio == false`); `audio_source_keeps_audio`; **`rotated_source_exports_vertical`** (`make_clip_rotated`); **`cancel_removes_partial`** (start export of a long clip, `cancel()`, wait for `Cancelled`, output file absent); `source_untouched` (input bytes/hash identical after export); `progress_reaches_one` (last `Progress` ≥0.99 before `Done`).
- [ ] **Step 3:** `cargo test export` → FAIL.
- [ ] **Step 4: Implement** per Interfaces. If a graph from Task 3 fails in ffmpeg, fix Task 3's `filters.rs` and its tests, not the integration test.
- [ ] **Step 5:** all export tests PASS.
- [ ] **Step 6: Commit** `feat: ffmpeg exporter with progress and cancel`.

---

### Task 7: Timeline thumbnails

**Files:** Create `src/thumbs.rs`, `tests/thumbs_it.rs`.

**Interfaces:**
- Produces: `pub fn extract(ffmpeg: &Path, input: &Path, duration: f64, count: usize, out_dir: &Path) -> Result<Vec<PathBuf>, String>` — one ffmpeg call: `-vf fps=count/duration,scale=-2:64 -frames:v count out_dir/thumb_%02d.jpg`; returns files in time order. Runs on a worker thread from the UI (Task 9), never the UI thread.

- [ ] **Step 1: Failing integration test:** 16s clip, `count=8` → 8 existing, non-empty `.jpg` files, sorted by name; and short clip (2s) with `count=24` → returns ≥1 file without error.
- [ ] **Step 2:** FAIL. **Step 3: Implement.** **Step 4:** PASS (if fps rounding yields count±1, pad by repeating the last frame path so the result length is exactly `count` when at least one frame exists).
- [ ] **Step 5: Commit** `feat: timeline thumbnails`.

---

### Task 8: libmpv player wrapper

**Files:** Create `src/player.rs`, `tests/player_it.rs`, `build.rs` (linker/rpath setup); note in `README.md` how to obtain the libmpv dev library per OS.

**Interfaces:**
- Consumes: `filters::reel_graph`, `Style`.
- Produces: `pub struct Player` with
  - `pub fn new_headless() -> Result<Player, String>` (vo=null, ao=null; for tests)
  - `pub fn new_gl(get_proc_address: &dyn Fn(&str) -> *const c_void, audio: bool) -> Result<Player, String>` (render-API context; `audio=false` for the 9:16 view's mpv so sound isn't doubled)
  - `pub fn load(&mut self, path: &Path) -> Result<(), String>`
  - `pub fn set_paused(&mut self, paused: bool)`, `pub fn seek(&mut self, secs: f64)`, `pub fn position(&self) -> f64`, `pub fn duration(&self) -> Option<f64>`
  - `pub fn set_loop(&mut self, window: Option<(f64, f64)>)` (`ab-loop-a/b`; `None` clears)
  - `pub fn set_reel_graph(&mut self, graph: Option<&str>) -> Result<(), String>` (sets `lavfi-complex`; `None` clears)
  - `pub fn render(&mut self, fbo: i32, w: u32, h: u32)`; `pub fn needs_redraw(&self) -> bool`

- [ ] **Step 1: Failing integration tests** (headless): `loads_and_reports_duration` (5s clip → ≈5.0); `seek_then_position` (seek 2.0 while paused → position within 0.2); **`accepts_reel_graph_both_modes`** (`reel_graph(Style{mode}, Canvas{270,480}, "vid1","vo")` set without error, then `load` + play 0.5s → position advances, for each `Mode`); `clearing_graph_ok`; `loop_window_wraps` (set_loop(1.0,2.0), play → position stays ≤2.1 after 3s wall time).
- [ ] **Step 2:** FAIL. **Step 3: Implement** with the `libmpv2` crate. Linking: libmpv is dynamically linked and found by the OS loader (next to the exe on Windows; `$ORIGIN` / `@executable_path` rpath set in `build.rs` on Linux/macOS). **Deviation from the spec's "dialog if libmpv missing": the OS loader fails before our code runs; the bundle must always ship libmpv (Task 12 verifies it).**
- [ ] **Step 4:** `cargo test player` → PASS. The GL render path is verified visually in Task 9, not here.
- [ ] **Step 5: Commit** `feat: libmpv player wrapper`.

---

### Task 9: App state and UI shell (stage + live 9:16 preview)

**Files:** Create `src/state.rs`, `src/app.rs`, `src/ui/mod.rs`, `src/ui/stage.rs`, `src/ui/sidebar.rs`; modify `src/main.rs`. State logic tested inline; UI verified manually.

**Interfaces:**
- Consumes: `MediaInfo`, `ReelWindow`, `Style`, `Preset`, `Tools`, `Player`, `thumbs::extract`, `probe::probe`, `filters`.
- Produces (`state.rs`, no egui or mpv imports):
  - `pub struct AppState { pub media: Option<Loaded>, pub style: Style, pub preset: Preset, pub window: Option<ReelWindow>, pub loop_window: bool, pub error: Option<String> }`; `pub struct Loaded { pub path: PathBuf, pub info: MediaInfo, pub thumbs: Vec<PathBuf> }`
  - `pub fn load_result(&mut self, path: PathBuf, r: Result<MediaInfo, ProbeError>)` (Ok → media + window from preset, clears error; Err → media stays/`None`, `error` set to a readable message)
  - `pub fn set_preset(&mut self, p: Preset)`, `pub fn set_mode(&mut self, m: Mode)`, `pub fn set_blur(&mut self, v: u32)`, `pub fn set_zoom(&mut self, v: f64)`, `pub fn set_crop_x(&mut self, v: f64)` (all clamp)
  - `pub fn preview_graph(&self) -> Option<String>` → `reel_graph(&style, Canvas{w:270,h:480}, "vid1", "vo")`
- UI: layout per `mockup.html` (header with filename / Import / Export; stage; transport; right column with Output style card, live 9:16 preview, export summary). Import uses `rfd`; probing and thumbnails run on a worker thread and report back over a channel. Two `Player`s: stage (audio on, plain) and 9:16 preview (audio off, `set_reel_graph(state.preview_graph())`), kept in sync (same `seek`/pause) each frame, graph re-applied only when `style` changes.

- [ ] **Step 1: Failing state tests:** `load_ok_sets_window` (180s → window 30s at 0); `load_err_sets_error_and_keeps_empty` (**unreadable / BadDuration → `media` None, `error` Some**); `preset_change_updates_window`; `short_video_whole_window_after_preset` (10s, Long → length 10); `setters_clamp`; `preview_graph_none_without_media`; `preview_graph_reflects_mode`.
- [ ] **Step 2:** FAIL. **Step 3: Implement** `state.rs`, then the UI shell.
- [ ] **Step 4:** `cargo test state` → PASS.
- [ ] **Step 5: Manual verification:** `cargo run`, import a landscape clip and a non-video file. Expected: stage plays with sound; 9:16 preview plays in sync and switching Blur/Zoom changes it live; non-video file shows an error and the empty state remains; layout resembles `mockup.html`.
- [ ] **Step 6: Commit** `feat: app state, stage and live preview`.

---

### Task 10: Timeline, window drag, crop box

**Files:** Create `src/ui/geometry.rs`, `src/ui/timeline.rs`; extend `src/ui/stage.rs`, `src/ui/sidebar.rs`.

**Interfaces:**
- Produces (`geometry.rs`, pure):
  - `pub fn time_to_x(t: f64, duration: f64, width: f32) -> f32`, `pub fn x_to_time(x: f32, duration: f64, width: f32) -> f64` (clamped)
  - `pub fn window_start_from_pointer(pointer_x: f32, grab_offset_secs: f64, duration: f64, width: f32) -> f64` (unclamped; `ReelWindow::drag_to` clamps)
  - `pub fn crop_x_from_pointer(pointer_x: f32, stage_left: f32, stage_w: f32) -> f64` (0..=1)
  - `pub fn crop_width_fraction(source_aspect: f64, zoom: f64) -> f64` = `min(1, (9/16) / (zoom * source_aspect))`

- [ ] **Step 1: Failing tests:** `time_x_roundtrip`; `x_to_time_clamps`; `window_drag_keeps_grab_offset`; `crop_x_clamps` (left of stage → 0, right → 1); `crop_width_16_9_zoom1` ≈ 0.3164; `crop_width_zoom2_halves`; `crop_width_narrow_source_caps_at_1` (aspect 0.4 → 1.0).
- [ ] **Step 2:** FAIL. **Step 3: Implement** geometry, then widgets: thumbnails row behind the track, draggable window with `HH:MM – HH:MM` tag, playhead, click-to-scrub, preset chips, Play and Loop-window buttons (loop uses `Player::set_loop`), crop box over the stage in Zoom mode with drag-to-reframe and the zoom slider (100–200%) / blur slider (4–40).
- [ ] **Step 4:** `cargo test geometry` → PASS.
- [ ] **Step 5: Manual verification:** on a 3-minute clip: dragging the window moves it along the track without moving the playhead; Loop window plays only the selection; clicking the track scrubs; chips resize the window; a 10s clip with Long shows a whole-video window; dragging the crop box changes the 9:16 preview framing; a short test export matches the preview framing.
- [ ] **Step 6: Commit** `feat: timeline, window drag and crop box`.

---

### Task 11: Export wiring (dialogs, progress, cancel, errors)

**Files:** Create `src/ui/export_panel.rs`; extend `src/state.rs`, `src/app.rs`.

**Interfaces:**
- Consumes: `export::{start, validate, ExportJob, ExportEvent, ExportError}`, `Tools`.
- Produces: `AppState::export_job(&self, output: PathBuf, overwrite: bool) -> Option<ExportJob>` (None without media; `start/length` from window, `has_audio` from probe, style from state); `pub fn default_output_name(input: &Path) -> String` → `"<stem>_reel.mp4"`.

- [ ] **Step 1: Failing tests:** `export_job_uses_window_and_style`; `export_job_none_without_media`; `default_output_name_strips_ext` (`C:\v\trip.mkv` → `trip_reel.mp4`); **`default_name_handles_unicode_and_spaces`** (`"my vidéo.mov"` → `my vidéo_reel.mp4`).
- [ ] **Step 2:** FAIL. **Step 3: Implement.** Export button → `rfd` save dialog (default dir = source dir) → if `validate` returns `Exists`, show a confirm dialog and only then retry with `overwrite: true`; `SameAsSource` shows an error and does not start; progress bar modal with Cancel; on `Failed(msg)` show the stderr tail; on `Done` show path and "Open folder". Missing ffmpeg at startup → blocking dialog listing `ToolsError.searched`.
- [ ] **Step 4:** `cargo test` (whole suite) → PASS.
- [ ] **Step 5: Manual verification:** export a 15s Short reel in each mode; play result in an external player (1080x1920, audio present); choose the source file as output → refused; choose an existing file → confirm prompt, Cancel leaves it unchanged; cancel mid-export leaves no partial file.
- [ ] **Step 6: Commit** `feat: export UI, progress, cancel, error handling`.

---

### Task 12: Build scripts and CI for Windows, macOS, Linux

**Files:** Create `scripts/build.ps1`, `scripts/build.sh`, `.github/workflows/release.yml`; modify `README.md`, `src/main.rs` (add `--version` flag that prints the version and exits after the libmpv-linked binary starts).

**Interfaces:**
- Produces: `dist/reelmaker-<os>-<arch>.zip|.tar.gz` containing the app binary, `ffmpeg`, `ffprobe`, and libmpv (`mpv-2.dll`/`libmpv.2.dylib`/`libmpv.so.2`) laid out so the lookup in Tasks 5 and 8 finds them next to the exe. Intended sources: BtbN GPL static ffmpeg builds (Windows/Linux), Homebrew `ffmpeg`+`mpv` on macOS runners (dylibs bundled with `dylibbundler`), shinchiro libmpv for Windows, distro `libmpv` on Linux; exact URLs and bundling method are the implementer's call. Binary lookup stays: next to exe → `PATH`.

- [ ] **Step 1: Write `scripts/build.ps1`** (Windows host): download/extract ffmpeg + libmpv into `vendor/` (git-ignored), `cargo build --release`, assemble `dist/`, zip. **Step 2: `scripts/build.sh`** same for macOS/Linux (switch on `uname`).
- [ ] **Step 3: Verification (Windows, local):** run `scripts/build.ps1`; expect `dist/reelmaker-windows-x86_64.zip`. Extract to a fresh temp dir, run `.\reelmaker.exe --version` from there with `PATH` stripped of any ffmpeg → expect the version string (proves `mpv-2.dll` is found), and launch the GUI to confirm Import and Export work using the bundled ffmpeg.
- [ ] **Step 4: Write `release.yml`:** matrix `windows-latest`, `macos-latest`, `ubuntu-latest`; steps call the matching script, run `cargo test` (installing ffmpeg for the integration tests first), upload artifacts; on tag `v*` attach them to a GitHub Release.
- [ ] **Step 5: Verification (CI):** push the branch, `gh run watch`; expect all three jobs green and three artifacts. Smoke-test each artifact's `--version` inside the job after unpacking. macOS and Linux packaging are the likeliest to need iteration; fixes belong in the scripts, not in app code.
- [ ] **Step 6:** update `README.md` (what it is, dev prerequisites, how to build, GPL note, unsigned-build warning).
- [ ] **Step 7: Commit** `build: cross-platform build scripts and release workflow`.

---

## Self-review notes

- **Spec coverage:** probe (T4), player (T8), thumbnails (T7), preview renderer (T9), window model (T2), exporter (T6), build/packaging (T12), filter graphs (T3), UI/mockup layout (T9–T11), errors (T4, T5, T9, T11). No gaps found.
- **Known deviation:** libmpv is dynamically linked and resolved by the OS loader, so "missing libmpv → dialog" from the spec cannot be done in-app; the packaging step guarantees the library ships. Flag in handoff.
- **Type consistency:** `reel_graph(&Style, Canvas, &str, &str)` is used identically in Tasks 6, 8, 9; `ExportJob.overwrite` set only in Task 11; `Tools` produced in Task 5, consumed in Tasks 6, 7, 11.
