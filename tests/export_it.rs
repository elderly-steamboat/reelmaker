mod common;

use reelmaker::export::{start, ExportEvent, ExportJob};
use reelmaker::model::{Mode, Style};
use reelmaker::probe::{probe, MediaInfo};
use reelmaker::tools::Tools;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn tools() -> Tools {
    Tools { ffmpeg: "ffmpeg".into(), ffprobe: "ffprobe".into() }
}

fn info(p: &Path) -> MediaInfo {
    probe(Path::new("ffprobe"), p).unwrap()
}

fn job(input: &Path, output: PathBuf, start: f64, length: f64, mode: Mode, has_audio: bool) -> ExportJob {
    ExportJob {
        input: input.into(),
        output,
        start,
        length,
        style: Style { mode, ..Style::default() },
        has_audio,
        overwrite: false,
    }
}

/// Runs an export to completion, returning every event.
fn run(j: ExportJob) -> Vec<ExportEvent> {
    let h = start(&tools(), j).expect("start");
    let mut evs = vec![];
    loop {
        let e = h.events.recv_timeout(Duration::from_secs(120)).expect("export timed out");
        let end = !matches!(e, ExportEvent::Progress(_));
        evs.push(e);
        if end {
            return evs;
        }
    }
}

#[test]
fn every_source_shape_exports_1080x1920() {
    let shapes = [(1920, 1080), (1080, 1920), (720, 720), (400, 1000)];
    for (w, h) in shapes {
        for mode in [Mode::BlurBars, Mode::ZoomCrop] {
            let d = tempfile::tempdir().unwrap();
            let src = common::make_clip(d.path(), w, h, 4, true);
            let out = d.path().join("reel.mp4");
            let evs = run(job(&src, out.clone(), 0.5, 3.0, mode, true));
            assert_eq!(evs.last(), Some(&ExportEvent::Done), "{w}x{h} {mode:?}: {evs:?}");
            let m = info(&out);
            assert_eq!((m.width, m.height), (1080, 1920), "{w}x{h} {mode:?}");
            assert!((m.duration - 3.0).abs() < 0.3, "{w}x{h} {mode:?}: {}", m.duration);
        }
    }
}

#[test]
fn no_audio_source_exports() {
    let d = tempfile::tempdir().unwrap();
    let src = common::make_clip(d.path(), 1280, 720, 3, false);
    let out = d.path().join("reel.mp4");
    assert_eq!(run(job(&src, out.clone(), 0.0, 2.0, Mode::BlurBars, false)).last(), Some(&ExportEvent::Done));
    assert!(!info(&out).has_audio);
}

#[test]
fn audio_source_keeps_audio() {
    let d = tempfile::tempdir().unwrap();
    let src = common::make_clip(d.path(), 1280, 720, 3, true);
    let out = d.path().join("reel.mp4");
    assert_eq!(run(job(&src, out.clone(), 0.0, 2.0, Mode::ZoomCrop, true)).last(), Some(&ExportEvent::Done));
    assert!(info(&out).has_audio);
}

#[test]
fn rotated_source_exports_vertical() {
    let d = tempfile::tempdir().unwrap();
    let src = common::make_clip_rotated(d.path(), 1280, 720, 3);
    let out = d.path().join("reel.mp4");
    assert_eq!(run(job(&src, out.clone(), 0.0, 2.0, Mode::ZoomCrop, true)).last(), Some(&ExportEvent::Done));
    let m = info(&out);
    assert_eq!((m.width, m.height), (1080, 1920));
}

#[test]
fn source_untouched_and_no_partial_left() {
    let d = tempfile::tempdir().unwrap();
    let src = common::make_clip(d.path(), 1280, 720, 3, true);
    let before = std::fs::read(&src).unwrap();
    let out = d.path().join("reel.mp4");
    run(job(&src, out.clone(), 0.0, 2.0, Mode::BlurBars, true));
    assert_eq!(std::fs::read(&src).unwrap(), before);
    let leftovers: Vec<_> = std::fs::read_dir(d.path()).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert!(!leftovers.iter().any(|n| n.to_string_lossy().contains("partial")), "{leftovers:?}");
}

#[test]
fn progress_reaches_one_before_done() {
    let d = tempfile::tempdir().unwrap();
    let src = common::make_clip(d.path(), 1280, 720, 4, true);
    let evs = run(job(&src, d.path().join("reel.mp4"), 0.0, 3.0, Mode::BlurBars, true));
    let last_progress = evs.iter().rev().find_map(|e| if let ExportEvent::Progress(p) = e { Some(*p) } else { None });
    assert!(last_progress.unwrap_or(0.0) >= 0.9, "{evs:?}");
    assert_eq!(evs.last(), Some(&ExportEvent::Done));
}

#[test]
fn cancel_removes_partial_and_keeps_existing_output() {
    let d = tempfile::tempdir().unwrap();
    let src = common::make_clip(d.path(), 1920, 1080, 120, false);
    let out = d.path().join("reel.mp4");
    std::fs::write(&out, b"precious").unwrap();
    let mut j = job(&src, out.clone(), 0.0, 60.0, Mode::BlurBars, false);
    j.overwrite = true;
    let h = start(&tools(), j).unwrap();
    // wait until encoding is actually under way
    loop {
        match h.events.recv_timeout(Duration::from_secs(60)).unwrap() {
            ExportEvent::Progress(_) => break,
            other => panic!("ended before cancel: {other:?}"),
        }
    }
    h.cancel();
    loop {
        match h.events.recv_timeout(Duration::from_secs(60)).unwrap() {
            ExportEvent::Cancelled => break,
            ExportEvent::Progress(_) => continue,
            other => panic!("expected Cancelled, got {other:?}"),
        }
    }
    assert_eq!(std::fs::read(&out).unwrap(), b"precious");
    let partials = std::fs::read_dir(d.path()).unwrap().filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().contains("partial")).count();
    assert_eq!(partials, 0);
}

#[test]
fn bad_input_fails_with_stderr_tail() {
    let d = tempfile::tempdir().unwrap();
    let junk = d.path().join("junk.mp4");
    std::fs::write(&junk, b"not a video").unwrap();
    let evs = run(job(&junk, d.path().join("reel.mp4"), 0.0, 2.0, Mode::BlurBars, false));
    assert!(matches!(evs.last(), Some(ExportEvent::Failed(m)) if !m.is_empty()), "{evs:?}");
    assert!(!d.path().join("reel.mp4").exists());
}
