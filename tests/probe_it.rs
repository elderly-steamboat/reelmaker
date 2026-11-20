mod common;

use reelmaker::probe::{probe, ProbeError};
use std::path::Path;

#[test]
fn probe_non_video_file_is_unreadable() {
    let dir = tempfile::tempdir().unwrap();
    let txt = dir.path().join("notes.txt");
    std::fs::write(&txt, "hello").unwrap();
    assert!(matches!(probe(Path::new("ffprobe"), &txt), Err(ProbeError::Unreadable(_))));
}

#[test]
fn probe_real_clip() {
    let dir = tempfile::tempdir().unwrap();
    let clip = common::make_clip(dir.path(), 1280, 720, 5, true);
    let m = probe(Path::new("ffprobe"), &clip).unwrap();
    assert!((m.duration - 5.0).abs() < 0.2, "{}", m.duration);
    assert_eq!((m.width, m.height, m.has_audio), (1280, 720, true));
}

#[test]
fn probe_rotated_clip_reports_display_size() {
    let dir = tempfile::tempdir().unwrap();
    let clip = common::make_clip_rotated(dir.path(), 1280, 720, 3);
    let m = probe(Path::new("ffprobe"), &clip).unwrap();
    assert_eq!((m.width, m.height), (720, 1280));
}
