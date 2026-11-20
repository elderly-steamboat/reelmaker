//! Builds the 9:16 filter graph. Used by BOTH the exporter (ffmpeg `-filter_complex`)
//! and the live preview (mpv `lavfi-complex`), so preview matches output.

use crate::model::{Mode, Style};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Canvas {
    pub w: u32,
    pub h: u32,
}

impl Canvas {
    pub const EXPORT: Canvas = Canvas { w: 1080, h: 1920 };
}

/// `input` / `output` are bare labels, e.g. ("0:v", "vout") for ffmpeg or ("vid1", "vo") for mpv.
/// Result has the form `[input]...[output]`.
/// Like `reel_graph`, for mpv: mpv feeds `lavfi-complex` the *unrotated* decoded frames (ffmpeg
/// auto-rotates before `-filter_complex`), so rotation metadata has to be applied inside the graph.
/// `rotation` is counter-clockwise degrees (0/90/180/270), as in `MediaInfo::rotation`.
pub fn preview_graph(style: &Style, canvas: Canvas, input: &str, output: &str, rotation: u32) -> String {
    let filter = match rotation % 360 {
        90 => "transpose=cclock",
        180 => "hflip,vflip",
        270 => "transpose=clock",
        _ => return reel_graph(style, canvas, input, output),
    };
    format!("[{input}]{filter}[rotd];{}", reel_graph(style, canvas, "rotd", output))
}

pub fn reel_graph(style: &Style, canvas: Canvas, input: &str, output: &str) -> String {
    let (w, h) = (canvas.w, canvas.h);
    // Square-pixel normalisation first, so anamorphic sources are not stretched and every later
    // computation (crop, aspect fitting) works on display geometry.
    let square = format!("[{input}]scale=w='trunc(iw*sar/2)*2':h=ih,setsar=1");
    match style.mode {
        Mode::BlurBars => {
            // Keep the blur look constant across canvas sizes (preview is smaller than export).
            let radius = (style.blur as u64 * w as u64 / 1080).max(1);
            [
                format!("{square},split[a][b]"),
                format!(
                    "[a]scale={w}:{h}:force_original_aspect_ratio=increase,crop={w}:{h},boxblur={radius}:5[bg]"
                ),
                // Fit inside the canvas: sources taller than 9:16 must not lose their top/bottom.
                format!("[b]scale={w}:{h}:force_original_aspect_ratio=decrease[fg]"),
                format!("[bg][fg]overlay=(W-w)/2:(H-h)/2,setsar=1[{output}]"),
            ]
            .join(";")
        }
        Mode::ZoomCrop => {
            let z = style.zoom;
            let cx = style.crop_x;
            // Crop width never exceeds the frame: sources narrower than 9:16 crop at full width.
            let cw = format!("min(iw,ih*9/16/{z})");
            format!(
                "{square},crop=w='{cw}':h='{cw}*16/9':x='clip({cx}*iw-ow/2,0,iw-ow)':y='(ih-oh)/2',scale={w}:{h},setsar=1[{output}]"
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blur(b: u32) -> Style {
        Style { mode: Mode::BlurBars, blur: b, ..Style::default() }
    }

    #[test]
    fn blur_graph_matches_spec() {
        let g = reel_graph(&blur(22), Canvas::EXPORT, "0:v", "vout");
        assert!(g.starts_with("[0:v]"), "{g}");
        assert!(g.ends_with("[vout]"), "{g}");
        for part in [
            "split[a][b]",
            "scale=1080:1920:force_original_aspect_ratio=increase,crop=1080:1920,boxblur=22:5",
            "scale=1080:1920:force_original_aspect_ratio=decrease",
            "overlay=(W-w)/2:(H-h)/2",
        ] {
            assert!(g.contains(part), "missing {part} in {g}");
        }
    }

    #[test]
    fn blur_scales_with_canvas() {
        let g = reel_graph(&blur(22), Canvas { w: 270, h: 480 }, "vid1", "vo");
        assert!(g.contains("boxblur=5:"), "{g}");
        assert!(g.contains("scale=270:480:"), "{g}");
    }

    #[test]
    fn zoom_graph_matches_spec() {
        let s = Style { mode: Mode::ZoomCrop, zoom: 1.5, crop_x: 0.25, ..Style::default() };
        let g = reel_graph(&s, Canvas::EXPORT, "0:v", "vout");
        for part in ["ih*9/16/1.5", "scale=1080:1920", "0.25*iw"] {
            assert!(g.contains(part), "missing {part} in {g}");
        }
    }

    #[test]
    fn zoom_never_exceeds_frame() {
        let s = Style { mode: Mode::ZoomCrop, ..Style::default() };
        let g = reel_graph(&s, Canvas::EXPORT, "0:v", "vout");
        assert!(g.contains("min(iw"), "{g}");
        assert!(g.contains("*16/9"), "{g}");
    }

    #[test]
    fn crop_uses_ffmpeg_output_size_constants() {
        // `w`/`h` are not defined inside crop expressions; `ow`/`oh` are.
        let s = Style { mode: Mode::ZoomCrop, ..Style::default() };
        let g = reel_graph(&s, Canvas::EXPORT, "0:v", "vout");
        assert!(g.contains("clip(0.5*iw-ow/2,0,iw-ow)"), "{g}");
        assert!(g.contains("y='(ih-oh)/2'"), "{g}");
    }

    #[test]
    fn output_is_square_pixels() {
        for mode in [Mode::BlurBars, Mode::ZoomCrop] {
            let g = reel_graph(&Style { mode, ..Style::default() }, Canvas::EXPORT, "0:v", "vout");
            assert!(g.ends_with("setsar=1[vout]"), "{g}");
            // anamorphic input is normalised to square pixels before anything else
            assert!(g.starts_with("[0:v]scale=w='trunc(iw*sar/2)*2':h=ih,setsar=1,"), "{g}");
        }
    }

    #[test]
    fn blur_foreground_fits_inside_canvas() {
        // sources taller than 9:16 must not have their top/bottom cut off
        let g = reel_graph(&Style::default(), Canvas::EXPORT, "0:v", "vout");
        assert!(g.contains("[b]scale=1080:1920:force_original_aspect_ratio=decrease[fg]"), "{g}");
    }

    #[test]
    fn preview_graph_rotation_filters() {
        let s = Style::default();
        let c = Canvas { w: 270, h: 480 };
        assert_eq!(preview_graph(&s, c, "vid1", "vo", 0), reel_graph(&s, c, "vid1", "vo"));
        for (deg, f) in [(90, "transpose=cclock"), (180, "hflip,vflip"), (270, "transpose=clock")] {
            let g = preview_graph(&s, c, "vid1", "vo", deg);
            assert!(g.starts_with(&format!("[vid1]{f}[rotd];[rotd]")), "{deg}: {g}");
            assert!(g.ends_with("[vo]"), "{g}");
        }
    }

    #[test]
    fn graph_has_no_whitespace() {
        for mode in [Mode::BlurBars, Mode::ZoomCrop] {
            let g = reel_graph(&Style { mode, ..Style::default() }, Canvas::EXPORT, "0:v", "vout");
            assert!(!g.contains(char::is_whitespace), "{g}");
        }
    }

    #[test]
    fn labels_for_mpv() {
        for mode in [Mode::BlurBars, Mode::ZoomCrop] {
            let s = Style { mode, ..Style::default() };
            let g = reel_graph(&s, Canvas { w: 270, h: 480 }, "vid1", "vo");
            assert!(g.starts_with("[vid1]") && g.ends_with("[vo]"), "{g}");
        }
    }
}
