// Release builds on Windows are GUI-subsystem apps (no console window behind the UI).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use eframe::egui;
use reelmaker::app::App;

#[cfg(windows)]
fn attach_parent_console() {
    // A GUI-subsystem exe has no console; attach the launching terminal's so `--version` can print.
    extern "system" {
        fn AttachConsole(process_id: u32) -> i32;
    }
    unsafe {
        AttachConsole(u32::MAX); // ATTACH_PARENT_PROCESS
    }
}
#[cfg(not(windows))]
fn attach_parent_console() {}

fn main() -> eframe::Result<()> {
    if std::env::args().nth(1).as_deref() == Some("--version") {
        attach_parent_console();
        println!("reelmaker {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([960.0, 640.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native("ReelMaker", options, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}
