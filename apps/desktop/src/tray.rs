//! The `Tray` adapter: Tauri's tray icon on macOS and Windows. The engine builds the menu; this
//! only draws it and reports clicks.
//!
//! macOS: a click shows the menu. Windows: a left click opens Muesli's window and a right click
//! shows the menu, as tray apps do there. While transcribing the icon carries a red dot; when an
//! update is ready, a Pumpkin seed one. Such an icon is a coloured image, which the system does
//! not tint, so there are two glyphs: milk on a dark menu bar or taskbar, roast on a light one. The
//! shell looks at the bar itself (macOS: the status item's appearance, Windows: the registry).

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
const UPDATE: &[u8] = include_bytes!("../icons/tray-update.png");
/// Milk-coloured versions for a dark menu bar (macOS) or taskbar (Windows 11's default).
#[cfg(any(windows, target_os = "macos"))]
const IDLE_LIGHT: &[u8] = include_bytes!("../icons/tray-light.png");
#[cfg(any(windows, target_os = "macos"))]
const TRANSCRIBING_LIGHT: &[u8] = include_bytes!("../icons/tray-light-transcribing.png");
#[cfg(any(windows, target_os = "macos"))]
const UPDATE_LIGHT: &[u8] = include_bytes!("../icons/tray-light-update.png");

/// The icon for the current state, and whether macOS may tint it (a template image). The red
/// transcribing dot wins over the update dot. `dark_bar`: the menu bar or taskbar is dark, so a
/// coloured icon needs the milk glyph. The idle icon on macOS is a template, which follows the bar
/// by itself.
fn icon_for(transcribing: bool, update: bool, dark_bar: bool) -> (&'static [u8], bool) {
    #[cfg(any(windows, target_os = "macos"))]
    if dark_bar && (cfg!(windows) || transcribing || update) {
        let icon = if transcribing {
            TRANSCRIBING_LIGHT
        } else if update {
            UPDATE_LIGHT
        } else {
            IDLE_LIGHT
        };
        return (icon, false);
    }
    let _ = dark_bar;
    // A coloured dot cannot be a template image (macOS would tint it black).
    if transcribing {
        (TRANSCRIBING, false)
    } else if update {
        (UPDATE, false)
    } else {
        (IDLE, true)
    }
}

/// Whether the menu bar (macOS) or taskbar (Windows) is dark right now. On macOS this must run on
/// the main thread once the tray icon exists; while the menu is open it keeps the last answer, as
/// redrawing the icon then would close the menu.
fn bar_is_dark(handle: &AppHandle) -> bool {
    #[cfg(windows)]
    {
        let _ = handle;
        !muesli_platform_windows::theme::taskbar_is_light()
    }
    #[cfg(target_os = "macos")]
    {
        handle
            .tray_by_id(TRAY_ID)
            .and_then(|tray| {
                tray.with_inner_tray_icon(|inner| inner.ns_status_item().and_then(|item| muesli_platform_macos::menubar::is_dark(&item)))
                    .ok()
            })
            .flatten()
            .unwrap_or_else(|| ICON.lock().unwrap().dark_bar)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = handle;
        false
    }
}

/// What the icon shows now, so a change of the bar's colour can redraw it (macOS looks).
#[derive(Clone, Copy, Default)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
struct IconState {
    transcribing: bool,
    update: bool,
    dark_bar: bool,
}

static ICON: Mutex<IconState> = Mutex::new(IconState { transcribing: false, update: false, dark_bar: false });

/// Draws the icon for this state. On the main thread.
fn draw_icon(handle: &AppHandle, transcribing: bool, update: bool) {
    let Some(tray) = handle.tray_by_id(TRAY_ID) else { return };
    let dark_bar = bar_is_dark(handle);
    *ICON.lock().unwrap() = IconState { transcribing, update, dark_bar };
    let (image, template) = icon_for(transcribing, update, dark_bar);
    if let Ok(icon) = Image::from_bytes(image) {
        let _ = tray.set_icon(Some(icon));
        let _ = tray.set_icon_as_template(template);
    }
}

/// macOS: the menu bar turns dark or light with the system's mode or the wallpaper behind it. A
/// coloured icon does not follow, so look every couple of seconds and redraw when it changed.
#[cfg(target_os = "macos")]
fn watch_menu_bar(handle: AppHandle) {
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(2));
        let drawn = *ICON.lock().unwrap();
        if !(drawn.transcribing || drawn.update) {
            continue; // the template icon follows the bar by itself
        }
        let h = handle.clone();
        let _ = handle.run_on_main_thread(move || {
            if bar_is_dark(&h) != drawn.dark_bar {
                draw_icon(&h, drawn.transcribing, drawn.update);
            }
        });
    });
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
        let (bytes, template) = icon_for(false, false, bar_is_dark(handle));
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
        #[cfg(target_os = "macos")]
        watch_menu_bar(handle.clone());
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
            draw_icon(&handle2, wanted.transcribing, wanted.update_ready);
        });
    }
}

/// The SF Symbol beside a menu item (macOS), by the engine's item id.
#[cfg(target_os = "macos")]
fn symbol(id: &str) -> Option<&'static str> {
    Some(match id {
        "update" => "arrow.down.circle",
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
            .map(|i| match i {
                TrayItem::Separator => (String::new(), None),
                TrayItem::Label { label } => (label.clone(), None),
                TrayItem::Action { id, label, detail, .. } => (
                    label.clone(),
                    Some(muesli_platform_macos::menu_icons::Extras {
                        symbol: symbol(id),
                        subtitle: detail.clone(),
                        since: if id == "stop" { since } else { None },
                    }),
                ),
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
