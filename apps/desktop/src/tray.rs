//! The `Tray` adapter: Tauri's tray icon, the same on macOS and Windows. The engine builds the
//! menu; this only draws it and reports clicks.

use std::sync::{Arc, Mutex, OnceLock};

use muesli_core::ports::{Tray, TrayItem, TrayMenu};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::AppHandle;

use crate::host::TauriHost;

const TRAY_ID: &str = "main";

/// The handle only exists once Tauri runs, so drawing waits until `attach`.
#[derive(Default)]
pub struct TauriTray {
    handle: OnceLock<AppHandle>,
    shown: Mutex<Option<TrayMenu>>,
}

impl TauriTray {
    pub fn attach(&self, handle: &AppHandle, host: Arc<TauriHost>) -> tauri::Result<()> {
        let _ = self.handle.set(handle.clone());
        let icon = Image::from_bytes(include_bytes!("../icons/tray.png"))?;
        TrayIconBuilder::with_id(TRAY_ID)
            .icon(icon)
            .icon_as_template(true)
            .show_menu_on_left_click(true)
            .tooltip("Muesli")
            .on_menu_event(move |_, event| host.tray_clicked(event.id().as_ref()))
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
            if let (Some(tray), Ok(menu)) = (handle2.tray_by_id(TRAY_ID), build_menu(&handle2, &wanted.items)) {
                let _ = tray.set_menu(Some(menu));
                let _ = tray.set_tooltip(Some(&wanted.tooltip));
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
