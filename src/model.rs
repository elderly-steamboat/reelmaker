//! Shared value types: output mode, window-length presets, and output style.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    BlurBars,
    ZoomCrop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Short,
    Medium,
    Long,
}

impl Preset {
    pub const ALL: [Preset; 3] = [Preset::Short, Preset::Medium, Preset::Long];

    pub fn seconds(self) -> f64 {
        match self {
            Preset::Short => 15.0,
            Preset::Medium => 30.0,
            Preset::Long => 60.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    pub mode: Mode,
    /// Blur strength, 4..=40.
    pub blur: u32,
    /// Zoom factor, 1.0..=2.0.
    pub zoom: f64,
    /// Centre of the crop box over the source width, 0..=1.
    pub crop_x: f64,
}

impl Default for Style {
    fn default() -> Self {
        Style { mode: Mode::BlurBars, blur: 22, zoom: 1.0, crop_x: 0.5 }
    }
}

impl Style {
    pub fn clamped(self) -> Style {
        Style {
            mode: self.mode,
            blur: self.blur.clamp(4, 40),
            zoom: self.zoom.clamp(1.0, 2.0),
            crop_x: self.crop_x.clamp(0.0, 1.0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preset_seconds() {
        assert_eq!(Preset::Short.seconds(), 15.0);
        assert_eq!(Preset::Medium.seconds(), 30.0);
        assert_eq!(Preset::Long.seconds(), 60.0);
    }

    #[test]
    fn style_default() {
        let s = Style::default();
        assert_eq!((s.mode, s.blur, s.zoom, s.crop_x), (Mode::BlurBars, 22, 1.0, 0.5));
    }

    #[test]
    fn style_clamped() {
        let s = Style { mode: Mode::ZoomCrop, blur: 100, zoom: 3.0, crop_x: -1.0 }.clamped();
        assert_eq!((s.blur, s.zoom, s.crop_x), (40, 2.0, 0.0));
        assert_eq!(Style { blur: 1, ..Style::default() }.clamped().blur, 4);
    }
}
