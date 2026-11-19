//! The tray icon (menu bar on macOS, notification area on Windows) and its menu.

use std::sync::Mutex;

use muesli_core::store::{shorten_title, Store};
use tauri::image::Image;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager};

use crate::ctx::Ctx;
use crate::state::{App, Status};
use crate::windows;

const TRAY_ID: &str = "main";

/// One line of the tray menu. The menu is described as data first, so it is only rebuilt (which
/// would close a menu the user has open) when it really differs.
#[derive(Debug, Clone, PartialEq)]
enum Entry {
    Item { id: String, text: String, enabled: bool },
    Separator,
}

fn item(id: &str, text: impl Into<String>, enabled: bool) -> Entry {
    Entry::Item { id: id.into(), text: text.into(), enabled }
}

static SHOWN: Mutex<Vec<Entry>> = Mutex::new(Vec::new());

fn entries(app: &App) -> Vec<Entry> {
    let label = |text: String| item("none", text, false);
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
    out.push(Entry::Separator);
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
        out.push(Entry::Separator);
        out.push(label("Recent Transcripts".into()));
        let recent = Store::recent_transcripts(&app.paths().transcripts, limit);
        if recent.is_empty() {
            out.push(label("No transcripts yet".into()));
        }
        for r in recent {
            out.push(item(&format!("recent:{}", r.path.to_string_lossy()), shorten_title(&r.title, 44), true));
        }
    }
    out.push(Entry::Separator);
    out.push(item("folder", "Open Transcripts Folder", true));
    out.push(Entry::Separator);
    out.push(item("settings", "Settings…", true));
    out.push(item("quit", "Quit Muesli", true));
    out
}

fn menu(handle: &AppHandle, entries: &[Entry]) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(handle)?;
    for entry in entries {
        match entry {
            Entry::Separator => menu.append(&PredefinedMenuItem::separator(handle)?)?,
            Entry::Item { id, text, enabled } => menu.append(&MenuItem::with_id(handle, id, text, *enabled, None::<&str>)?)?,
        }
    }
    Ok(menu)
}

pub fn build(ctx: &Ctx) -> tauri::Result<()> {
    let entries = entries(&ctx.app);
    let icon = Image::from_bytes(include_bytes!("../icons/tray.png"))?;
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon)
        .icon_as_template(true)
        .menu(&menu(&ctx.handle, &entries)?)
        .show_menu_on_left_click(true)
        .tooltip("Muesli")
        .on_menu_event(|handle, event| on_menu(handle, event.id().as_ref()))
        .build(&ctx.handle)?;
    *SHOWN.lock().unwrap() = entries;
    Ok(())
}

/// Rebuilds the menu if what it should show has changed.
pub fn refresh(ctx: &Ctx) {
    let wanted = entries(&ctx.app);
    {
        let mut shown = SHOWN.lock().unwrap();
        if *shown == wanted {
            return;
        }
        *shown = wanted.clone();
    }
    let handle = ctx.handle.clone();
    let _ = ctx.handle.run_on_main_thread(move || {
        if let (Some(tray), Ok(menu)) = (handle.tray_by_id(TRAY_ID), menu(&handle, &wanted)) {
            let _ = tray.set_menu(Some(menu));
        }
    });
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
