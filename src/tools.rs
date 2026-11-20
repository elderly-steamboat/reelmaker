//! Locating the ffmpeg / ffprobe sidecar binaries: next to the exe first, then PATH.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct Tools {
    pub ffmpeg: PathBuf,
    pub ffprobe: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolsError {
    pub searched: Vec<PathBuf>,
}

impl std::fmt::Display for ToolsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "ffmpeg and ffprobe were not found. Searched:")?;
        for d in &self.searched {
            writeln!(f, "  {}", d.display())?;
        }
        Ok(())
    }
}

fn exe_name(base: &str) -> String {
    if cfg!(windows) { format!("{base}.exe") } else { base.to_string() }
}

pub fn locate(exe_dir: &Path, path_var: &OsStr) -> Result<Tools, ToolsError> {
    let mut searched = vec![exe_dir.to_path_buf()];
    searched.extend(std::env::split_paths(path_var));
    let find = |base: &str| {
        let name = exe_name(base);
        searched.iter().map(|d| d.join(&name)).find(|p| p.is_file())
    };
    match (find("ffmpeg"), find("ffprobe")) {
        (Some(ffmpeg), Some(ffprobe)) => Ok(Tools { ffmpeg, ffprobe }),
        _ => Err(ToolsError { searched }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(dir: &Path, base: &str) {
        std::fs::write(dir.join(exe_name(base)), b"").unwrap();
    }

    fn path_of(dirs: &[&Path]) -> std::ffi::OsString {
        std::env::join_paths(dirs).unwrap()
    }

    #[test]
    fn prefers_exe_dir() {
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        for d in [a.path(), b.path()] {
            touch(d, "ffmpeg");
            touch(d, "ffprobe");
        }
        let t = locate(a.path(), &path_of(&[b.path()])).unwrap();
        assert_eq!(t.ffmpeg, a.path().join(exe_name("ffmpeg")));
        assert_eq!(t.ffprobe, a.path().join(exe_name("ffprobe")));
    }

    #[test]
    fn falls_back_to_path() {
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        touch(b.path(), "ffmpeg");
        touch(b.path(), "ffprobe");
        let t = locate(a.path(), &path_of(&[b.path()])).unwrap();
        assert_eq!(t.ffmpeg, b.path().join(exe_name("ffmpeg")));
    }

    #[test]
    fn missing_lists_searched_dirs() {
        let (a, b, c) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        let err = locate(a.path(), &path_of(&[b.path(), c.path()])).unwrap_err();
        for d in [a.path(), b.path(), c.path()] {
            assert!(err.searched.iter().any(|s| s == d), "{d:?} not in {:?}", err.searched);
        }
        assert!(err.to_string().contains(&*a.path().to_string_lossy()));
    }

    #[test]
    fn needs_both() {
        let (a, b) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
        touch(a.path(), "ffmpeg");
        assert!(locate(a.path(), &path_of(&[b.path()])).is_err());
    }
}
