//! The `Tray` adapter: Tauri's tray icon on macOS and Windows. The engine builds the menu; this
//! only draws it and reports clicks.
//!
//! macOS: a click shows the menu. Windows: a left click opens Muesli's window and a right click
//! shows the menu, as tray apps do there. While transcribing the icon carries a red dot.

use std::sync::{Arc, Mutex, OnceLock};

use muesli_core::ports::{Tray, TrayItem, TrayMenu};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::AppHandle;

use crate::host::TauriHost;

const TRAY_ID: &str = "main";
/// The template image (black, tinted by macOS) and the same with a red dot while transcribing.
const IDLE: &[u8] = include_bytes!("../icons/tray.png");
const TRANSCRIBING: &[u8] = include_bytes!("../icons/tray-transcribing.png");
/// Windows: milk-coloured versions for a dark taskbar (the default on Windows 11).
#[cfg(windows)]
const IDLE_LIGHT: &[u8] = include_bytes!("../icons/tray-light.png");
#[cfg(windows)]
const TRANSCRIBING_LIGHT: &[u8] = include_bytes!("../icons/tray-light-transcribing.png");

/// The icon for the current state, and whether macOS may tint it (a template image).
fn icon_for(transcribing: bool) -> (&'static [u8], bool) {
    #[cfg(windows)]
    if !muesli_platform_windows::theme::taskbar_is_light() {
        return (if transcribing { TRANSCRIBING_LIGHT } else { IDLE_LIGHT }, false);
    }
    // The red dot cannot be a template image (macOS would tint it black).
    if transcribing {
        (TRANSCRIBING, false)
    } else {
        (IDLE, true)
    }
}

/// The handle only exists once Tauri runs, so drawing waits until `attach`.
#[derive(Default)]
pub struct TauriTray {
    handle: OnceLock<AppHandle>,
    shown: Mutex<Option<TrayMenu>>,
}

impl TauriTray {
    pub fn attach(&self, handle: &AppHandle, host: Arc<TauriHost>) -> tauri::Result<()> {
        let _ = self.handle.set(handle.clone());
        let (bytes, template) = icon_for(false);
        let icon = Image::from_bytes(bytes)?;
        let clicks = host.clone();
        TrayIconBuilder::with_id(TRAY_ID)
            .icon(icon)
            .icon_as_template(template)
            .show_menu_on_left_click(cfg!(target_os = "macos"))
            .tooltip("Muesli")
            .on_menu_event(move |_, event| host.tray_clicked(event.id().as_ref()))
            .on_tray_icon_event(move |_, event| {
                if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = event {
                    if !cfg!(target_os = "macos") {
                        clicks.open_muesli();
                    }
                }
            })
            .build(handle)?;
        Ok(())
    }
}

impl Tray for TauriTray {
    fn show(&self, wanted: &TrayMenu) {
        let Some(handle) = self.handle.get() else { return };
        {
            // Rebuilding closes a menu the user has open, so only when it really differs.
            let mut shown = self.shown.lock().unwrap();
            if shown.as_ref() == Some(wanted) {
                return;
            }
            *shown = Some(wanted.clone());
        }
        let (handle2, wanted) = (handle.clone(), wanted.clone());
        let _ = handle.run_on_main_thread(move || {
            let Some(tray) = handle2.tray_by_id(TRAY_ID) else { return };
            if let Ok(menu) = build_menu(&handle2, &wanted.items, wanted.transcribing_since) {
                let _ = tray.set_menu(Some(menu));
            }
            let _ = tray.set_tooltip(Some(&wanted.tooltip));
            let (image, template) = icon_for(wanted.transcribing);
            if let Ok(icon) = Image::from_bytes(image) {
                let _ = tray.set_icon(Some(icon));
                let _ = tray.set_icon_as_template(template);
            }
        });
    }
}

/// The SF Symbol beside a menu item (macOS), by the engine's item id.
#[cfg(target_os = "macos")]
fn symbol(id: &str) -> Option<&'static str> {
    Some(match id {
        "start" => "record.circle",
        "stop" => "stop.circle",
        "dismiss" => "xmark.circle",
        "open" => "macwindow",
        "transcribe_file" => "waveform.badge.plus",
        "settings" => "gearshape",
        "quit" => "power",
        id if id.starts_with("recent:") => "doc.text",
        _ => return None,
    })
}

/// macOS shows an item's detail as its second line (see `menu_icons`); elsewhere it follows the
/// label.
fn build_menu(
    handle: &AppHandle,
    items: &[TrayItem],
    #[allow(unused_variables)] since: Option<chrono::DateTime<chrono::Utc>>,
) -> tauri::Result<Menu<tauri::Wry>> {
    #[cfg(target_os = "macos")]
    muesli_platform_macos::menu_icons::set(
        items
            .iter()
            .filter_map(|i| match i {
                TrayItem::Action { id, label, detail, .. } => Some((
                    label.clone(),
                    muesli_platform_macos::menu_icons::Extras {
                        symbol: symbol(id),
                        subtitle: detail.clone(),
                        since: if id == "stop" { since } else { None },
                    },
                )),
                _ => None,
            })
            .collect(),
    );
    let menu = Menu::new(handle)?;
    for (n, entry) in items.iter().enumerate() {
        match entry {
            TrayItem::Separator => menu.append(&PredefinedMenuItem::separator(handle)?)?,
            TrayItem::Label { label } => menu.append(&MenuItem::with_id(handle, format!("label:{n}"), label, false, None::<&str>)?)?,
            TrayItem::Action { id, label, enabled, detail } => {
                let label = match detail {
                    Some(detail) if !cfg!(target_os = "macos") => format!("{label}  ·  {detail}"),
                    _ => label.clone(),
                };
                let shortcut = match id.as_str() {
                    "settings" => Some("CmdOrCtrl+,"),
                    "quit" => Some("CmdOrCtrl+Q"),
                    _ => None,
                };
                menu.append(&MenuItem::with_id(handle, id, &label, *enabled, shortcut)?)?
            }
        }
    }
    Ok(menu)
}
