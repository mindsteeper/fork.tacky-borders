//! Settings window (egui/eframe), opened from the tray menu or the `settings` CLI command
//!   - winit allows only one event loop per process, so the GUI thread lives for the whole run:
//!     closing the window only hides it, reopening shows it again and re-reads config.yaml
//!   - IME stays enabled on this thread so Korean text input works (see main.rs)

mod app;
mod editors;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::{fs, thread};

use egui::{FontData, FontDefinitions, FontFamily, FontId, TextStyle, ViewportCommand};
use winit::platform::windows::EventLoopBuilderExtWindows;

use crate::display_error_box;

pub const WINDOW_TITLE: &str = "tacky-borders 설정";

/// egui context of the live window; None until the GUI thread has created it
static GUI_CTX: Mutex<Option<egui::Context>> = Mutex::new(None);
/// Set when the GUI thread is spawned; only reset if spawning itself failed
static GUI_STARTED: AtomicBool = AtomicBool::new(false);
/// Set when the event loop exited. It cannot be recreated, so reopening needs an app restart.
static GUI_DEAD: AtomicBool = AtomicBool::new(false);
/// Consumed by the app on its next frame: show the window and re-read config.yaml
static SHOW_REQUESTED: AtomicBool = AtomicBool::new(true);

/// Shows the settings window, creating the GUI thread on first use. Safe to call from any thread.
pub fn open() {
    SHOW_REQUESTED.store(true, Ordering::SeqCst);

    if GUI_DEAD.load(Ordering::SeqCst) {
        thread::spawn(|| {
            display_error_box(
                "설정 창을 다시 열 수 없습니다. tacky-borders 를 다시 시작해 주세요.",
                None,
            )
        });
        return;
    }

    let ctx_opt = GUI_CTX.lock().unwrap().clone();
    match ctx_opt {
        Some(ctx) => {
            ctx.send_viewport_cmd(ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(ViewportCommand::Focus);
            ctx.request_repaint();
        }
        None => spawn_gui_thread(),
    }
}

fn take_show_request() -> bool {
    SHOW_REQUESTED.swap(false, Ordering::SeqCst)
}

fn spawn_gui_thread() {
    if GUI_STARTED.swap(true, Ordering::SeqCst) {
        // The thread is already starting up and will pick up SHOW_REQUESTED itself
        return;
    }

    let spawn_res = thread::Builder::new()
        .name("settings-gui".to_string())
        .spawn(run_gui);

    if let Err(err) = spawn_res {
        error!("could not spawn settings gui thread: {err}");
        GUI_STARTED.store(false, Ordering::SeqCst);
    }
}

fn run_gui() {
    debug!("entering settings gui thread");

    let mut viewport = egui::ViewportBuilder::default()
        .with_title(WINDOW_TITLE)
        .with_inner_size([920.0, 680.0])
        .with_min_inner_size([760.0, 520.0]);

    match eframe::icon_data::from_png_bytes(include_bytes!("../resources/icon.png")) {
        Ok(icon) => viewport = viewport.with_icon(Arc::new(icon)),
        Err(err) => warn!("could not load settings window icon: {err}"),
    }

    let options = eframe::NativeOptions {
        viewport,
        centered: true,
        // The main thread is busy with the tray icon's message loop, so the event loop has to
        // be allowed on this worker thread
        event_loop_builder: Some(Box::new(|builder| {
            builder.with_any_thread(true);
        })),
        ..Default::default()
    };

    let result = eframe::run_native(
        "tacky-borders-settings",
        options,
        Box::new(|cc| {
            setup_style(&cc.egui_ctx);
            *GUI_CTX.lock().unwrap() = Some(cc.egui_ctx.clone());
            Ok(Box::new(app::SettingsApp::new()))
        }),
    );

    *GUI_CTX.lock().unwrap() = None;
    GUI_DEAD.store(true, Ordering::SeqCst);

    match result {
        Ok(()) => warn!("settings gui event loop exited; the window cannot be reopened"),
        Err(err) => error!("settings gui failed: {err}"),
    }

    debug!("exiting settings gui thread");
}

/// Korean-capable fonts plus slightly larger text than egui's defaults
fn setup_style(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();

    match load_korean_font() {
        Some(bytes) => {
            let name = "malgun".to_string();
            fonts
                .font_data
                .insert(name.clone(), Arc::new(FontData::from_owned(bytes)));
            fonts
                .families
                .entry(FontFamily::Proportional)
                .or_default()
                .insert(0, name.clone());
            // Keep the monospace font first; the Korean font is only a fallback for Hangul
            fonts
                .families
                .entry(FontFamily::Monospace)
                .or_default()
                .push(name);
        }
        None => warn!("could not load a Korean system font; Hangul will not render in settings"),
    }

    ctx.set_fonts(fonts);

    ctx.all_styles_mut(|style| {
        style
            .text_styles
            .insert(TextStyle::Small, FontId::proportional(11.5));
        style
            .text_styles
            .insert(TextStyle::Body, FontId::proportional(14.0));
        style
            .text_styles
            .insert(TextStyle::Button, FontId::proportional(14.0));
        style
            .text_styles
            .insert(TextStyle::Heading, FontId::proportional(20.0));
        style
            .text_styles
            .insert(TextStyle::Monospace, FontId::monospace(13.0));
        style.spacing.item_spacing = egui::vec2(8.0, 6.0);
        style.spacing.button_padding = egui::vec2(8.0, 4.0);
    });
}

/// Malgun Gothic ships with every supported Windows version, so no font is embedded
fn load_korean_font() -> Option<Vec<u8>> {
    let fonts_dir = std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("Fonts");

    ["malgun.ttf", "malgunsl.ttf"]
        .iter()
        .find_map(|name| fs::read(fonts_dir.join(name)).ok())
}
