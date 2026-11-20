@echo off
rem Dev-only cargo runner (Windows): lets `cargo run` / `cargo test` find libmpv-2.dll.
rem Release bundles ship the DLL next to the exe instead.
if defined MPV_LIB_DIR (set "PATH=%MPV_LIB_DIR%;%PATH%") else (set "PATH=%~dp0..\vendor\mpv-dev;%PATH%")
%*
