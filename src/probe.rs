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
    let rotation = video["side_data_list"]
        .as_array()
        .and_then(|l| l.iter().find_map(|d| num(&d["rotation"])))
        .or_else(|| num(&video["tags"]["rotate"]))
        .unwrap_or(0.0);
    if (rotation.abs().round() as i64) % 180 == 90 {
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

    Ok(MediaInfo { duration, width, height, fps, has_audio })
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
