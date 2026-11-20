mod common;

use reelmaker::filters::{reel_graph, Canvas};
use reelmaker::model::{Mode, Style};
use reelmaker::player::Player;
use std::time::{Duration, Instant};

fn wait_duration(p: &Player) -> f64 {
    let t = Instant::now();
    loop {
        if let Some(d) = p.duration() {
            return d;
        }
        assert!(t.elapsed() < Duration::from_secs(10), "file never loaded");
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn loaded(secs: u32) -> (Player, tempfile::TempDir) {
    let d = tempfile::tempdir().unwrap();
    let clip = common::make_clip(d.path(), 1280, 720, secs, true);
    let mut p = Player::new_headless().unwrap();
    p.set_paused(true);
    p.load(&clip).unwrap();
    wait_duration(&p);
    (p, d)
}

#[test]
fn loads_and_reports_duration() {
    let (p, _d) = loaded(5);
    assert!((p.duration().unwrap() - 5.0).abs() < 0.2);
}

#[test]
fn seek_then_position() {
    let (mut p, _d) = loaded(5);
    p.seek(2.0);
    std::thread::sleep(Duration::from_millis(400));
    assert!((p.position() - 2.0).abs() < 0.2, "{}", p.position());
}

#[test]
fn accepts_reel_graph_both_modes() {
    for mode in [Mode::BlurBars, Mode::ZoomCrop] {
        let (mut p, _d) = loaded(5);
        let g = reel_graph(&Style { mode, ..Style::default() }, Canvas { w: 270, h: 480 }, "vid1", "vo");
        p.set_reel_graph(Some(&g)).unwrap_or_else(|e| panic!("{mode:?}: {e}"));
        p.seek(0.0);
        p.set_paused(false);
        std::thread::sleep(Duration::from_millis(1500));
        assert!(p.position() > 0.3, "{mode:?}: position {}", p.position());
    }
}

#[test]
fn clearing_graph_ok() {
    let (mut p, _d) = loaded(3);
    let g = reel_graph(&Style::default(), Canvas { w: 270, h: 480 }, "vid1", "vo");
    p.set_reel_graph(Some(&g)).unwrap();
    p.set_reel_graph(None).unwrap();
}

#[test]
fn loop_window_wraps() {
    let (mut p, _d) = loaded(6);
    p.seek(1.0);
    p.set_loop(Some((1.0, 2.0)));
    p.set_paused(false);
    let t = Instant::now();
    let mut max_pos: f64 = 0.0;
    while t.elapsed() < Duration::from_secs(3) {
        max_pos = max_pos.max(p.position());
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(max_pos <= 2.2, "left the window: {max_pos}");
}
