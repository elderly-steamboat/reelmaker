//! Test helpers: generate small clips with the system ffmpeg.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

fn run(args: &[String]) {
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-y"])
        .args(args)
        .output()
        .expect("ffmpeg not found on PATH");
    assert!(out.status.success(), "ffmpeg failed: {}", String::from_utf8_lossy(&out.stderr));
}

/// `dir/clip.mp4`: testsrc2 video, optional sine audio.
pub fn make_clip(dir: &Path, w: u32, h: u32, secs: u32, audio: bool) -> PathBuf {
    let out = dir.join("clip.mp4");
    let mut a: Vec<String> = vec![
        "-f".into(), "lavfi".into(), "-i".into(),
        format!("testsrc2=size={w}x{h}:rate=30:duration={secs}"),
    ];
    if audio {
        a.extend(["-f".into(), "lavfi".into(), "-i".into(), format!("sine=duration={secs}")]);
    }
    a.extend(["-c:v".into(), "libx264".into(), "-pix_fmt".into(), "yuv420p".into()]);
    if audio {
        a.extend(["-c:a".into(), "aac".into(), "-shortest".into()]);
    }
    a.push(out.to_string_lossy().into_owned());
    run(&a);
    out
}

/// Like `make_clip` (with audio) but tagged with a 90 degree display-matrix rotation.
pub fn make_clip_rotated(dir: &Path, w: u32, h: u32, secs: u32) -> PathBuf {
    make_clip_rotated_by(dir, w, h, secs, 90)
}

/// Same, with an explicit display-matrix rotation in counter-clockwise degrees.
/// Uses `-display_rotation` (ffmpeg >= 6.0) and falls back to the legacy clockwise `rotate`
/// metadata tag for older ffmpeg builds (e.g. Ubuntu 22.04's 4.4).
pub fn make_clip_rotated_by(dir: &Path, w: u32, h: u32, secs: u32, degrees: u32) -> PathBuf {
    let src = make_clip(dir, w, h, secs, true);
    let out = dir.join("clip_rot.mp4");
    let src_s = src.to_string_lossy().into_owned();
    let out_s = out.to_string_lossy().into_owned();
    let modern = Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-display_rotation", &degrees.to_string(), "-i", &src_s, "-c", "copy", &out_s])
        .output()
        .expect("ffmpeg not found on PATH");
    if !modern.status.success() {
        run(&[
            "-i".into(), src_s,
            "-c".into(), "copy".into(),
            "-metadata:s:v:0".into(), format!("rotate={}", (360 - degrees % 360) % 360),
            out_s,
        ]);
    }
    out
}

/// `dir/clip_ana.mp4`: stored w x h with a non-square sample aspect ratio (e.g. 32/27 like DV PAL-ish).
pub fn make_clip_anamorphic(dir: &Path, w: u32, h: u32, secs: u32, sar: &str) -> PathBuf {
    let out = dir.join("clip_ana.mp4");
    run(&[
        "-f".into(), "lavfi".into(), "-i".into(),
        format!("testsrc2=size={w}x{h}:rate=30:duration={secs},setsar={sar}"),
        "-c:v".into(), "libx264".into(), "-pix_fmt".into(), "yuv420p".into(),
        out.to_string_lossy().into_owned(),
    ]);
    out
}

/// First video stream's `sample_aspect_ratio` as printed by ffprobe (e.g. "1:1").
pub fn sample_aspect_ratio(file: &Path) -> String {
    let out = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "v:0", "-show_entries", "stream=sample_aspect_ratio", "-of", "csv=p=0"])
        .arg(file)
        .output()
        .expect("ffprobe not found on PATH");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}
