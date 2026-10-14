//! Opens only the settings window, without borders or the tray icon, for GUI development
//!   - Run with `cargo run --example settings_window`
//!   - Set TACKY_BORDERS_CONFIG_HOME to point it at a scratch config directory;
//!     "저장 및 적용" still reloads borders in this process like the real app would

use std::{thread, time};

use sp_log::{ColorChoice, LevelFilter, TermLogger, TerminalMode};

fn main() {
    TermLogger::init(
        LevelFilter::Debug,
        sp_log::Config::default(),
        TerminalMode::Mixed,
        ColorChoice::Never,
    )
    .expect("could not init logger");

    tacky_borders::settings_gui::open();

    // The window lives on its own thread; keep the process alive until it is killed
    loop {
        thread::sleep(time::Duration::from_secs(1));
    }
}
