//! The engine's `Host`: windows, dialogs, opening files, and where the bundled helper and Claude
//! Desktop extension are. The app never opens a socket: assistants start the bundled `muesli-mcp`
//! themselves and talk to it over stdio.

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use muesli_engine::{Engine, Host, Intent, Window};
use tauri::{AppHandle, Emitter};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

/// Tauri's side of the engine. The handle and the engine arrive once Tauri runs.
#[derive(Default)]
pub struct TauriHost {
    pub handle: OnceLock<AppHandle>,
    pub engine: OnceLock<std::sync::Weak<Engine>>,
}

impl TauriHost {
    fn handle(&self) -> Option<&AppHandle> {
        self.handle.get()
    }

    fn send(&self, intent: Intent) {
        if let Some(engine) = self.engine.get().and_then(|e| e.upgrade()) {
            engine.intent(intent);
        }
    }

    pub fn attach(&self, handle: &AppHandle, engine: &Arc<Engine>) {
        let _ = self.handle.set(handle.clone());
        let _ = self.engine.set(Arc::downgrade(engine));
    }
}

/// The `muesli-mcp` binary next to the app.
fn helper_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let name = if cfg!(windows) { "muesli-mcp.exe" } else { "muesli-mcp" };
    let candidate = exe.parent()?.join(name);
    candidate.exists().then_some(candidate)
}

impl Host for TauriHost {
    fn show(&self, window: Window) {
        if let Some(handle) = self.handle() {
            crate::windows::show(handle, window);
        }
    }

    fn close(&self, window: Window) {
        if let Some(handle) = self.handle() {
            crate::windows::close(handle, window);
        }
    }

    fn changed(&self) {
        if let Some(handle) = self.handle() {
            let _ = handle.emit("state-changed", ());
        }
    }

    fn pick_audio_files(&self) {
        let Some(handle) = self.handle().cloned() else { return };
        let extensions: Vec<&str> = muesli_core::import::AUDIO_EXTENSIONS.to_vec();
        let engine = self.engine.get().cloned();
        handle.dialog().file().set_title("Choose audio files to transcribe").add_filter("Audio", &extensions).pick_files(move |files| {
            let paths: Vec<PathBuf> = files.into_iter().flatten().filter_map(|f| f.into_path().ok()).collect();
            if let (false, Some(engine)) = (paths.is_empty(), engine.and_then(|e| e.upgrade())) {
                engine.intent(Intent::TranscribeFiles { paths });
            }
        });
    }

    fn pick_transcripts_folder(&self) {
        let Some(handle) = self.handle().cloned() else { return };
        let engine = self.engine.get().cloned();
        handle.dialog().file().set_title("Choose where Muesli saves transcripts").pick_folder(move |folder| {
            if let (Some(path), Some(engine)) = (folder.and_then(|f| f.into_path().ok()), engine.and_then(|e| e.upgrade())) {
                engine.intent(Intent::SetTranscriptsFolder { path });
            }
        });
    }

    fn reveal(&self, path: &Path) {
        if let Some(handle) = self.handle() {
            let _ = if path.is_dir() {
                handle.opener().open_path(path.to_string_lossy(), None::<&str>)
            } else {
                handle.opener().reveal_item_in_dir(path)
            };
        }
    }

    fn open(&self, path: &Path) {
        if let Some(handle) = self.handle() {
            let _ = handle.opener().open_path(path.to_string_lossy(), None::<&str>);
        }
    }

    fn open_url(&self, url: &str) {
        if let Some(handle) = self.handle() {
            let _ = handle.opener().open_url(url, None::<&str>);
        }
    }

    fn quit(&self) {
        if let Some(handle) = self.handle() {
            handle.exit(0);
        }
    }

    fn helper_path(&self) -> Option<PathBuf> {
        helper_path()
    }

    /// The Claude Desktop extension bundled with the app, if any.
    fn extension_path(&self) -> Option<PathBuf> {
        let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
        [dir.join("Muesli.mcpb"), dir.join("../Resources/Muesli.mcpb"), dir.join("resources/Muesli.mcpb")].into_iter().find(|p| p.exists())
    }

    /// On Windows the installer's folder is checked, and Claude's data folder as a fallback,
    /// because the install location differs between builds.
    fn claude_installed(&self) -> bool {
        if cfg!(target_os = "macos") {
            let home_apps = dirs::home_dir().map(|h| h.join("Applications/Claude.app"));
            Path::new("/Applications/Claude.app").exists() || home_apps.is_some_and(|p| p.exists())
        } else {
            let exe = dirs::data_local_dir().map(|d| d.join("AnthropicClaude").join("claude.exe"));
            let data = dirs::data_dir().map(|d| d.join("Claude"));
            exe.is_some_and(|p| p.exists()) || data.is_some_and(|p| p.exists())
        }
    }
}

impl TauriHost {
    /// A click in the tray menu.
    pub fn tray_clicked(&self, id: &str) {
        self.send(Intent::Tray { id: id.to_string() });
    }

    /// A left click on the tray icon (Windows).
    pub fn open_muesli(&self) {
        self.send(Intent::OpenMuesli);
    }
}
