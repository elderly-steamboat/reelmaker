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
    let src = make_clip(dir, w, h, secs, true);
    let out = dir.join("clip_rot.mp4");
    run(&[
        "-display_rotation".into(), "90".into(),
        "-i".into(), src.to_string_lossy().into_owned(),
        "-c".into(), "copy".into(),
        out.to_string_lossy().into_owned(),
    ]);
    out
}
