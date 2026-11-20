use std::path::{Path, PathBuf};

use crate::ports::PlatformPaths;

/// Where Muesli keeps its data. Passed around explicitly instead of read from globals,
/// so tests can run in parallel in their own folders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    /// App data: Whisper models, state.json, config.json, meetings/.
    pub support: PathBuf,
    /// Where finished transcripts are written.
    pub transcripts: PathBuf,
    /// Where downloaded Whisper models live. The support folder, except on Windows, where large
    /// files belong in the local (non-roaming) profile.
    pub models: PathBuf,
    /// `TRANSCRIPT_DIR` fixes the transcripts folder.
    pub transcripts_locked: bool,
}

impl Paths {
    pub fn new(support: impl Into<PathBuf>, transcripts: impl Into<PathBuf>) -> Self {
        let support = support.into();
        Self { models: support.clone(), support, transcripts: transcripts.into(), transcripts_locked: false }
    }

    /// Platform defaults, with `MUESLI_SUPPORT_DIR` and `TRANSCRIPT_DIR` overriding them.
    ///
    /// macOS: `~/Library/Application Support/Muesli`; Windows: `%APPDATA%\Muesli`, with models in
    /// `%LOCALAPPDATA%\Muesli`. Transcripts: `~/Documents/MeetingTranscripts`.
    pub fn from_platform(platform: &dyn PlatformPaths) -> Self {
        let home = platform.home_dir();
        let overridden = platform.env("MUESLI_SUPPORT_DIR").filter(|s| !s.is_empty());
        let support = overridden.as_deref().map(|p| expand_tilde(p, &home)).unwrap_or_else(|| platform.data_dir().join("Muesli"));
        let transcript_env = platform.env("TRANSCRIPT_DIR").filter(|s| !s.is_empty());
        let transcripts = transcript_env
            .as_deref()
            .map(|p| expand_tilde(p, &home))
            .unwrap_or_else(|| platform.documents_dir().join("MeetingTranscripts"));
        let models = if overridden.is_some() { support.clone() } else { platform.local_data_dir().join("Muesli") };
        Self { support, transcripts, models, transcripts_locked: transcript_env.is_some() }
    }

    /// Whether `TRANSCRIPT_DIR` fixes the transcripts folder (the user's choice is then ignored).
    pub fn transcripts_set_by_environment(&self) -> bool {
        self.transcripts_locked
    }

    /// These paths with the transcripts folder the user chose, if any.
    pub fn with_preferences(mut self, prefs: &crate::Preferences) -> Self {
        if !self.transcripts_locked {
            if let Some(folder) = prefs.transcripts_folder.as_deref().filter(|f| !f.trim().is_empty()) {
                self.transcripts = PathBuf::from(folder);
            }
        }
        self
    }

    pub fn state(&self) -> PathBuf {
        self.support.join("state.json")
    }

    pub fn config(&self) -> PathBuf {
        self.support.join("config.json")
    }

    /// Names and terms for Whisper (`vocabulary.rs`), shared by the app and `muesli-mcp`.
    pub fn vocabulary(&self) -> PathBuf {
        self.support.join("vocabulary.json")
    }

    pub fn audit_log(&self) -> PathBuf {
        self.support.join("audit.log")
    }

    /// The canonical meeting store. Created on demand.
    pub fn meetings(&self) -> PathBuf {
        let dir = self.support.join("meetings");
        let _ = std::fs::create_dir_all(&dir);
        dir
    }

    /// Where exported transcripts live as far as other processes can tell: `TRANSCRIPT_DIR` if set,
    /// else the folder the running app recorded in `state.json`, else the default.
    pub fn export_folder(&self) -> PathBuf {
        if self.transcripts_locked {
            return self.transcripts.clone();
        }
        crate::status::AppStatus::read(self)
            .and_then(|s| s.transcripts_folder)
            .filter(|f| !f.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| self.transcripts.clone())
    }

    /// The transcripts folder. Created on demand.
    pub fn transcripts_dir(&self) -> &Path {
        let _ = std::fs::create_dir_all(&self.transcripts);
        &self.transcripts
    }
}

fn expand_tilde(path: &str, home: &Path) -> PathBuf {
    match path.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None => PathBuf::from(path),
    }
}
