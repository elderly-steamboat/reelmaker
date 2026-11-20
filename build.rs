//! Link/runtime setup for libmpv.
//!
//! Windows: expects `mpv.lib` + `libmpv-2.dll` in `$MPV_LIB_DIR` (default `vendor/mpv-dev`); the DLL is
//! copied next to the built binaries so `cargo run` / `cargo test` can load it.
//! Linux/macOS: libmpv comes from the system (pkg-config / standard lib dirs); the binary gets an
//! rpath so a bundled copy next to the executable is preferred.

use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=MPV_LIB_DIR");

    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    match std::env::var("CARGO_CFG_TARGET_OS").unwrap().as_str() {
        "windows" => {
            let dir = std::env::var("MPV_LIB_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| manifest.join("vendor").join("mpv-dev"));
            println!("cargo:rustc-link-search=native={}", dir.display());
            let dll = dir.join("libmpv-2.dll");
            if dll.exists() {
                // OUT_DIR = target/<profile>/build/<pkg>-<hash>/out  ->  target/<profile>
                let out = PathBuf::from(std::env::var("OUT_DIR").unwrap());
                if let Some(profile_dir) = out.ancestors().nth(3) {
                    copy_if_newer(&dll, &profile_dir.join("libmpv-2.dll"));
                    copy_if_newer(&dll, &profile_dir.join("deps").join("libmpv-2.dll"));
                }
            } else {
                println!("cargo:warning=libmpv-2.dll not found in {} (set MPV_LIB_DIR)", dir.display());
            }
        }
        "linux" => println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN"),
        "macos" => println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path"),
        _ => {}
    }
}

fn copy_if_newer(src: &Path, dst: &Path) {
    let stale = match (std::fs::metadata(src), std::fs::metadata(dst)) {
        (Ok(s), Ok(d)) => s.len() != d.len(),
        _ => true,
    };
    if stale {
        let _ = std::fs::copy(src, dst);
    }
}
