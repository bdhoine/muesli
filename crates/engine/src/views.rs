//! The view state of each window: everything a window shows, already decided. The web UI renders
//! these as they are; the words that differ per platform come from [`PlatformWords`].

use muesli_core::ports::{Access, Permission, PlatformWords};
use serde::Serialize;

use crate::ModelStatus;

/// One line of a transcript: `[mm:ss] **Speaker**: text`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Line {
    /// `mm:ss`, or `h:mm:ss` past the first hour.
    pub time: String,
    pub speaker: String,
    pub text: String,
}

/// A meeting in the main window's list.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingRow {
    pub id: String,
    pub title: String,
    /// ISO 8601; the UI writes it in the user's locale.
    pub started_at: String,
    pub duration_minutes: Option<i64>,
    /// "Calendar", "Teams call", "Call", "Started by hand", "Audio file".
    pub source: String,
    pub selected: bool,
    /// The transcript was cut short (recovered after a crash, or a failed import).
    pub partial: bool,
    pub failed: bool,
}

/// The meeting being transcribed now, at the top of the list.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveRow {
    pub id: String,
    pub title: String,
    pub started_at: String,
    pub selected: bool,
    /// "Transcribing" or "Saving…".
    pub status: String,
}

/// An audio file being transcribed, waiting, or failed.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRow {
    pub id: u64,
    pub name: String,
    /// "Transcribing", "Waiting", "Waiting for the speech model", or the error.
    pub status: String,
    /// 0..1 while transcribing, when the length is known.
    pub progress: Option<f64>,
    pub running: bool,
    pub failed: bool,
}

/// The right side of the main window.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Reader {
    pub id: String,
    pub title: String,
    pub started_at: String,
    pub duration_minutes: Option<i64>,
    pub source: String,
    pub attendees: Vec<String>,
    pub lines: Vec<Line>,
    /// True for the meeting being transcribed: read-only, text updates as it goes.
    pub live: bool,
    /// "Transcribing · live, read-only", or `None`.
    pub badge: Option<String>,
    /// Rename and Delete are offered for finished meetings only.
    pub editable: bool,
    /// Shown when there is no text (yet).
    pub empty_text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Toast {
    pub text: String,
    /// Offers Undo (a delete).
    pub undo: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MainView {
    pub words: PlatformWords,
    pub live: Option<LiveRow>,
    pub imports: Vec<ImportRow>,
    pub meetings: Vec<MeetingRow>,
    pub reader: Option<Reader>,
    pub toast: Option<Toast>,
    /// A problem to show at the top (capture stopped, model could not load), with Dismiss.
    pub error: Option<String>,
    /// "Transcribe Audio File…" is enabled (the speech model is ready).
    pub can_transcribe_file: bool,
    /// "Start Transcribing" is enabled.
    pub can_start: bool,
    pub transcribing: bool,
    /// "Show in Finder" / "Show in Explorer".
    pub show_in_file_manager: String,
    /// Shown when there are no transcripts at all.
    pub empty_text: Option<String>,
}

/// The small window that asks when notifications cannot.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptView {
    pub words: PlatformWords,
    pub id: u64,
    pub title: String,
    pub body: String,
    pub accept: String,
    pub decline: String,
}

/// One permission row, in setup and in Settings.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRow {
    pub permission: Permission,
    pub title: String,
    pub detail: String,
    pub required: bool,
    pub access: Access,
    /// "Allowed", "Not allowed", "Not asked yet", "Unknown".
    pub badge: String,
    pub granted: bool,
    /// The button: "Allow…" (asks), "Open System Settings…" / "Open Settings…" (denied), or none.
    pub button: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageChip {
    pub code: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguagesView {
    pub chosen: Vec<LanguageChip>,
    /// What "+ Add language" offers: the supported ones not chosen yet.
    pub available: Vec<LanguageChip>,
    /// The list differs from the defaults, so Reset does something.
    pub can_reset: bool,
    /// The last language cannot be removed.
    pub can_remove: bool,
}

/// The speech model, for the setup footer and Settings > Advanced.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelView {
    pub status: ModelStatus,
    pub title: String,
    /// "Ready", "Downloading · 42%", "Download failed", "Not downloaded".
    pub text: String,
    pub progress: Option<f64>,
    pub error: Option<String>,
    pub url: Option<String>,
    pub can_retry: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupView {
    pub words: PlatformWords,
    /// 0 Welcome, 1 Languages, 2 Permissions, 3 Done.
    pub step: usize,
    pub steps: usize,
    pub title: String,
    pub subtitle: Option<String>,
    /// Welcome: the three promises.
    pub promises: Vec<String>,
    /// One line under each promise.
    pub promise_details: Vec<String>,
    pub languages: LanguagesView,
    pub permissions: Vec<PermissionRow>,
    /// Windows: call audio needs no permission.
    pub permissions_note: Option<String>,
    /// The model footer, from the second screen on.
    pub model: Option<ModelView>,
    /// Done: "Tell people you are transcribing…".
    pub notice: Option<String>,
    pub primary: String,
    pub primary_enabled: bool,
    pub can_go_back: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantChoice {
    pub name: String,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeDesktopRow {
    /// "Connect Claude Desktop", "Update Claude Desktop extension", or none when connected.
    pub button: Option<String>,
    /// "Connected", "Not connected", "An older version is connected".
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssistantsView {
    pub enabled: bool,
    pub privacy: String,
    /// Only when the Claude desktop app is installed.
    pub claude_desktop: Option<ClaudeDesktopRow>,
    pub assistants: Vec<AssistantChoice>,
    /// For the selected assistant: what to do, and the command or config to copy.
    pub instructions: String,
    pub command: Option<String>,
    pub command_is_terminal: bool,
    /// "Add to Claude's config file" is offered (Claude Desktop installed, entry not added yet).
    pub can_add_to_claude_config: bool,
    pub claude_config_status: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub words: PlatformWords,
    pub version: String,
    pub transcripts_folder: String,
    /// `TRANSCRIPT_DIR` fixes the folder: Change… is disabled.
    pub transcripts_folder_locked: bool,
    pub start_at_login: bool,
    pub languages: LanguagesView,
    pub assistants: AssistantsView,
    pub model: ModelView,
    pub permissions: Vec<PermissionRow>,
}
