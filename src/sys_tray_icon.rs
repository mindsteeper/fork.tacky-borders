use anyhow::Context;
use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, UnhookWinEvent};
use windows::Win32::UI::WindowsAndMessaging::PostQuitMessage;

use crate::auto_start::{is_autostart_enabled, toggle_autostart};
use crate::config::Config;
use crate::settings_gui;
use crate::utils::LogIfErr;
use crate::{BG_SERVICES, destroy_borders, reload_borders};

const MENU_SETTINGS: &str = "settings";
const MENU_OPEN_CONFIG_DIR: &str = "open_config_dir";
const MENU_AUTO_START: &str = "auto_start";
const MENU_RELOAD: &str = "reload";
const MENU_QUIT: &str = "quit";

pub fn create_tray_icon(hwineventhook: HWINEVENTHOOK) -> anyhow::Result<TrayIcon> {
    let icon = match Icon::from_resource(1, Some((64, 64))) {
        Ok(icon) => icon,
        Err(err) => {
            error!("could not retrieve icon from tacky-borders.exe for tray menu: {err:#}");

            // If we could not retrieve an icon from the exe, then try to create an empty icon. If
            // even that fails, then we'll just return an Error.
            let rgba: Vec<u8> = vec![0, 0, 0, 0];
            Icon::from_rgba(rgba, 1, 1).context("could not create empty tray icon")?
        }
    };

    let auto_enabled = is_autostart_enabled()?;

    let tray_menu = Menu::new();
    tray_menu.append_items(&[
        &MenuItem::with_id(MENU_SETTINGS, "설정...", true, None),
        &MenuItem::with_id(MENU_OPEN_CONFIG_DIR, "설정 폴더 열기", true, None),
        &PredefinedMenuItem::separator(),
        &CheckMenuItem::with_id(MENU_AUTO_START, "자동 시작", true, auto_enabled, None),
        &MenuItem::with_id(MENU_RELOAD, "다시 불러오기", true, None),
        &PredefinedMenuItem::separator(),
        &MenuItem::with_id(MENU_QUIT, "종료", true, None),
    ])?;

    let tooltip = format!("{}{}", "tacky-borders v", env!("CARGO_PKG_VERSION"));

    let tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip(tooltip)
        .with_icon(icon)
        .build();

    // Convert HWINEVENTHOOK to isize so we can move it into the event handler below
    let hwineventhook_isize = hwineventhook.0 as isize;

    // Handle tray icon events (i.e. clicking on the menu items)
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| match event.id.0.as_str() {
        MENU_SETTINGS => settings_gui::open(),
        MENU_OPEN_CONFIG_DIR => match Config::get_dir() {
            Ok(dir) => {
                open::that(dir).log_if_err();
            }
            Err(err) => error!("{err:#}"),
        },
        MENU_AUTO_START => {
            if let Err(err) = toggle_autostart() {
                error!("{err:#}")
            }
        }
        MENU_RELOAD => {
            Config::reload();
            reload_borders();
        }
        MENU_QUIT => {
            destroy_borders();

            // Convert hwineventhook_isize back into HWINEVENTHOOK, and unhook it
            let hwineventhook = HWINEVENTHOOK(hwineventhook_isize as _);
            unsafe { UnhookWinEvent(hwineventhook) }
                .ok()
                .context("could not unhook win event")
                .log_if_err();

            BG_SERVICES.lock().unwrap().shutdown();

            unsafe { PostQuitMessage(0) };
        }
        _ => {}
    }));

    tray_icon.map_err(anyhow::Error::new)
}
