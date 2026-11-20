//! Export: one ffmpeg run producing the 1080x1920 reel, with progress and cancel.
//!
//! ffmpeg writes to `<output>.partial`; the file is renamed to `output` only on success, so a
//! cancelled or failed export never leaves a partial file and never clobbers an existing one.

use crate::filters::{reel_graph, Canvas};
use crate::model::Style;
use crate::tools::Tools;
use std::collections::VecDeque;
use std::ffi::OsString;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};

#[derive(Debug, Clone)]
pub struct ExportJob {
    pub input: PathBuf,
    pub output: PathBuf,
    pub start: f64,
    pub length: f64,
    pub style: Style,
    pub has_audio: bool,
    pub overwrite: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExportError {
    SameAsSource,
    OutputDirMissing,
    Exists,
    Spawn(String),
}

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExportError::SameAsSource => write!(f, "The output file is the source video. Choose a different name."),
            ExportError::OutputDirMissing => write!(f, "The output folder does not exist."),
            ExportError::Exists => write!(f, "The output file already exists."),
            ExportError::Spawn(e) => write!(f, "Could not start ffmpeg: {e}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ExportEvent {
    /// 0.0..=1.0
    Progress(f64),
    Done,
    /// Last lines of ffmpeg's stderr.
    Failed(String),
    Cancelled,
}

pub struct ExportHandle {
    pub events: mpsc::Receiver<ExportEvent>,
    cancelled: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
    waiter: Option<std::thread::JoinHandle<()>>,
    finished: Arc<AtomicBool>,
}

impl Drop for ExportHandle {
    /// Abandoning a running export (e.g. the app window was closed) stops ffmpeg and removes the
    /// `.partial` file before returning, so nothing keeps encoding invisibly in the background.
    fn drop(&mut self) {
        if !self.finished.load(Ordering::SeqCst) {
            self.cancel();
        }
        if let Some(w) = self.waiter.take() {
            let _ = w.join();
        }
    }
}

impl ExportHandle {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        if let Some(c) = self.child.lock().unwrap().as_mut() {
            let _ = c.kill();
        }
    }
}

pub fn validate(job: &ExportJob) -> Result<(), ExportError> {
    let dir = match job.output.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => PathBuf::from("."),
    };
    let dir = dir.canonicalize().map_err(|_| ExportError::OutputDirMissing)?;
    let name = job.output.file_name().ok_or(ExportError::OutputDirMissing)?;
    let out = dir.join(name);
    if let Ok(src) = job.input.canonicalize() {
        // `out` keeps the user's typed spelling; an existing file may still be the source under a
        // different case (Windows/macOS) or via a symlink, so compare its resolved path too.
        if src == out || out.canonicalize().is_ok_and(|resolved| resolved == src) {
            return Err(ExportError::SameAsSource);
        }
    }
    if out.exists() && !job.overwrite {
        return Err(ExportError::Exists);
    }
    Ok(())
}

pub fn build_args(job: &ExportJob) -> Vec<OsString> {
    let graph = reel_graph(&job.style, Canvas::EXPORT, "0:v", "vout");
    let mut a: Vec<OsString> = Vec::new();
    let mut push = |xs: &[&str]| a.extend(xs.iter().map(OsString::from));
    push(&["-hide_banner", "-nostdin", "-ss"]);
    push(&[&job.start.to_string(), "-t", &job.length.to_string(), "-i"]);
    a.push(job.input.clone().into_os_string());
    let mut push = |xs: &[&str]| a.extend(xs.iter().map(OsString::from));
    push(&["-filter_complex", &graph, "-map", "[vout]"]);
    if job.has_audio {
        push(&["-map", "0:a?"]);
    }
    push(&["-c:v", "libx264", "-crf", "18", "-preset", "medium", "-pix_fmt", "yuv420p", "-r", "30"]);
    if job.has_audio {
        push(&["-c:a", "aac", "-b:a", "160k"]);
    }
    push(&["-movflags", "+faststart", "-progress", "pipe:1", "-nostats"]);
    push(&[if job.overwrite { "-y" } else { "-n" }, "-f", "mp4"]);
    a.push(job.output.clone().into_os_string());
    a
}

/// `out_time_us=N` -> seconds. Anything else (including `N/A`) -> None.
pub fn parse_progress_line(line: &str) -> Option<f64> {
    let us: i64 = line.trim().strip_prefix("out_time_us=")?.parse().ok()?;
    (us >= 0).then(|| us as f64 / 1_000_000.0)
}

fn partial_path(output: &PathBuf) -> PathBuf {
    let mut name = output.file_name().unwrap_or_default().to_os_string();
    name.push(".partial");
    output.with_file_name(name)
}

pub fn start(tools: &Tools, job: ExportJob) -> Result<ExportHandle, ExportError> {
    validate(&job)?;
    let final_out = job.output.clone();
    let tmp = partial_path(&final_out);
    let length = job.length;
    // ffmpeg writes to the partial file; it is always safe to overwrite our own partial.
    let run_job = ExportJob { output: tmp.clone(), overwrite: true, ..job };

    let mut cmd = Command::new(&tools.ffmpeg);
    cmd.args(build_args(&run_job))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let mut child = cmd.spawn().map_err(|e| ExportError::Spawn(e.to_string()))?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let child = Arc::new(Mutex::new(Some(child)));
    let cancelled = Arc::new(AtomicBool::new(false));
    let (tx, rx) = mpsc::channel();

    let tail: Arc<Mutex<VecDeque<String>>> = Arc::new(Mutex::new(VecDeque::new()));
    let tail_w = tail.clone();
    let err_thread = std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            let mut t = tail_w.lock().unwrap();
            t.push_back(line);
            if t.len() > 10 {
                t.pop_front();
            }
        }
    });

    let (child_w, cancelled_w) = (child.clone(), cancelled.clone());
    let finished = Arc::new(AtomicBool::new(false));
    let finished_w = finished.clone();
    let waiter = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if let Some(t) = parse_progress_line(&line) {
                let _ = tx.send(ExportEvent::Progress((t / length).clamp(0.0, 1.0)));
            }
        }
        let status = child_w.lock().unwrap().take().and_then(|mut c| c.wait().ok());
        let _ = err_thread.join();
        let ok = status.map(|s| s.success()).unwrap_or(false);
        let event = if cancelled_w.load(Ordering::SeqCst) {
            let _ = std::fs::remove_file(&tmp);
            ExportEvent::Cancelled
        } else if ok {
            match std::fs::rename(&tmp, &final_out) {
                Ok(()) => ExportEvent::Done,
                Err(e) => {
                    let _ = std::fs::remove_file(&tmp);
                    ExportEvent::Failed(format!("could not write {}: {e}", final_out.display()))
                }
            }
        } else {
            let _ = std::fs::remove_file(&tmp);
            let msg = tail.lock().unwrap().iter().cloned().collect::<Vec<_>>().join("\n");
            ExportEvent::Failed(if msg.is_empty() { "ffmpeg failed".into() } else { msg })
        };
        let _ = tx.send(event);
        finished_w.store(true, Ordering::SeqCst);
    });

    Ok(ExportHandle { events: rx, cancelled, child, waiter: Some(waiter), finished })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Mode;

    fn job(input: &str, output: &str) -> ExportJob {
        ExportJob {
            input: input.into(),
            output: output.into(),
            start: 12.0,
            length: 30.0,
            style: Style { mode: Mode::BlurBars, ..Style::default() },
            has_audio: true,
            overwrite: false,
        }
    }

    fn args_str(j: &ExportJob) -> Vec<String> {
        build_args(j).iter().map(|a| a.to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn progress_parses_us() {
        assert_eq!(parse_progress_line("out_time_us=2500000"), Some(2.5));
    }

    #[test]
    fn progress_ignores_na_and_other_keys() {
        assert_eq!(parse_progress_line("out_time_us=N/A"), None);
        assert_eq!(parse_progress_line("frame=42"), None);
        assert_eq!(parse_progress_line("out_time_us=-9223372036854775807"), None);
    }

    #[test]
    fn args_contain_codec_flags() {
        let a = args_str(&job("in.mp4", "out.mp4")).join(" ");
        for flag in [
            "-c:v libx264", "-crf 18", "-preset medium", "-pix_fmt yuv420p", "-r 30",
            "-c:a aac", "-b:a 160k", "-movflags +faststart", "-progress pipe:1", "-ss 12", "-t 30",
        ] {
            assert!(a.contains(flag), "missing {flag} in {a}");
        }
        assert!(a.contains("-n "), "{a}");
        assert!(a.ends_with("out.mp4"), "{a}");
        let mut j = job("in.mp4", "out.mp4");
        j.overwrite = true;
        assert!(args_str(&j).contains(&"-y".to_string()));
    }

    #[test]
    fn args_input_before_filter() {
        let a = args_str(&job("in.mp4", "out.mp4"));
        let i = a.iter().position(|x| x == "-i").unwrap();
        let f = a.iter().position(|x| x == "-filter_complex").unwrap();
        assert!(i < f);
        assert!(a[i + 1] == "in.mp4");
        assert!(a[f + 1].starts_with("[0:v]") && a[f + 1].ends_with("[vout]"));
    }

    #[test]
    fn args_omit_audio_when_source_has_none() {
        let mut j = job("in.mp4", "out.mp4");
        j.has_audio = false;
        let a = args_str(&j).join(" ");
        assert!(!a.contains("aac") && !a.contains("0:a"), "{a}");
    }

    #[test]
    fn validate_same_as_source() {
        let d = tempfile::tempdir().unwrap();
        let f = d.path().join("v.mp4");
        std::fs::write(&f, b"x").unwrap();
        let mut j = job(f.to_str().unwrap(), f.to_str().unwrap());
        j.overwrite = true;
        assert_eq!(validate(&j), Err(ExportError::SameAsSource));
    }

    #[test]
    fn validate_exists_without_overwrite() {
        let d = tempfile::tempdir().unwrap();
        let (src, out) = (d.path().join("v.mp4"), d.path().join("o.mp4"));
        std::fs::write(&src, b"x").unwrap();
        std::fs::write(&out, b"y").unwrap();
        let mut j = job(src.to_str().unwrap(), out.to_str().unwrap());
        assert_eq!(validate(&j), Err(ExportError::Exists));
        j.overwrite = true;
        assert_eq!(validate(&j), Ok(()));
    }

    #[test]
    fn validate_output_dir_missing() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("v.mp4");
        std::fs::write(&src, b"x").unwrap();
        let out = d.path().join("nope").join("o.mp4");
        assert_eq!(validate(&job(src.to_str().unwrap(), out.to_str().unwrap())), Err(ExportError::OutputDirMissing));
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn validate_same_source_with_different_case() {
        let d = tempfile::tempdir().unwrap();
        let src = d.path().join("Video.mp4");
        std::fs::write(&src, b"x").unwrap();
        let mut j = job(src.to_str().unwrap(), d.path().join("video.MP4").to_str().unwrap());
        j.overwrite = true;
        assert_eq!(validate(&j), Err(ExportError::SameAsSource));
    }
}
