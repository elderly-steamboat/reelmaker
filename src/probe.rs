//! ffprobe wrapper: duration, display size, fps, audio presence.

use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, PartialEq)]
pub struct MediaInfo {
    pub duration: f64,
    /// Display dimensions (already swapped for 90/270 degree rotation).
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub has_audio: bool,
    /// Container rotation in counter-clockwise degrees: 0, 90, 180 or 270.
    pub rotation: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProbeError {
    Unreadable(String),
    NoVideo,
    BadDuration,
}

impl std::fmt::Display for ProbeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProbeError::Unreadable(e) => write!(f, "Could not read this file as a video ({e})"),
            ProbeError::NoVideo => write!(f, "This file has no video stream"),
            ProbeError::BadDuration => write!(f, "This video has no usable duration"),
        }
    }
}

/// `"32:27"` or `"32/27"` -> 32/27. Unknown (`0:1`, `N/A`) -> None.
fn parse_ratio(r: &str) -> Option<f64> {
    let (a, b) = r.split_once(':').or_else(|| r.split_once('/'))?;
    let (a, b): (f64, f64) = (a.parse().ok()?, b.parse().ok()?);
    (a > 0.0 && b > 0.0).then(|| a / b)
}

pub fn parse_probe(json: &str) -> Result<MediaInfo, ProbeError> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| ProbeError::Unreadable(e.to_string()))?;
    let streams = v["streams"].as_array().cloned().unwrap_or_default();
    let video = streams
        .iter()
        .find(|s| s["codec_type"] == "video")
        .ok_or(ProbeError::NoVideo)?;
    let has_audio = streams.iter().any(|s| s["codec_type"] == "audio");

    let num = |x: &serde_json::Value| -> Option<f64> {
        x.as_f64().or_else(|| x.as_str().and_then(|s| s.parse().ok()))
    };
    let duration = num(&v["format"]["duration"])
        .or_else(|| num(&video["duration"]))
        .filter(|d| d.is_finite() && *d > 0.0)
        .ok_or(ProbeError::BadDuration)?;

    let (mut width, mut height) = (
        video["width"].as_u64().unwrap_or(0) as u32,
        video["height"].as_u64().unwrap_or(0) as u32,
    );
    if width == 0 || height == 0 {
        return Err(ProbeError::NoVideo);
    }
    // Display width for non-square pixels; same rule as the filter graph's `trunc(iw*sar/2)*2`.
    if let Some(sar) = video["sample_aspect_ratio"].as_str().and_then(parse_ratio) {
        if (sar - 1.0).abs() > 1e-3 {
            width = ((width as f64 * sar / 2.0).trunc() as u32 * 2).max(2);
        }
    }

    // ffprobe's `rotation` is counter-clockwise degrees; the legacy `rotate` tag is clockwise.
    let raw = video["side_data_list"]
        .as_array()
        .and_then(|l| l.iter().find_map(|d| num(&d["rotation"])))
        .or_else(|| num(&video["tags"]["rotate"]).map(|r| -r))
        .unwrap_or(0.0);
    let rotation = (((raw.round() as i64) % 360 + 360) % 360) as u32;
    if rotation % 180 == 90 {
        std::mem::swap(&mut width, &mut height);
    }

    let fps = video["avg_frame_rate"]
        .as_str()
        .and_then(|r| {
            let (a, b) = r.split_once('/')?;
            let (a, b): (f64, f64) = (a.parse().ok()?, b.parse().ok()?);
            (b != 0.0).then(|| a / b)
        })
        .unwrap_or(0.0);

    Ok(MediaInfo { duration, width, height, fps, has_audio, rotation })
}

pub fn probe(ffprobe: &Path, file: &Path) -> Result<MediaInfo, ProbeError> {
    let out = Command::new(ffprobe)
        .args(["-v", "error", "-show_streams", "-show_format", "-of", "json"])
        .arg(file)
        .output()
        .map_err(|e| ProbeError::Unreadable(format!("cannot run ffprobe: {e}")))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(ProbeError::Unreadable(err.lines().next().unwrap_or("ffprobe failed").to_string()));
    }
    parse_probe(&String::from_utf8_lossy(&out.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fx(name: &str) -> String {
        std::fs::read_to_string(format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    #[test]
    fn parse_landscape() {
        let m = parse_probe(&fx("probe_landscape.json")).unwrap();
        assert_eq!((m.width, m.height, m.has_audio), (1920, 1080, true));
        assert!((m.fps - 29.97).abs() < 0.01, "{}", m.fps);
        assert!((m.duration - 12.5).abs() < 1e-6);
    }

    #[test]
    fn parse_rotated_swaps_dims() {
        let m = parse_probe(&fx("probe_rotated.json")).unwrap();
        assert_eq!((m.width, m.height), (1080, 1920));
    }

    #[test]
    fn parse_rotation_normalised_to_ccw_degrees() {
        assert_eq!(parse_probe(&fx("probe_rotated.json")).unwrap().rotation, 270);
        assert_eq!(parse_probe(&fx("probe_landscape.json")).unwrap().rotation, 0);
        let tag = r#"{"streams":[{"codec_type":"video","width":1920,"height":1080,"avg_frame_rate":"30/1","tags":{"rotate":"90"}}],"format":{"duration":"4.0"}}"#;
        // old-style `rotate` tag is clockwise degrees; ffprobe's `rotation` is counter-clockwise
        let m = parse_probe(tag).unwrap();
        assert_eq!((m.rotation, m.width, m.height), (270, 1080, 1920));
    }

    #[test]
    fn parse_anamorphic_uses_display_width() {
        let j = r#"{"streams":[{"codec_type":"video","width":720,"height":480,"sample_aspect_ratio":"32:27","avg_frame_rate":"30/1"}],"format":{"duration":"4.0"}}"#;
        let m = parse_probe(j).unwrap();
        assert_eq!((m.width, m.height), (852, 480)); // trunc(720*32/27 / 2) * 2, same rule as the graph
    }

    #[test]
    fn parse_unknown_sar_is_square() {
        for sar in ["0:1", "N/A", "1:1"] {
            let j = format!(r#"{{"streams":[{{"codec_type":"video","width":640,"height":360,"sample_aspect_ratio":"{sar}","avg_frame_rate":"30/1"}}],"format":{{"duration":"4.0"}}}}"#);
            assert_eq!(parse_probe(&j).unwrap().width, 640, "{sar}");
        }
    }

    #[test]
    fn parse_no_audio() {
        assert!(!parse_probe(&fx("probe_noaudio.json")).unwrap().has_audio);
    }

    #[test]
    fn parse_no_video_stream() {
        let j = r#"{"streams":[{"codec_type":"audio"}],"format":{"duration":"3.0"}}"#;
        assert_eq!(parse_probe(j), Err(ProbeError::NoVideo));
    }

    #[test]
    fn zero_duration_rejected() {
        let j = r#"{"streams":[{"codec_type":"video","width":10,"height":10,"avg_frame_rate":"30/1"}],"format":{"duration":"0.0"}}"#;
        assert_eq!(parse_probe(j), Err(ProbeError::BadDuration));
    }

    #[test]
    fn garbage_json() {
        assert!(matches!(parse_probe("not json"), Err(ProbeError::Unreadable(_))));
    }
}
