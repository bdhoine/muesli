//! Everything the web UI can ask for.

use std::sync::Arc;

use muesli_core::assistants::{AssistantClient, ExtensionState};
use muesli_core::store::Store;
use muesli_core::whisper_model::DEFAULT_MODEL;
use muesli_core::{languages, DetectedMeeting, Preferences, VERSION};
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
    /// The one speech model every install uses (Settings shows it; another needs an own file).
    model_info: ModelInfo,
    download: Option<Download>,
    download_error: Option<String>,
    models_folder: PathBuf,
    /// Model files from earlier versions that can be removed, and the space they take.
    legacy_models: usize,
    legacy_bytes: u64,
    system: SystemStatus,
    recent: Vec<Recent>,
    transcripts_folder: PathBuf,
    /// True when the user chose the folder (so it can be reset).
    transcripts_folder_is_custom: bool,
    /// True when TRANSCRIPT_DIR fixes the folder and it cannot be changed here.
    transcripts_folder_locked: bool,
    assistants: Assistants,
}

#[derive(Serialize)]
struct Download {
    received: u64,
    total: u64,
}

/// How Muesli stands with the operating system, for the Permissions page.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemStatus {
    /// "allowed", "denied" or "unknown"
    microphone: &'static str,
    notifications: &'static str,
    /// False where the system asks nothing for recording what the computer plays (Windows).
    system_audio_needs_permission: bool,
    start_at_login: bool,
}

#[derive(Serialize)]
struct Language {
    code: &'static str,
    name: &'static str,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum ModelPhase {
    Missing,
    Downloading { received: u64, total: u64 },
    Ready { path: PathBuf },
    Failed { message: String },
}

impl From<Phase> for ModelPhase {
    fn from(phase: Phase) -> Self {
        match phase {
            Phase::Missing => Self::Missing,
            Phase::Downloading { received, total } => Self::Downloading { received, total },
            Phase::Ready(path) => Self::Ready { path },
            Phase::Failed(message) => Self::Failed { message },
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ModelInfo {
    title: &'static str,
    summary: &'static str,
    bytes: u64,
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
pub fn get_state(handle: AppHandle, app: Shared) -> UiState {
    use muesli_system::status;
    use tauri_plugin_autostart::ManagerExt;
    let prefs = app.prefs();
    let paths = app.paths();
    let (legacy, legacy_bytes) = app.model.legacy_models();
    let defaults = languages::system_defaults();
    UiState {
        version: VERSION,
        platform: platform(),
        status: app.status(),
        effective_languages: prefs.effective_languages(&defaults),
        system_languages: defaults,
        supported_languages: languages::SUPPORTED.iter().map(|code| Language { code, name: languages::english_name(code) }).collect(),
        model: app.model.phase().into(),
        model_info: ModelInfo { title: DEFAULT_MODEL.title, summary: DEFAULT_MODEL.summary, bytes: DEFAULT_MODEL.approximate_bytes },
        download: app.model.download_progress().map(|(received, total)| Download { received, total }),
        download_error: app.model.last_error(),
        models_folder: paths.models.clone(),
        legacy_models: legacy.len(),
        legacy_bytes,
        system: SystemStatus {
            microphone: status::microphone().as_str(),
            notifications: status::notifications(crate::prompt::APP_ID).as_str(),
            system_audio_needs_permission: cfg!(target_os = "macos"),
            start_at_login: handle.autolaunch().is_enabled().unwrap_or(false),
        },
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
    if !app.model.is_ready() && !app.model.is_downloading() && app.model.last_error().is_none() {
        let c = ctx(&handle, &app);
        app.model.ensure_ready(move || c.changed()); // e.g. back from an own model file to the download
    }
    if prefs.mcp_enabled != before.mcp_enabled || prefs.claude_desktop_config_entry != before.claude_desktop_config_entry {
        bridge::apply(&app, &prefs);
    }
    ctx(&handle, &app).changed();
}

#[tauri::command]
pub fn download_model(handle: AppHandle, app: Shared) {
    let c = ctx(&handle, &app);
    app.model.ensure_ready(move || c.changed());
    ctx(&handle, &app).changed();
}

/// Deletes the model files earlier versions let the user pick (Small, Medium, Large).
#[tauri::command]
pub fn remove_legacy_models(handle: AppHandle, app: Shared) {
    app.model.remove_legacy_models();
    ctx(&handle, &app).changed();
}

/// Opens the folder that holds the downloaded models.
#[tauri::command]
pub fn open_models_folder(handle: AppHandle, app: Shared) {
    let folder = app.paths().models;
    let _ = std::fs::create_dir_all(&folder);
    crate::tray::open_path(&handle, &folder.to_string_lossy());
}

/// Switches between the system's languages and the user's own list, keeping that list.
#[tauri::command]
pub fn use_system_languages(handle: AppHandle, app: Shared, on: bool) {
    let mut prefs = app.prefs();
    prefs.set_uses_system_languages(on, &languages::system_defaults());
    update_prefs(handle, app, prefs);
}

#[tauri::command]
pub fn set_start_at_login(handle: AppHandle, app: Shared, on: bool) {
    use tauri_plugin_autostart::ManagerExt;
    let launcher = handle.autolaunch();
    let _ = if on { launcher.enable() } else { launcher.disable() };
    ctx(&handle, &app).changed();
}

#[tauri::command]
pub fn cancel_download(handle: AppHandle, app: Shared) {
    app.model.cancel();
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

/// Opens Muesli's repository in the browser.
#[tauri::command]
pub fn open_repository(handle: AppHandle) {
    use tauri_plugin_opener::OpenerExt;
    let _ = handle.opener().open_url(muesli_core::REPOSITORY, None::<&str>);
}

#[tauri::command]
pub fn open_privacy_settings(handle: AppHandle, kind: String) {
    use tauri_plugin_opener::OpenerExt;
    let url = match (cfg!(windows), kind.as_str()) {
        (true, "notifications") => "ms-settings:notifications",
        (true, _) => "ms-settings:privacy-microphone",
        (false, "microphone") => "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
        (false, "notifications") => "x-apple.systempreferences:com.apple.Notifications-Settings.extension",
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
