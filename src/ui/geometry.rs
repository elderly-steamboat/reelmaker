//! Pure geometry for the timeline and the crop box (no egui types, so it is unit-testable).

/// Time -> x pixel in a track `width` wide spanning `0..duration`.
pub fn time_to_x(t: f64, duration: f64, width: f32) -> f32 {
    if duration <= 0.0 { 0.0 } else { (t / duration * width as f64) as f32 }
}

/// x pixel -> time, clamped to `0..=duration`.
pub fn x_to_time(x: f32, duration: f64, width: f32) -> f64 {
    if width <= 0.0 { 0.0 } else { (x as f64 / width as f64 * duration).clamp(0.0, duration) }
}

/// Where the window should start when the pointer is at `pointer_x` and the user grabbed the window
/// `grab_offset_secs` after its start. Not clamped to the window length (`ReelWindow::drag_to` does that).
pub fn window_start_from_pointer(pointer_x: f32, grab_offset_secs: f64, duration: f64, width: f32) -> f64 {
    x_to_time(pointer_x, duration, width) - grab_offset_secs
}

/// Crop-box centre as a 0..=1 fraction of the stage width.
pub fn crop_x_from_pointer(pointer_x: f32, stage_left: f32, stage_w: f32) -> f64 {
    if stage_w <= 0.0 { 0.5 } else { (((pointer_x - stage_left) / stage_w) as f64).clamp(0.0, 1.0) }
}

/// Width of the 9:16 crop box as a fraction of the source width. `source_aspect` = width / height.
pub fn crop_width_fraction(source_aspect: f64, zoom: f64) -> f64 {
    ((9.0 / 16.0) / (zoom * source_aspect)).min(1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_x_roundtrip() {
        for t in [0.0, 12.5, 90.0, 180.0] {
            let x = time_to_x(t, 180.0, 800.0);
            assert!((x_to_time(x, 180.0, 800.0) - t).abs() < 1e-3, "{t}");
        }
        assert_eq!(time_to_x(90.0, 180.0, 800.0), 400.0);
    }

    #[test]
    fn x_to_time_clamps() {
        assert_eq!(x_to_time(-50.0, 180.0, 800.0), 0.0);
        assert_eq!(x_to_time(5000.0, 180.0, 800.0), 180.0);
    }

    #[test]
    fn window_drag_keeps_grab_offset() {
        // pointer at 10s on a 100s/1000px track, grabbed 3s into the window -> start 7s
        let s = window_start_from_pointer(100.0, 3.0, 100.0, 1000.0);
        assert!((s - 7.0).abs() < 1e-9, "{s}");
    }

    #[test]
    fn crop_x_clamps() {
        assert_eq!(crop_x_from_pointer(-10.0, 100.0, 400.0), 0.0);
        assert_eq!(crop_x_from_pointer(900.0, 100.0, 400.0), 1.0);
        assert_eq!(crop_x_from_pointer(300.0, 100.0, 400.0), 0.5);
    }

    #[test]
    fn crop_width_16_9_zoom1() {
        assert!((crop_width_fraction(16.0 / 9.0, 1.0) - 0.3164).abs() < 1e-3);
    }

    #[test]
    fn crop_width_zoom2_halves() {
        let one = crop_width_fraction(16.0 / 9.0, 1.0);
        assert!((crop_width_fraction(16.0 / 9.0, 2.0) - one / 2.0).abs() < 1e-9);
    }

    #[test]
    fn crop_width_narrow_source_caps_at_1() {
        assert_eq!(crop_width_fraction(0.4, 1.0), 1.0);
    }
}
