//! eframe application: owns the state, the two mpv players, and the worker-thread plumbing.

use crate::export::{self, ExportEvent, ExportHandle};
use crate::model::Mode;
use crate::player::Player;
use crate::probe::{self, MediaInfo, ProbeError};
use crate::state::{default_output_name, play_restart, AppState};
use crate::thumbs;
use crate::tools::{self, Tools, ToolsError};
use crate::ui::video::{fit_rect, paint_video, SharedPlayer, SharedSurface};
use eframe::egui::{self, Color32, RichText};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

const THUMB_COUNT: usize = 24;

enum Msg {
    Probed(PathBuf, Result<MediaInfo, ProbeError>),
    Thumbs(PathBuf, Vec<PathBuf>),
}

pub struct App {
    pub state: AppState,
    tools: Result<Tools, ToolsError>,
    players: Result<Players, String>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    thumb_dir: PathBuf,
    pending_import: Option<PathBuf>,
    export: Option<RunningExport>,
    export_outcome: Option<Outcome>,
}

struct RunningExport {
    handle: ExportHandle,
    progress: f64,
    output: PathBuf,
}

enum Outcome {
    Done(PathBuf),
    Failed(String),
}

struct Players {
    stage: SharedPlayer,
    stage_surface: SharedSurface,
    preview: SharedPlayer,
    preview_surface: SharedSurface,
    applied_graph: Option<String>,
    applied_loop: Option<(f64, f64)>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> App {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .unwrap_or_default();
        let tools = tools::locate(&exe_dir, &std::env::var_os("PATH").unwrap_or_default());

        let players = match cc.get_proc_address {
            None => Err("This build has no OpenGL context for video playback.".to_string()),
            Some(gpa) => {
                let load = |name: &str| match std::ffi::CString::new(name) {
                    Ok(c) => gpa(&c),
                    Err(_) => std::ptr::null(),
                };
                (|| {
                    let stage = Player::new_gl(&load, true)?;
                    let preview = Player::new_gl(&load, false)?;
                    Ok(Players {
                        stage: Arc::new(Mutex::new(stage)),
                        stage_surface: Default::default(),
                        preview: Arc::new(Mutex::new(preview)),
                        preview_surface: Default::default(),
                        applied_graph: None,
                        applied_loop: None,
                    })
                })()
            }
        };

        let (tx, rx) = channel();
        let thumb_dir = std::env::temp_dir().join(format!("reelmaker-thumbs-{}", std::process::id()));
        let pending_import = std::env::args_os().nth(1).map(PathBuf::from);
        App { state: AppState::default(), tools, players, tx, rx, thumb_dir, pending_import, export: None, export_outcome: None }
    }

    fn import(&mut self, ctx: &egui::Context, path: PathBuf) {
        let Ok(tools) = &self.tools else { return };
        let (ffprobe, tx, ctx) = (tools.ffprobe.clone(), self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let r = probe::probe(&ffprobe, &path);
            let _ = tx.send(Msg::Probed(path, r));
            ctx.request_repaint();
        });
    }

    fn pump_messages(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Probed(path, r) => {
                    let ok = r.is_ok();
                    self.state.load_result(path.clone(), r);
                    if ok {
                        self.start_playback_of(&path);
                        self.spawn_thumbs(ctx, path);
                    }
                }
                Msg::Thumbs(path, files) => {
                    if self.state.media.as_ref().is_some_and(|m| m.path == path) {
                        ctx.forget_all_images();
                        self.state.set_thumbs(files);
                    }
                }
            }
        }
    }

    fn start_playback_of(&mut self, path: &Path) {
        if let Ok(p) = &mut self.players {
            for pl in [&p.stage, &p.preview] {
                let mut pl = pl.lock().unwrap();
                pl.set_paused(true);
                if let Err(e) = pl.load(path) {
                    self.state.error = Some(e);
                }
            }
            p.applied_graph = None;
            p.applied_loop = None;
        }
    }

    fn spawn_thumbs(&self, ctx: &egui::Context, path: PathBuf) {
        let (Ok(tools), Some(m)) = (&self.tools, &self.state.media) else { return };
        let (ffmpeg, dur, dir) = (tools.ffmpeg.clone(), m.info.duration, self.thumb_dir.clone());
        let (tx, ctx) = (self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            if let Ok(files) = thumbs::extract(&ffmpeg, &path, dur, THUMB_COUNT, &dir) {
                let _ = tx.send(Msg::Thumbs(path, files));
                ctx.request_repaint();
            }
        });
    }

    /// Keeps both players aligned and applies the current preview graph / loop window.
    fn sync_players(&mut self) {
        let Ok(p) = &mut self.players else { return };
        let graph = self.state.preview_graph();
        let loop_win = match (self.state.loop_window, self.state.window) {
            (true, Some(w)) => Some((w.start, w.end())),
            _ => None,
        };
        let (mut stage, mut preview) = (p.stage.lock().unwrap(), p.preview.lock().unwrap());
        if graph != p.applied_graph {
            if let Err(e) = preview.set_reel_graph(graph.as_deref()) {
                self.state.error = Some(e);
            }
            p.applied_graph = graph;
        }
        if loop_win != p.applied_loop {
            stage.set_loop(loop_win);
            preview.set_loop(loop_win);
            p.applied_loop = loop_win;
        }
        let (t, pt) = (stage.position(), preview.position());
        if stage.paused() != preview.paused() {
            let paused = stage.paused();
            preview.set_paused(paused);
        }
        if (t - pt).abs() > 0.25 {
            preview.seek(t);
        }
    }

    fn seek(&mut self, t: f64) {
        if let Ok(p) = &self.players {
            for pl in [&p.stage, &p.preview] {
                pl.lock().unwrap().seek(t);
            }
        }
    }

    fn toggle_play(&mut self) {
        let Ok(p) = &self.players else { return };
        let (paused, pos, dur) = {
            let s = p.stage.lock().unwrap();
            (s.paused(), s.position(), s.duration().unwrap_or(0.0))
        };
        if paused {
            // After playing to the end, Play restarts from the reel window instead of doing nothing.
            if let Some(t) = play_restart(pos, dur, self.state.window) {
                self.seek(t);
            }
        }
        let Ok(p) = &self.players else { return };
        for pl in [&p.stage, &p.preview] {
            pl.lock().unwrap().set_paused(!paused);
        }
    }

    fn begin_export(&mut self) {
        let (Ok(tools), Some(m)) = (&self.tools, &self.state.media) else { return };
        let mut dialog = rfd::FileDialog::new()
            .add_filter("MP4 video", &["mp4"])
            .set_file_name(default_output_name(&m.path));
        if let Some(dir) = m.path.parent() {
            dialog = dialog.set_directory(dir);
        }
        let Some(mut out) = dialog.save_file() else { return };
        if out.extension().is_none() {
            out.set_extension("mp4");
        }
        let Some(mut job) = self.state.export_job(out.clone(), false) else { return };
        if let Err(export::ExportError::Exists) = export::validate(&job) {
            let replace = rfd::MessageDialog::new()
                .set_title("Replace existing file?")
                .set_description(format!("{} already exists. Replace it?", out.display()))
                .set_buttons(rfd::MessageButtons::YesNo)
                .show();
            if replace != rfd::MessageDialogResult::Yes {
                return;
            }
            job.overwrite = true;
        }
        match export::start(tools, job) {
            Ok(handle) => self.export = Some(RunningExport { handle, progress: 0.0, output: out }),
            Err(e) => self.export_outcome = Some(Outcome::Failed(e.to_string())),
        }
    }

    fn poll_export(&mut self) {
        let Some(run) = &mut self.export else { return };
        let mut finished = None;
        for ev in run.handle.events.try_iter() {
            match ev {
                ExportEvent::Progress(p) => run.progress = p,
                ExportEvent::Done => finished = Some(Some(Outcome::Done(run.output.clone()))),
                ExportEvent::Failed(m) => finished = Some(Some(Outcome::Failed(m))),
                ExportEvent::Cancelled => finished = Some(None),
            }
        }
        if let Some(outcome) = finished {
            self.export = None;
            if outcome.is_some() {
                self.export_outcome = outcome;
            }
        }
    }

    fn export_windows(&mut self, ctx: &egui::Context) {
        if let Some(run) = &self.export {
            let mut cancel = false;
            egui::Window::new("Exporting reel…")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.add(egui::ProgressBar::new(run.progress as f32).show_percentage().desired_width(320.0));
                    ui.add_space(6.0);
                    cancel = ui.button("Cancel").clicked();
                });
            if cancel {
                run.handle.cancel();
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
        let mut close = false;
        match &self.export_outcome {
            Some(Outcome::Done(path)) => {
                egui::Window::new("Export finished")
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        ui.label(format!("Saved to {}", path.display()));
                        ui.horizontal(|ui| {
                            if ui.button("Open folder").clicked() {
                                open_folder(path);
                            }
                            close = ui.button("Close").clicked();
                        });
                    });
            }
            Some(Outcome::Failed(msg)) => {
                egui::Window::new("Export failed")
                    .collapsible(false)
                    .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                    .show(ctx, |ui| {
                        egui::ScrollArea::vertical().max_height(220.0).show(ui, |ui| {
                            ui.monospace(msg);
                        });
                        close = ui.button("Close").clicked();
                    });
            }
            None => {}
        }
        if close {
            self.export_outcome = None;
        }
    }

    fn header(&mut self, ctx: &egui::Context) {
        let mut start_export = false;
        egui::TopBottomPanel::top("header").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading(RichText::new("ReelMaker").strong());
                let name = self
                    .state
                    .media
                    .as_ref()
                    .and_then(|m| m.path.file_name())
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "No video loaded".into());
                ui.weak(name);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let can_export = self.state.media.is_some() && self.export.is_none();
                    if ui.add_enabled(can_export, egui::Button::new("Export reel")).clicked() {
                        start_export = true;
                    }
                    if ui.button("Import video").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("Video", &["mp4", "mkv", "mov", "avi", "webm", "m4v", "wmv", "flv", "ts", "mpg", "mpeg"])
                            .add_filter("All files", &["*"])
                            .pick_file()
                        {
                            self.import(ctx, path);
                        }
                    }
                });
            });
        });
        if start_export {
            self.begin_export();
        }
    }

    fn sidebar(&mut self, ctx: &egui::Context) {
        egui::SidePanel::right("sidebar").exact_width(320.0).show(ctx, |ui| {
            ui.add_space(8.0);
            ui.group(|ui| {
                ui.label(RichText::new("OUTPUT STYLE").small().weak());
                ui.horizontal(|ui| {
                    let m = self.state.style.mode;
                    if ui.selectable_label(m == Mode::BlurBars, "Blur bars").clicked() {
                        self.state.set_mode(Mode::BlurBars);
                    }
                    if ui.selectable_label(m == Mode::ZoomCrop, "Zoom & crop").clicked() {
                        self.state.set_mode(Mode::ZoomCrop);
                    }
                });
                match self.state.style.mode {
                    Mode::BlurBars => {
                        let mut b = self.state.style.blur;
                        if ui.add(egui::Slider::new(&mut b, 4..=40).text("Blur")).changed() {
                            self.state.set_blur(b);
                        }
                    }
                    Mode::ZoomCrop => {
                        let mut z = (self.state.style.zoom * 100.0).round();
                        if ui.add(egui::Slider::new(&mut z, 100.0..=200.0).suffix("%").text("Zoom")).changed() {
                            self.state.set_zoom(z / 100.0);
                        }
                    }
                }
            });
            ui.add_space(8.0);
            ui.group(|ui| {
                ui.label(RichText::new("LIVE PREVIEW · 9:16").small().weak());
                let avail = egui::Rect::from_min_size(ui.cursor().min, egui::vec2(ui.available_width(), 380.0));
                let rect = fit_rect(avail, 9.0 / 16.0);
                ui.allocate_rect(avail, egui::Sense::hover());
                if let (Ok(p), Some(_)) = (&self.players, &self.state.media) {
                    paint_video(ui, rect, &p.preview, &p.preview_surface);
                } else {
                    ui.painter().rect_filled(rect, 12.0, Color32::BLACK);
                }
            });
            ui.add_space(8.0);
            ui.group(|ui| {
                ui.label(RichText::new("EXPORT").small().weak());
                let len = self.state.window.map(|w| w.length).unwrap_or(0.0);
                ui.label("Resolution: 1080 × 1920");
                ui.label("Format: MP4 · 30fps");
                ui.label(format!("Length: {len:.0}s"));
            });
        });
    }

    fn central(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(err) = self.state.error.clone() {
                ui.colored_label(Color32::LIGHT_RED, err);
            }
            let aspect = self
                .state
                .media
                .as_ref()
                .map(|m| m.info.width as f32 / m.info.height as f32)
                .unwrap_or(16.0 / 9.0);
            let avail = egui::Rect::from_min_size(
                ui.cursor().min,
                egui::vec2(ui.available_width(), (ui.available_height() - 230.0).max(100.0)),
            );
            let rect = fit_rect(avail, aspect);
            ui.allocate_rect(avail, egui::Sense::hover());
            ui.painter().rect_filled(avail, 8.0, Color32::BLACK);
            match (&self.players, &self.state.media) {
                (Ok(p), Some(_)) => {
                    paint_video(ui, rect, &p.stage, &p.stage_surface);
                    if self.state.style.mode == Mode::ZoomCrop {
                        crate::ui::cropbox::show(ui, rect, &mut self.state);
                    }
                }
                _ => {
                    ui.painter().text(
                        avail.center(),
                        egui::Align2::CENTER_CENTER,
                        "Import or drop a video to start",
                        egui::FontId::proportional(18.0),
                        Color32::GRAY,
                    );
                }
            }
            let mut playhead = 0.0;
            ui.horizontal(|ui| {
                let (paused, pos, dur) = match &self.players {
                    Ok(p) if self.state.media.is_some() => {
                        let s = p.stage.lock().unwrap();
                        (s.paused(), s.position(), s.duration().unwrap_or(0.0))
                    }
                    _ => (true, 0.0, 0.0),
                };
                let label = if paused { "▶ Play" } else { "⏸ Pause" };
                if ui.add_enabled(self.state.media.is_some(), egui::Button::new(label)).clicked() {
                    self.toggle_play();
                }
                let has = self.state.window.is_some();
                if ui.add_enabled(has, egui::SelectableLabel::new(self.state.loop_window, "⟲ Loop window")).clicked() {
                    self.state.loop_window = !self.state.loop_window;
                    if let (true, Some(w)) = (self.state.loop_window, self.state.window) {
                        self.seek(w.start);
                    }
                }
                ui.monospace(format!("{} / {}", fmt_time(pos), fmt_time(dur)));
                playhead = pos;
            });
            if self.state.media.is_some() {
                if let Some(t) = crate::ui::timeline::show(ui, &mut self.state, playhead) {
                    self.seek(t);
                }
            }
        });
    }
}

fn open_folder(path: &Path) {
    let dir = path.parent().unwrap_or(path);
    #[cfg(windows)]
    let cmd = "explorer";
    #[cfg(target_os = "macos")]
    let cmd = "open";
    #[cfg(all(unix, not(target_os = "macos")))]
    let cmd = "xdg-open";
    let _ = std::process::Command::new(cmd).arg(dir).spawn();
}

pub fn fmt_time(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

impl eframe::App for App {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // Closing the window mid-export stops ffmpeg and removes the partial file (ExportHandle::drop).
        self.export = None;
    }

    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Err(e) = &self.tools {
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading("ffmpeg not found");
                ui.label(e.to_string());
            });
            return;
        }
        if let Err(e) = &self.players {
            let e = e.clone();
            egui::CentralPanel::default().show(ctx, |ui| {
                ui.heading("Video playback unavailable");
                ui.label(e);
            });
            return;
        }

        if let Some(path) = self.pending_import.take() {
            self.import(ctx, path);
        }
        let dropped = ctx.input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone()));
        if let Some(path) = dropped {
            self.import(ctx, path);
        }
        self.pump_messages(ctx);
        self.poll_export();
        self.sync_players();
        self.header(ctx);
        self.sidebar(ctx);
        self.central(ctx);
        self.export_windows(ctx);
        if self.state.media.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(33));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::fmt_time;

    #[test]
    fn formats_minutes_and_seconds() {
        assert_eq!(fmt_time(0.0), "0:00");
        assert_eq!(fmt_time(65.9), "1:05");
        assert_eq!(fmt_time(-3.0), "0:00");
    }
}
