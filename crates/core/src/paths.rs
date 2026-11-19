use std::path::{Path, PathBuf};

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
}

impl Paths {
    pub fn new(support: impl Into<PathBuf>, transcripts: impl Into<PathBuf>) -> Self {
        let support = support.into();
        Self { models: support.clone(), support, transcripts: transcripts.into() }
    }

    /// Platform defaults, with `MUESLI_SUPPORT_DIR` and `TRANSCRIPT_DIR` overriding them.
    ///
    /// macOS: `~/Library/Application Support/Muesli`; Windows: `%APPDATA%\Muesli`.
    /// Transcripts: `~/Documents/MeetingTranscripts`.
    pub fn from_env() -> Self {
        let support = std::env::var_os("MUESLI_SUPPORT_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join("Muesli"));
        let transcripts = std::env::var("TRANSCRIPT_DIR").map(|p| expand_tilde(&p)).unwrap_or_else(|_| {
            dirs::document_dir().or_else(dirs::home_dir).unwrap_or_else(|| PathBuf::from(".")).join("MeetingTranscripts")
        });
        let overridden = std::env::var_os("MUESLI_SUPPORT_DIR").is_some();
        let models = match dirs::data_local_dir() {
            Some(local) if cfg!(windows) && !overridden => local.join("Muesli"),
            _ => support.clone(),
        };
        Self { support, transcripts, models }
    }

    /// Whether `TRANSCRIPT_DIR` fixes the transcripts folder (the user's choice is then ignored).
    pub fn transcripts_set_by_environment() -> bool {
        std::env::var_os("TRANSCRIPT_DIR").is_some()
    }

    /// These paths with the transcripts folder the user chose, if any.
    pub fn with_preferences(mut self, prefs: &crate::Preferences) -> Self {
        if !Self::transcripts_set_by_environment() {
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
        if std::env::var_os("TRANSCRIPT_DIR").is_some() {
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

fn expand_tilde(path: &str) -> PathBuf {
    match (path.strip_prefix("~/"), dirs::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest),
        _ => PathBuf::from(path),
    }
}
