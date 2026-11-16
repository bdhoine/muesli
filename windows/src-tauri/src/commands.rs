//! Everything the web UI can ask for.

use std::sync::Arc;

use muesli_core::assistants::{AssistantClient, ExtensionState};
use muesli_core::store::Store;
use muesli_core::{languages, DetectedMeeting, Preferences, WhisperModel, VERSION};
use muesli_whisper::Phase;
use serde::Serialize;
use std::path::PathBuf;
use tauri::{AppHandle, State};

use crate::bridge;
use crate::ctx::Ctx;
use crate::state::{App, Status};

type Shared<'a> = State<'a, Arc<App>>;

fn ctx(handle: &AppHandle, app: &Shared) -> Ctx {
    Ctx { app: app.inner().clone(), handle: handle.clone() }
}

fn platform() -> &'static str {
    if cfg!(target_os = "macos") { "macos" } else if cfg!(windows) { "windows" } else { "linux" }
}

// What the UI gets from `get_state`. One struct per section, so the contract is in one place.

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UiState {
    version: &'static str,
    platform: &'static str,
    status: Status,
    prefs: Preferences,
    effective_languages: Vec<String>,
    system_languages: Vec<String>,
    supported_languages: Vec<Language>,
    model: ModelPhase,
    models: Vec<ModelInfo>,
    recent: Vec<Recent>,
    transcripts_folder: PathBuf,
    /// True when the user chose the folder (so it can be reset).
    transcripts_folder_is_custom: bool,
    /// True when TRANSCRIPT_DIR fixes the folder and it cannot be changed here.
    transcripts_folder_locked: bool,
    assistants: Assistants,
}

#[derive(Serialize)]
struct Language {
    code: &'static str,
    name: &'static str,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum ModelPhase {
    Missing { model: WhisperModel },
    Downloading { model: WhisperModel, received: u64, total: u64 },
    Ready { path: PathBuf },
    Failed { message: String },
}

impl From<Phase> for ModelPhase {
    fn from(phase: Phase) -> Self {
        match phase {
            Phase::Missing(model) => Self::Missing { model },
            Phase::Downloading { model, received, total } => Self::Downloading { model, received, total },
            Phase::Ready(path) => Self::Ready { path },
            Phase::Failed(message) => Self::Failed { message },
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelInfo {
    id: WhisperModel,
    title: &'static str,
    summary: &'static str,
    bytes: u64,
    installed: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Recent {
    title: String,
    path: PathBuf,
    started_at: String,
    minutes: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Assistants {
    helper: Option<PathBuf>,
    clients: Vec<Client>,
    extension: Extension,
    config_status: String,
}

#[derive(Serialize)]
struct Client {
    name: &'static str,
    instructions: String,
    terminal: bool,
    snippet: Option<String>,
}

#[derive(Serialize)]
struct Extension {
    state: &'static str,
    version: Option<String>,
    bundled: bool,
}

fn assistants(app: &App) -> Assistants {
    let helper = bridge::helper_path();
    let (state, version) = match bridge::extension_state() {
        ExtensionState::ClaudeMissing => ("claudeMissing", None),
        ExtensionState::NotInstalled => ("notInstalled", None),
        ExtensionState::Installed { version } => ("installed", Some(version)),
        ExtensionState::Outdated { installed, .. } => ("outdated", Some(installed)),
    };
    Assistants {
        clients: AssistantClient::ALL
            .iter()
            .map(|c| Client {
                name: c.name(),
                instructions: c.instructions(),
                terminal: c.is_terminal_command(),
                snippet: helper.as_ref().map(|h| c.snippet(&h.to_string_lossy())),
            })
            .collect(),
        helper,
        extension: Extension { state, version, bundled: bridge::extension_path().is_some() },
        config_status: app.claude_status.lock().unwrap().clone(),
    }
}

#[tauri::command]
pub fn get_state(app: Shared) -> UiState {
    let prefs = app.prefs();
    let defaults = languages::system_defaults();
    UiState {
        version: VERSION,
        platform: platform(),
        status: app.status(),
        effective_languages: prefs.effective_languages(&defaults),
        system_languages: defaults,
        supported_languages: languages::SUPPORTED.iter().map(|code| Language { code, name: languages::english_name(code) }).collect(),
        model: app.model.phase().into(),
        models: WhisperModel::ALL
            .iter()
            .map(|m| ModelInfo {
                id: *m,
                title: m.title(),
                summary: m.summary(),
                bytes: m.approximate_bytes(),
                installed: app.model.is_installed(*m),
            })
            .collect(),
        recent: Store::recent_transcripts(&app.paths().transcripts, prefs.effective_recent_limit() as usize)
            .into_iter()
            .map(|r| Recent { minutes: r.duration_minutes(), started_at: r.started_at.to_rfc3339(), title: r.title, path: r.path })
            .collect(),
        transcripts_folder: app.paths().transcripts,
        transcripts_folder_is_custom: prefs.transcripts_folder.is_some(),
        transcripts_folder_locked: muesli_core::Paths::transcripts_set_by_environment(),
        assistants: assistants(&app),
        prefs,
    }
}

#[tauri::command]
pub fn update_prefs(handle: AppHandle, app: Shared, prefs: Preferences) {
    let before = app.prefs();
    let _ = prefs.save(&app.paths());
    app.model.apply(prefs.clone());
    *app.prefs.lock().unwrap() = prefs.clone();
    if prefs.mcp_enabled != before.mcp_enabled || prefs.claude_desktop_config_entry != before.claude_desktop_config_entry {
        bridge::apply(&app, &prefs);
    }
    ctx(&handle, &app).changed();
}

#[tauri::command]
pub fn download_model(handle: AppHandle, app: Shared, model: WhisperModel) {
    let c = ctx(&handle, &app);
    app.model.download(model, move || c.changed());
    ctx(&handle, &app).changed();
}

#[tauri::command]
pub fn cancel_download(handle: AppHandle, app: Shared) {
    app.model.cancel();
    ctx(&handle, &app).changed();
}

#[tauri::command]
pub fn delete_model(handle: AppHandle, app: Shared, model: WhisperModel) {
    app.model.delete(model);
    ctx(&handle, &app).changed();
}

/// Asks for a custom model file; returns the chosen path.
#[tauri::command]
pub async fn choose_model_file(handle: AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let file = handle.dialog().file().set_title("Choose a Whisper model").add_filter("Whisper model", &["bin"]).blocking_pick_file()?;
    file.into_path().ok().map(|p| p.to_string_lossy().into_owned())
}

/// Asks where transcripts should go; returns the chosen folder.
#[tauri::command]
pub async fn choose_transcripts_folder(handle: AppHandle) -> Option<String> {
    use tauri_plugin_dialog::DialogExt;
    let folder = handle.dialog().file().set_title("Choose where Muesli saves transcripts").blocking_pick_folder()?;
    folder.into_path().ok().map(|p| p.to_string_lossy().into_owned())
}

#[tauri::command]
pub fn start_recording(handle: AppHandle, app: Shared) {
    crate::session::start(&ctx(&handle, &app), DetectedMeeting::new("Meeting", "manual"));
}

#[tauri::command]
pub fn stop_recording(handle: AppHandle, app: Shared) {
    crate::session::stop(&ctx(&handle, &app));
}

#[tauri::command]
pub fn dismiss(handle: AppHandle, app: Shared) {
    ctx(&handle, &app).set_status(Status::Idle);
}

#[tauri::command]
pub fn get_prompt(app: Shared) -> Option<crate::state::Prompt> {
    app.prompt.lock().unwrap().clone()
}

/// The prompt window's answer: accepted means "start" or "stop and save", depending on its kind.
#[tauri::command]
pub fn prompt_response(handle: AppHandle, app: Shared, accept: bool) {
    crate::prompt::answer(&ctx(&handle, &app), None, accept);
}

#[tauri::command]
pub fn finish_onboarding(handle: AppHandle, app: Shared) {
    let mut prefs = app.prefs();
    prefs.onboarding_done = true;
    let _ = prefs.save(&app.paths());
    *app.prefs.lock().unwrap() = prefs;
    crate::windows::close(&handle, "onboarding");
    ctx(&handle, &app).changed();
}

/// Opens the system's microphone or system-audio prompt at the moment the user asks for it.
#[tauri::command]
pub async fn request_permission(kind: String) -> Result<(), String> {
    let result = match kind.as_str() {
        "microphone" => muesli_audio::probe_microphone(),
        _ => muesli_audio::probe_system_audio(),
    };
    result.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn open_privacy_settings(handle: AppHandle, kind: String) {
    use tauri_plugin_opener::OpenerExt;
    let url = match (cfg!(windows), kind.as_str()) {
        (true, _) => "ms-settings:privacy-microphone",
        (false, "microphone") => "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
        (false, _) => "x-apple.systempreferences:com.apple.preference.security?Privacy_AudioCapture",
    };
    let _ = handle.opener().open_url(url, None::<&str>);
}

/// Opens a transcript or the transcripts folder. Anything outside that folder is refused, so a
/// page can never make the app open arbitrary files.
#[tauri::command]
pub fn open_path(handle: AppHandle, app: Shared, path: String) -> Result<(), String> {
    let folder = app.paths().transcripts_dir().canonicalize().map_err(|e| e.to_string())?;
    let target = std::path::Path::new(&path).canonicalize().map_err(|e| e.to_string())?;
    if !target.starts_with(&folder) {
        return Err("Only files in the transcripts folder can be opened.".into());
    }
    crate::tray::open_path(&handle, &target.to_string_lossy());
    Ok(())
}

#[tauri::command]
pub fn install_extension(handle: AppHandle) {
    if let Some(path) = bridge::extension_path() {
        crate::tray::open_path(&handle, &path.to_string_lossy());
    }
}

#[tauri::command]
pub fn show_window(handle: AppHandle, label: String) {
    crate::windows::show(&handle, &label);
}

#[tauri::command]
pub fn close_window(handle: AppHandle, label: String) {
    crate::windows::close(&handle, &label);
}
