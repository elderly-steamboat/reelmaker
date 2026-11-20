//! The sliding reel window: a fixed-length clip selection along the timeline.

use crate::model::Preset;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReelWindow {
    pub start: f64,
    pub length: f64,
}

impl ReelWindow {
    pub fn new(duration: f64, preset: Preset) -> Self {
        ReelWindow { start: 0.0, length: preset.seconds().min(duration) }
    }

    /// Switch preset, keeping `start` where possible.
    pub fn set_preset(&mut self, duration: f64, preset: Preset) {
        self.length = preset.seconds().min(duration);
        self.drag_to(duration, self.start);
    }

    pub fn drag_to(&mut self, duration: f64, start: f64) {
        let max = (duration - self.length).max(0.0);
        self.start = start.clamp(0.0, max);
    }

    pub fn end(&self) -> f64 {
        self.start + self.length
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_medium_on_long_video() {
        let w = ReelWindow::new(180.0, Preset::Medium);
        assert_eq!((w.start, w.length), (0.0, 30.0));
    }

    #[test]
    fn short_video_uses_whole_video() {
        let w = ReelWindow::new(10.0, Preset::Long);
        assert_eq!((w.start, w.length, w.end()), (0.0, 10.0, 10.0));
    }

    #[test]
    fn drag_clamps_high() {
        let mut w = ReelWindow::new(180.0, Preset::Medium);
        w.drag_to(180.0, 500.0);
        assert_eq!(w.start, 150.0);
    }

    #[test]
    fn drag_clamps_low() {
        let mut w = ReelWindow::new(180.0, Preset::Medium);
        w.drag_to(180.0, -5.0);
        assert_eq!(w.start, 0.0);
    }

    #[test]
    fn set_preset_reclamps() {
        let mut w = ReelWindow::new(180.0, Preset::Short);
        w.drag_to(180.0, 170.0);
        assert_eq!(w.start, 165.0);
        w.set_preset(180.0, Preset::Long);
        assert_eq!((w.start, w.length), (120.0, 60.0));
    }
}
