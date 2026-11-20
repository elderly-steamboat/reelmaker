//! Timeline thumbnails: `count` evenly spaced small JPEGs from one ffmpeg run.
//! Call from a worker thread, never the UI thread.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Returns exactly `count` paths in time order (the last frame is repeated if ffmpeg yields fewer).
pub fn extract(
    ffmpeg: &Path,
    input: &Path,
    duration: f64,
    count: usize,
    out_dir: &Path,
) -> Result<Vec<PathBuf>, String> {
    if count == 0 || !(duration.is_finite() && duration > 0.0) {
        return Err("nothing to extract".into());
    }
    std::fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;
    // Clear stale thumbnails from an earlier import.
    for e in std::fs::read_dir(out_dir).map_err(|e| e.to_string())?.flatten() {
        if e.file_name().to_string_lossy().starts_with("thumb_") {
            let _ = std::fs::remove_file(e.path());
        }
    }
    let mut cmd = Command::new(ffmpeg);
    cmd.args(["-v", "error", "-nostdin", "-i"])
        .arg(input)
        .args(["-an", "-vf"])
        .arg(format!("fps={count}/{duration},scale=-2:64"))
        .args(["-frames:v", &count.to_string(), "-q:v", "5"])
        .arg(out_dir.join("thumb_%03d.jpg"));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let out = cmd.output().map_err(|e| format!("cannot run ffmpeg: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).lines().last().unwrap_or("ffmpeg failed").to_string());
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(out_dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "jpg"))
        .collect();
    files.sort();
    let last = files.last().cloned().ok_or("ffmpeg produced no thumbnails")?;
    files.truncate(count);
    while files.len() < count {
        files.push(last.clone());
    }
    Ok(files)
}
