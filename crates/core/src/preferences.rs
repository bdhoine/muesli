use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::json::{self, to_pretty_sorted};
use crate::languages;
use crate::paths::Paths;
use crate::whisper_model::{DEFAULT_MODEL, VAD_MODEL};

/// Preferences shared by the app (writes) and muesli-mcp (reads): `config.json`.
///
/// Missing keys take their default. `onboarding_done` is the exception: a `config.json` that
/// predates onboarding counts as finished, while no file at all does not.
///
/// Keys from earlier versions (`autoStop`, `endDelaySeconds`, `recentLimit`, `customModelPath`,
/// `savedLanguages`, `dropFolder`, `dropFolderCleanup`, `model`) are read and dropped: the next
/// save leaves them out. [`Preferences::migrate`] says which ones a file still had. Keys added
/// later (the update keys) take their default when a file does not have them yet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Preferences {
    pub mcp_enabled: bool,
    pub claude_desktop_config_entry: bool,
    /// The languages Whisper may pick from. `None`: the defaults (the system language and English).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub languages: Option<Vec<String>>,
    #[serde(default = "done_when_missing")]
    pub onboarding_done: bool,
    /// Where transcripts are written, when the user chose a folder.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transcripts_folder: Option<String>,
    /// The Muesli version whose Claude Desktop extension update was announced in a system
    /// notification, so each version is announced once.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension_update_notified: Option<String>,
    /// The Muesli version whose update banner the user put away with "Not now".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extension_update_dismissed: Option<String>,
    /// "Check for updates automatically" in Settings. On by default.
    #[serde(default = "yes")]
    pub auto_update_check: bool,
    /// When Muesli last asked GitHub for a newer version and got an answer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_last_checked: Option<DateTime<Utc>>,
    /// The version whose "ready to install" notification was shown, so each is announced once.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_notified: Option<String>,
    /// The version that ran last, so the first start of a new version is noticed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run_version: Option<String>,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            mcp_enabled: false,
            claude_desktop_config_entry: false,
            languages: None,
            onboarding_done: false,
            transcripts_folder: None,
            extension_update_notified: None,
            extension_update_dismissed: None,
            auto_update_check: true,
            update_last_checked: None,
            update_notified: None,
            last_run_version: None,
        }
    }
}

/// The keys earlier versions wrote that this one no longer has.
pub const REMOVED_KEYS: [&str; 8] =
    ["autoStop", "endDelaySeconds", "recentLimit", "customModelPath", "savedLanguages", "dropFolder", "dropFolderCleanup", "model"];

fn done_when_missing() -> bool {
    true
}

fn yes() -> bool {
    true
}

impl Preferences {
    /// The languages Whisper may pick from: the chosen ones, else the system defaults.
    pub fn effective_languages(&self, system_defaults: &[String]) -> Vec<String> {
        let chosen = languages::unique(
            self.languages.iter().flatten().map(|l| languages::base_code(l)).filter(|l| languages::is_supported(l)).collect(),
        );
        if chosen.is_empty() {
            system_defaults.to_vec()
        } else {
            chosen
        }
    }

    /// Adds a language to the list (starting from the defaults the first time).
    pub fn add_language(&mut self, code: &str, system_defaults: &[String]) {
        let code = languages::base_code(code);
        if !languages::is_supported(&code) {
            return;
        }
        let mut list = self.effective_languages(system_defaults);
        if !list.contains(&code) {
            list.push(code);
        }
        self.languages = Some(list);
    }

    /// Removes a language. The last one stays: Whisper needs at least one.
    pub fn remove_language(&mut self, code: &str, system_defaults: &[String]) {
        let list: Vec<String> = self.effective_languages(system_defaults).into_iter().filter(|l| l != code).collect();
        if !list.is_empty() {
            self.languages = Some(list);
        }
    }

    /// Back to the system language and English.
    pub fn reset_languages(&mut self) {
        self.languages = None;
    }

    /// The speech model Whisper loads. There is one model and nothing to choose.
    pub fn model_path(&self, paths: &Paths) -> PathBuf {
        DEFAULT_MODEL.path(paths)
    }

    /// The voice activity model.
    pub fn vad_model_path(&self, paths: &Paths) -> PathBuf {
        VAD_MODEL.path(paths)
    }

    /// A missing or unreadable file gives the defaults (and a first-launch onboarding).
    pub fn load(paths: &Paths) -> Self {
        std::fs::read_to_string(paths.config()).ok().and_then(|text| json::from_str(&text)).unwrap_or_default()
    }

    pub fn save(&self, paths: &Paths) -> std::io::Result<()> {
        json::write_atomic(&paths.config(), &to_pretty_sorted(self))
    }

    /// Loads `config.json` and, if it still has keys from an earlier version, writes it back
    /// without them. Returns the preferences and the keys that were dropped.
    pub fn migrate(paths: &Paths) -> (Self, Vec<&'static str>) {
        let prefs = Self::load(paths);
        let old: Vec<&'static str> = std::fs::read_to_string(paths.config())
            .ok()
            .and_then(|text| serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&text).ok())
            .map(|map| REMOVED_KEYS.iter().copied().filter(|k| map.contains_key(*k)).collect())
            .unwrap_or_default();
        if !old.is_empty() {
            let _ = prefs.save(paths);
        }
        (prefs, old)
    }
}
