mod common;

use reelmaker::thumbs::extract;
use std::path::Path;

#[test]
fn extracts_exactly_count_thumbnails() {
    let d = tempfile::tempdir().unwrap();
    let clip = common::make_clip(d.path(), 1280, 720, 16, false);
    let out = d.path().join("thumbs");
    let files = extract(Path::new("ffmpeg"), &clip, 16.0, 8, &out).unwrap();
    assert_eq!(files.len(), 8);
    for f in &files {
        assert!(std::fs::metadata(f).unwrap().len() > 0, "{f:?}");
        assert_eq!(f.extension().unwrap(), "jpg");
    }
    let mut sorted = files.clone();
    sorted.sort();
    assert_eq!(sorted, files, "must be in time order");
}

#[test]
fn clip_shorter_than_count_still_succeeds() {
    let d = tempfile::tempdir().unwrap();
    let clip = common::make_clip(d.path(), 640, 360, 2, false);
    let files = extract(Path::new("ffmpeg"), &clip, 2.0, 24, &d.path().join("t")).unwrap();
    assert_eq!(files.len(), 24);
    assert!(files.iter().all(|f| f.exists()));
}

#[test]
fn bad_input_is_an_error() {
    let d = tempfile::tempdir().unwrap();
    let junk = d.path().join("junk.mp4");
    std::fs::write(&junk, b"nope").unwrap();
    assert!(extract(Path::new("ffmpeg"), &junk, 5.0, 4, &d.path().join("t")).is_err());
}
