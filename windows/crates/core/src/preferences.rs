use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::json::{self, to_pretty_sorted};
use crate::languages;
use crate::paths::Paths;
use crate::whisper_model::{DEFAULT_MODEL, VAD_MODEL};

pub const RECENT_LIMIT_OPTIONS: [i32; 4] = [0, 3, 5, 10];
pub const MAX_RECENT_LIMIT: i32 = 10;
pub const END_DELAY_OPTIONS: [i32; 4] = [30, 60, 120, 300];

/// Preferences shared by the app (writes) and muesli-mcp (reads): `config.json`.
///
/// Missing keys take their default. `onboarding_done` is the exception: a `config.json` that
/// predates onboarding counts as finished, while no file at all does not.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Preferences {
    pub mcp_enabled: bool,
    pub claude_desktop_config_entry: bool,
    pub auto_stop: bool,
    pub end_delay_seconds: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub languages: Option<Vec<String>>,
    pub recent_limit: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub custom_model_path: Option<String>,
    #[serde(default = "done_when_missing")]
    pub onboarding_done: bool,
    /// Where transcripts are written, when the user chose a folder. Both apps keep it here
    /// (the macOS app moved it over from its own defaults once).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcripts_folder: Option<String>,
    /// The user's own language list, kept while "use my computer's languages" is on, so it comes back.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub saved_languages: Option<Vec<String>>,
}

fn done_when_missing() -> bool {
    true
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            mcp_enabled: false,
            claude_desktop_config_entry: false,
            auto_stop: false,
            end_delay_seconds: 60,
            languages: None,
            recent_limit: 5,
            custom_model_path: None,
            onboarding_done: false,
            transcripts_folder: None,
            saved_languages: None,
        }
    }
}

impl Preferences {
    pub fn effective_recent_limit(&self) -> i32 {
        self.recent_limit.clamp(0, MAX_RECENT_LIMIT)
    }

    /// Whether the languages follow the system (`languages` is unset).
    pub fn uses_system_languages(&self) -> bool {
        self.languages.is_none()
    }

    /// Switches between the system's languages and the user's own list without losing that list.
    pub fn set_uses_system_languages(&mut self, on: bool, system_defaults: &[String]) {
        if on {
            if let Some(own) = self.languages.take() {
                self.saved_languages = Some(own);
            }
        } else if self.languages.is_none() {
            self.languages = Some(self.saved_languages.clone().unwrap_or_else(|| system_defaults.to_vec()));
        }
    }

    /// The languages Whisper may pick from: the chosen ones, else the system defaults.
    pub fn effective_languages(&self, system_defaults: &[String]) -> Vec<String> {
        let chosen = languages::unique(
            self.languages
                .iter()
                .flatten()
                .map(|l| languages::base_code(l))
                .filter(|l| languages::is_supported(l))
                .collect(),
        );
        if chosen.is_empty() { system_defaults.to_vec() } else { chosen }
    }

    /// A model file the user chose themselves; it replaces the downloadable models.
    pub fn custom_model_url(&self) -> Option<PathBuf> {
        let path = self.custom_model_path.as_deref().filter(|p| !p.is_empty())?;
        match (path.strip_prefix("~/"), dirs::home_dir()) {
            (Some(rest), Some(home)) => Some(home.join(rest)),
            _ => Some(PathBuf::from(path)),
        }
    }

    /// The file Whisper will load: the custom model if set, else the default download.
    pub fn model_path(&self, paths: &Paths) -> PathBuf {
        self.custom_model_url().unwrap_or_else(|| DEFAULT_MODEL.path(paths))
    }

    /// The voice activity model. Always the downloaded one, also with a custom speech model.
    pub fn vad_model_path(&self, paths: &Paths) -> PathBuf {
        VAD_MODEL.path(paths)
    }

    /// A missing or unreadable file gives the defaults (and a first-launch onboarding).
    pub fn load(paths: &Paths) -> Self {
        std::fs::read_to_string(paths.config())
            .ok()
            .and_then(|text| json::from_str(&text))
            .unwrap_or_default()
    }

    pub fn save(&self, paths: &Paths) -> std::io::Result<()> {
        json::write_atomic(&paths.config(), &to_pretty_sorted(self))
    }
}
