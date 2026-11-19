//! The tray icon (menu bar on macOS, notification area on Windows) and its menu.

use std::sync::{Mutex, OnceLock};

use muesli_core::ports::{Tray, TrayItem, TrayMenu};

use muesli_core::store::{shorten_title, Store};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};

use crate::ctx::Ctx;
use crate::state::{App, Status};
use crate::windows;

const TRAY_ID: &str = "main";

type Entry = TrayItem;

fn item(id: &str, text: impl Into<String>, enabled: bool) -> Entry {
    TrayItem::Action { id: id.into(), label: text.into(), enabled }
}

fn entries(app: &App) -> Vec<Entry> {
    let label = |text: String| TrayItem::Label { label: text };
    let mut out = Vec::new();
    match app.status() {
        Status::Idle => {
            if let Some((text, _)) = app.model.activity() {
                out.push(label(format!("{text}…"))); // progress is shown in the windows
            } else if !app.model.is_ready() {
                out.push(label("Transcribing needs a Whisper model".into()));
                out.push(item("settings", "Open Model Settings…", true));
            }
            out.push(item("start", "Start Transcribing", app.model.is_ready()));
        }
        Status::Recording { title } => {
            out.push(label(format!("Recording · {}", shorten_title(&title, 40))));
            out.push(item("stop", "Stop and Save", true));
        }
        Status::Saving => out.push(label("Saving Transcript…".into())),
        Status::Error { message } => {
            out.push(label(shorten_title(&message, 90)));
            out.push(item("dismiss", "Dismiss", true));
        }
    }
    out.push(TrayItem::Separator);
    out.push(item("transcribe_file", "Transcribe Audio File…", app.model.is_ready()));
    if let Some(running) = app.files.running() {
        let progress = match running.state {
            crate::files::FileState::Running { progress: Some(p) } => format!(" · {}%", (p * 100.0) as u32),
            _ => "…".into(),
        };
        out.push(label(format!("Transcribing {}{progress}", shorten_title(&running.name, 36))));
        if app.files.waiting() > 0 {
            out.push(label(format!("{} more waiting", app.files.waiting())));
        }
    } else if app.files.waiting() > 0 {
        out.push(label(format!("{} audio files waiting", app.files.waiting())));
    }
    if let Some(problem) = app.files.error.lock().unwrap().clone() {
        out.push(label(shorten_title(&problem, 90)));
        out.push(item("dismiss_file_error", "Dismiss", true));
    }
    let limit = app.prefs().effective_recent_limit() as usize;
    if limit > 0 {
        out.push(TrayItem::Separator);
        out.push(label("Recent Transcripts".into()));
        let recent = Store::recent_transcripts(&app.paths().transcripts, limit);
        if recent.is_empty() {
            out.push(label("No transcripts yet".into()));
        }
        for r in recent {
            out.push(item(&format!("recent:{}", r.path.to_string_lossy()), shorten_title(&r.title, 44), true));
        }
    }
    out.push(TrayItem::Separator);
    out.push(item("folder", "Open Transcripts Folder", true));
    out.push(TrayItem::Separator);
    out.push(item("settings", "Settings…", true));
    out.push(item("quit", "Quit Muesli", true));
    out
}

/// The menu Muesli wants now.
pub fn menu_for(app: &App) -> TrayMenu {
    TrayMenu { items: entries(app), transcribing: app.is_recording(), tooltip: "Muesli".into() }
}

/// Hands the menu to the tray port.
pub fn refresh(ctx: &Ctx) {
    ctx.app.platform.tray.show(&menu_for(&ctx.app));
}

/// The `Tray` adapter: Tauri's tray icon, the same on macOS and Windows. It needs the app handle,
/// which only exists once Tauri runs, so the menu waits until `attach`.
#[derive(Default)]
pub struct TauriTray {
    handle: OnceLock<AppHandle>,
    shown: Mutex<Option<TrayMenu>>,
}

impl TauriTray {
    pub fn attach(&self, handle: &AppHandle) -> tauri::Result<()> {
        let _ = self.handle.set(handle.clone());
        let icon = Image::from_bytes(include_bytes!("../icons/tray.png"))?;
        TrayIconBuilder::with_id(TRAY_ID)
            .icon(icon)
            .icon_as_template(true)
            .show_menu_on_left_click(true)
            .tooltip("Muesli")
            .on_menu_event(|handle, event| on_menu(handle, event.id().as_ref()))
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
    for entry in items {
        match entry {
            TrayItem::Separator => menu.append(&PredefinedMenuItem::separator(handle)?)?,
            TrayItem::Label { label } => menu.append(&MenuItem::with_id(handle, "none", label, false, None::<&str>)?)?,
            TrayItem::Action { id, label, enabled } => menu.append(&MenuItem::with_id(handle, id, label, *enabled, None::<&str>)?)?,
        }
    }
    Ok(menu)
}

fn on_menu(handle: &AppHandle, id: &str) {
    let Some(app) = handle.try_state::<std::sync::Arc<crate::state::App>>() else { return };
    let ctx = Ctx { app: app.inner().clone(), handle: handle.clone() };
    match id {
        "start" => crate::session::start(&ctx, muesli_core::DetectedMeeting::new("Meeting", "manual")),
        "stop" => crate::session::stop(&ctx),
        "dismiss" => ctx.set_status(Status::Idle),
        "transcribe_file" => crate::commands::pick_and_queue(handle, ctx.app.clone()),
        "dismiss_file_error" => crate::files::dismiss_error(&ctx),
        "settings" => windows::show(handle, "settings"),
        "folder" => open_path(handle, &ctx.app.paths().transcripts_dir().to_string_lossy()),
        "quit" => handle.exit(0),
        other => {
            if let Some(path) = other.strip_prefix("recent:") {
                open_path(handle, path);
            }
        }
    }
}

pub fn open_path(handle: &AppHandle, path: &str) {
    use tauri_plugin_opener::OpenerExt;
    let _ = handle.opener().open_path(path, None::<&str>);
}
