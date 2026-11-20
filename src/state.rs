//! UI-independent application state. No egui or mpv in here, so it is plain-unit-testable.

use crate::export::ExportJob;
use crate::filters::{reel_graph, Canvas};
use crate::model::{Mode, Preset, Style};
use crate::probe::{MediaInfo, ProbeError};
use crate::window::ReelWindow;
use std::path::{Path, PathBuf};

/// Size of the live 9:16 preview canvas (the export uses `Canvas::EXPORT`).
pub const PREVIEW_CANVAS: Canvas = Canvas { w: 270, h: 480 };

#[derive(Debug, Clone)]
pub struct Loaded {
    pub path: PathBuf,
    pub info: MediaInfo,
    pub thumbs: Vec<PathBuf>,
}

#[derive(Debug)]
pub struct AppState {
    pub media: Option<Loaded>,
    pub style: Style,
    pub preset: Preset,
    pub window: Option<ReelWindow>,
    pub loop_window: bool,
    pub error: Option<String>,
}

impl Default for AppState {
    fn default() -> Self {
        AppState {
            media: None,
            style: Style::default(),
            preset: Preset::Medium,
            window: None,
            loop_window: false,
            error: None,
        }
    }
}

impl AppState {
    /// Result of probing a freshly imported file. Errors leave the previous video (if any) loaded.
    pub fn load_result(&mut self, path: PathBuf, r: Result<MediaInfo, ProbeError>) {
        match r {
            Ok(info) => {
                self.window = Some(ReelWindow::new(info.duration, self.preset));
                self.media = Some(Loaded { path, info, thumbs: Vec::new() });
                self.error = None;
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    pub fn set_thumbs(&mut self, thumbs: Vec<PathBuf>) {
        if let Some(m) = &mut self.media {
            m.thumbs = thumbs;
        }
    }

    pub fn set_preset(&mut self, p: Preset) {
        self.preset = p;
        if let (Some(m), Some(w)) = (&self.media, &mut self.window) {
            w.set_preset(m.info.duration, p);
        }
    }

    pub fn set_mode(&mut self, m: Mode) {
        self.style.mode = m;
    }

    pub fn set_blur(&mut self, v: u32) {
        self.style = Style { blur: v, ..self.style }.clamped();
    }

    pub fn set_zoom(&mut self, v: f64) {
        self.style = Style { zoom: v, ..self.style }.clamped();
    }

    pub fn set_crop_x(&mut self, v: f64) {
        self.style = Style { crop_x: v, ..self.style }.clamped();
    }

    /// The export job for the current media, window and style. `None` without media.
    pub fn export_job(&self, output: PathBuf, overwrite: bool) -> Option<ExportJob> {
        let (m, w) = (self.media.as_ref()?, self.window?);
        Some(ExportJob {
            input: m.path.clone(),
            output,
            start: w.start,
            length: w.length,
            style: self.style,
            has_audio: m.info.has_audio,
            overwrite,
        })
    }

    /// mpv `lavfi-complex` graph for the 9:16 preview; `None` with no media loaded.
    pub fn preview_graph(&self) -> Option<String> {
        self.media.as_ref()?;
        Some(reel_graph(&self.style, PREVIEW_CANVAS, "vid1", "vo"))
    }
}

/// `<stem>_reel.mp4`, for the save dialog's default file name.
pub fn default_output_name(input: &Path) -> String {
    match input.file_stem().map(|s| s.to_string_lossy()) {
        Some(stem) if !stem.is_empty() => format!("{stem}_reel.mp4"),
        _ => "reel.mp4".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(duration: f64) -> MediaInfo {
        MediaInfo { duration, width: 1920, height: 1080, fps: 30.0, has_audio: true }
    }

    fn loaded(duration: f64) -> AppState {
        let mut s = AppState::default();
        s.load_result("a.mp4".into(), Ok(info(duration)));
        s
    }

    #[test]
    fn load_ok_sets_window() {
        let s = loaded(180.0);
        assert_eq!(s.window, Some(ReelWindow { start: 0.0, length: 30.0 }));
        assert!(s.error.is_none() && s.media.is_some());
    }

    #[test]
    fn load_err_sets_error_and_keeps_empty() {
        for e in [ProbeError::NoVideo, ProbeError::BadDuration, ProbeError::Unreadable("x".into())] {
            let mut s = AppState::default();
            s.load_result("bad.txt".into(), Err(e));
            assert!(s.media.is_none() && s.window.is_none());
            assert!(s.error.as_deref().is_some_and(|m| !m.is_empty()));
        }
    }

    #[test]
    fn load_err_keeps_previous_video() {
        let mut s = loaded(60.0);
        s.load_result("bad.txt".into(), Err(ProbeError::NoVideo));
        assert!(s.media.is_some() && s.error.is_some());
    }

    #[test]
    fn successful_load_clears_error() {
        let mut s = AppState::default();
        s.load_result("bad".into(), Err(ProbeError::NoVideo));
        s.load_result("ok.mp4".into(), Ok(info(60.0)));
        assert!(s.error.is_none());
    }

    #[test]
    fn preset_change_updates_window() {
        let mut s = loaded(180.0);
        s.set_preset(Preset::Short);
        assert_eq!(s.window.unwrap().length, 15.0);
        assert_eq!(s.preset, Preset::Short);
    }

    #[test]
    fn short_video_whole_window_after_preset() {
        let mut s = loaded(10.0);
        s.set_preset(Preset::Long);
        assert_eq!(s.window, Some(ReelWindow { start: 0.0, length: 10.0 }));
    }

    #[test]
    fn new_video_uses_current_preset() {
        let mut s = AppState::default();
        s.set_preset(Preset::Long);
        s.load_result("a.mp4".into(), Ok(info(180.0)));
        assert_eq!(s.window.unwrap().length, 60.0);
    }

    #[test]
    fn setters_clamp() {
        let mut s = AppState::default();
        s.set_blur(999);
        s.set_zoom(0.1);
        s.set_crop_x(7.0);
        assert_eq!((s.style.blur, s.style.zoom, s.style.crop_x), (40, 1.0, 1.0));
    }

    #[test]
    fn preview_graph_none_without_media() {
        assert_eq!(AppState::default().preview_graph(), None);
    }

    #[test]
    fn preview_graph_reflects_mode() {
        let mut s = loaded(60.0);
        assert!(s.preview_graph().unwrap().contains("boxblur"));
        s.set_mode(Mode::ZoomCrop);
        let g = s.preview_graph().unwrap();
        assert!(g.contains("crop=") && !g.contains("boxblur"));
        assert!(g.starts_with("[vid1]") && g.ends_with("[vo]"));
    }

    #[test]
    fn export_job_uses_window_and_style() {
        let mut s = loaded(180.0);
        s.set_preset(Preset::Short);
        if let Some(w) = &mut s.window {
            w.drag_to(180.0, 42.0);
        }
        s.set_mode(Mode::ZoomCrop);
        s.set_zoom(1.5);
        let j = s.export_job("out.mp4".into(), true).unwrap();
        assert_eq!((j.start, j.length), (42.0, 15.0));
        assert_eq!(j.style.mode, Mode::ZoomCrop);
        assert_eq!(j.style.zoom, 1.5);
        assert!(j.has_audio && j.overwrite);
        assert_eq!(j.input, PathBuf::from("a.mp4"));
        assert_eq!(j.output, PathBuf::from("out.mp4"));
    }

    #[test]
    fn export_job_none_without_media() {
        assert!(AppState::default().export_job("o.mp4".into(), false).is_none());
    }

    #[test]
    fn default_output_name_strips_ext() {
        assert_eq!(default_output_name(&PathBuf::from("v").join("trip.mkv")), "trip_reel.mp4");
    }

    #[test]
    fn default_name_handles_unicode_and_spaces() {
        assert_eq!(default_output_name(Path::new("my vidéo.mov")), "my vidéo_reel.mp4");
    }

    #[test]
    fn default_name_without_extension_or_stem() {
        assert_eq!(default_output_name(Path::new("clip")), "clip_reel.mp4");
        assert_eq!(default_output_name(Path::new("")), "reel.mp4");
    }
}
