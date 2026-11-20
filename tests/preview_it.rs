mod common;

use reelmaker::filters::{preview_graph, reel_graph, Canvas};
use reelmaker::model::{Mode, Style};
use reelmaker::probe::probe;
use std::path::Path;
use std::process::Command;

/// md5 of the first output frame of `ffmpeg <pre_args> -i clip -filter_complex graph`.
fn first_frame_md5(clip: &Path, pre: &[&str], graph: &str) -> String {
    let out = Command::new("ffmpeg")
        .args(["-v", "error"])
        .args(pre)
        .arg("-i")
        .arg(clip)
        .args(["-filter_complex", graph, "-map", "[vout]", "-frames:v", "1", "-f", "md5", "-"])
        .output()
        .expect("ffmpeg not found on PATH");
    assert!(out.status.success(), "ffmpeg failed: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// mpv hands `lavfi-complex` the unrotated frames; the preview graph must therefore rotate them
/// itself so the preview shows exactly what the export (which ffmpeg auto-rotates) produces.
#[test]
fn preview_graph_matches_export_for_every_rotation() {
    let canvas = Canvas { w: 270, h: 480 };
    for degrees in [90u32, 180, 270] {
        let d = tempfile::tempdir().unwrap();
        let clip = common::make_clip_rotated_by(d.path(), 640, 360, 2, degrees);
        let rotation = probe(Path::new("ffprobe"), &clip).unwrap().rotation;
        for mode in [Mode::BlurBars, Mode::ZoomCrop] {
            let style = Style { mode, crop_x: 0.2, zoom: 1.3, ..Style::default() };
            let export = first_frame_md5(&clip, &[], &reel_graph(&style, canvas, "0:v", "vout"));
            let preview = first_frame_md5(&clip, &["-noautorotate"], &preview_graph(&style, canvas, "0:v", "vout", rotation));
            assert_eq!(export, preview, "rotation {degrees} (probed {rotation}), {mode:?}");
        }
    }
}
