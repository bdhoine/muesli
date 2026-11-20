use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Muesli's windows. Each shows one view state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Window {
    /// The list of transcripts and the reader.
    Main,
    Settings,
    /// The first-launch setup.
    Setup,
    /// The small question window, when notifications cannot ask.
    Prompt,
}

impl Window {
    pub const ALL: [Window; 4] = [Self::Main, Self::Settings, Self::Setup, Self::Prompt];

    pub fn label(self) -> &'static str {
        match self {
            Self::Main => "main",
            Self::Settings => "settings",
            Self::Setup => "setup",
            Self::Prompt => "prompt",
        }
    }

    pub fn from_label(label: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|w| w.label() == label)
    }
}

/// What the shell around the engine does for it. The Tauri app implements it with windows and
/// dialogs; the simulator and the tests with a browser page or a recorder.
///
/// The engine never calls these while it holds its own lock, so an implementation may call back
/// into the engine.
pub trait Host: Send + Sync {
    /// Opens a window, or brings it to the front.
    fn show(&self, window: Window);
    fn close(&self, window: Window);
    /// Something the windows show has changed: they should ask for their view again.
    fn changed(&self);
    /// Asks for audio files; the answer comes back as `Intent::TranscribeFiles`.
    fn pick_audio_files(&self);
    /// Asks for a folder; the answer comes back as `Intent::SetTranscriptsFolder`.
    fn pick_transcripts_folder(&self);
    /// Shows a file or folder in Finder / Explorer.
    fn reveal(&self, path: &Path);
    /// Opens a file with its app (the Claude Desktop extension).
    fn open(&self, path: &Path);
    /// Opens a web page in the browser (release notes on GitHub).
    fn open_url(&self, url: &str);
    fn quit(&self);
    /// The `muesli-mcp` executable shipped with the app, if any.
    fn helper_path(&self) -> Option<PathBuf>;
    /// The Claude Desktop extension (`Muesli.mcpb`) shipped with the app, if any.
    fn extension_path(&self) -> Option<PathBuf>;
    /// Whether the Claude desktop app is installed.
    fn claude_installed(&self) -> bool;
}
