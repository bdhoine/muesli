use std::path::PathBuf;

use muesli_core::ports::Permission;
use serde::Deserialize;

/// Everything a window, the tray or a notification can ask the engine to do. The web UI sends
/// these as JSON: `{"type": "rename", "id": "…", "title": "…"}`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Intent {
    // Transcribing
    StartTranscribing,
    StopAndSave,
    /// The answer to the open question. `prompt` is the question's id when it comes from a
    /// notification (a late answer to an older question is ignored); the prompt window leaves it out.
    AnswerPrompt {
        prompt: Option<u64>,
        accept: bool,
    },

    // Windows and the tray
    OpenMuesli,
    OpenSettings,
    /// A click in the tray menu, by item id.
    Tray {
        id: String,
    },
    /// Opens the main window on this meeting.
    OpenMeeting {
        id: String,
    },
    Quit,

    // Audio files
    /// Opens the file picker.
    ChooseAudioFiles,
    /// Files picked, or dropped on a Muesli window.
    TranscribeFiles {
        paths: Vec<PathBuf>,
    },
    CancelImport {
        id: u64,
    },
    /// Removes a failed import from the list.
    DismissImport {
        id: u64,
    },

    // Main window
    Select {
        id: String,
    },
    Rename {
        id: String,
        title: String,
    },
    /// Moves a finished meeting to the Trash, after the undo toast is gone.
    Delete {
        id: String,
    },
    Undo,
    DismissToast,
    /// Shows the meeting's file, or the transcripts folder, in Finder / Explorer.
    ShowInFileManager {
        id: Option<String>,
    },
    DismissError,

    // Settings
    ChooseTranscriptsFolder,
    SetTranscriptsFolder {
        path: PathBuf,
    },
    SetStartAtLogin {
        on: bool,
    },
    AddLanguage {
        code: String,
    },
    RemoveLanguage {
        code: String,
    },
    ResetLanguages,
    SetAssistantsEnabled {
        on: bool,
    },
    ConnectClaudeDesktop,
    /// "Not now" on the main window's extension update banner.
    DismissExtensionUpdate,
    AddToClaudeConfig,
    /// Picks the assistant whose setup command "Other apps…" shows.
    PickAssistant {
        name: String,
    },
    /// "Check for Updates" / "Try again" in Settings.
    CheckForUpdates,
    /// Installs the ready update and starts the new version.
    RestartToUpdate,
    /// "Not now" on the update banner: hidden until the next start.
    DismissUpdate,
    /// "What's new": the release notes of a version on GitHub.
    OpenReleaseNotes {
        version: String,
    },
    SetAutoUpdateCheck {
        on: bool,
    },
    DownloadModelAgain,
    RequestPermission {
        permission: Permission,
    },
    OpenPermissionSettings {
        permission: Permission,
    },

    // Setup
    SetupNext,
    SetupBack,
    FinishSetup,
}
