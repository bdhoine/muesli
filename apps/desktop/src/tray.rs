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

/// The handle only exists once Tauri runs, so drawing waits until `attach`.
#[derive(Default)]
pub struct TauriTray {
    handle: OnceLock<AppHandle>,
    shown: Mutex<Option<TrayMenu>>,
}

impl TauriTray {
    pub fn attach(&self, handle: &AppHandle, host: Arc<TauriHost>) -> tauri::Result<()> {
        let _ = self.handle.set(handle.clone());
        let icon = Image::from_bytes(IDLE)?;
        let clicks = host.clone();
        TrayIconBuilder::with_id(TRAY_ID)
            .icon(icon)
            .icon_as_template(true)
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
            if let Ok(menu) = build_menu(&handle2, &wanted.items) {
                let _ = tray.set_menu(Some(menu));
            }
            let _ = tray.set_tooltip(Some(&wanted.tooltip));
            // The red dot cannot be a template image (macOS would tint it black).
            let image = if wanted.transcribing { TRANSCRIBING } else { IDLE };
            if let Ok(icon) = Image::from_bytes(image) {
                let _ = tray.set_icon(Some(icon));
                let _ = tray.set_icon_as_template(!wanted.transcribing);
            }
        });
    }
}

fn build_menu(handle: &AppHandle, items: &[TrayItem]) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(handle)?;
    for (n, entry) in items.iter().enumerate() {
        match entry {
            TrayItem::Separator => menu.append(&PredefinedMenuItem::separator(handle)?)?,
            TrayItem::Label { label } => menu.append(&MenuItem::with_id(handle, format!("label:{n}"), label, false, None::<&str>)?)?,
            TrayItem::Action { id, label, enabled } => menu.append(&MenuItem::with_id(handle, id, label, *enabled, None::<&str>)?)?,
        }
    }
    Ok(menu)
}
