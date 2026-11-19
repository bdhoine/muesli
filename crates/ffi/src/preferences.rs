use muesli_core::preferences::{self, Preferences};
use muesli_core::AudioCleanup;

use crate::paths::PathSet;

#[uniffi::remote(Record)]
pub struct Preferences {
    pub mcp_enabled: bool,
    pub claude_desktop_config_entry: bool,
    pub auto_stop: bool,
    pub end_delay_seconds: i32,
    pub languages: Option<Vec<String>>,
    pub recent_limit: i32,
    pub custom_model_path: Option<String>,
    pub onboarding_done: bool,
    pub transcripts_folder: Option<String>,
    pub saved_languages: Option<Vec<String>>,
    pub drop_folder: Option<String>,
    pub drop_folder_cleanup: AudioCleanup,
}

#[uniffi::export]
pub fn preferences_default() -> Preferences {
    Preferences::default()
}

/// A missing or unreadable file gives the defaults (and a first-launch onboarding).
#[uniffi::export]
pub fn preferences_load(paths: PathSet) -> Preferences {
    Preferences::load(&paths)
}

#[uniffi::export]
pub fn preferences_save(prefs: Preferences, paths: PathSet) {
    let _ = prefs.save(&paths);
}

#[uniffi::export]
pub fn preferences_recent_limit_options() -> Vec<i32> {
    preferences::RECENT_LIMIT_OPTIONS.to_vec()
}

#[uniffi::export]
pub fn preferences_max_recent_limit() -> i32 {
    preferences::MAX_RECENT_LIMIT
}

#[uniffi::export]
pub fn preferences_end_delay_options() -> Vec<i32> {
    preferences::END_DELAY_OPTIONS.to_vec()
}

#[uniffi::export]
pub fn preferences_effective_recent_limit(prefs: Preferences) -> i32 {
    prefs.effective_recent_limit()
}

#[uniffi::export]
pub fn preferences_set_uses_system_languages(mut prefs: Preferences, on: bool, system_defaults: Vec<String>) -> Preferences {
    prefs.set_uses_system_languages(on, &system_defaults);
    prefs
}

#[uniffi::export]
pub fn preferences_effective_languages(prefs: Preferences, system_defaults: Vec<String>) -> Vec<String> {
    prefs.effective_languages(&system_defaults)
}

/// The model file the user chose themselves, if any.
#[uniffi::export]
pub fn preferences_custom_model_path(prefs: Preferences) -> Option<String> {
    prefs.custom_model_url().map(|p| p.to_string_lossy().into_owned())
}

/// The voice activity model Whisper needs next to the speech model.
#[uniffi::export]
pub fn preferences_vad_model_path(prefs: Preferences, paths: PathSet) -> String {
    prefs.vad_model_path(&paths).to_string_lossy().into_owned()
}

/// The file Whisper will load: the custom model if set, else the default download.
#[uniffi::export]
pub fn preferences_model_path(prefs: Preferences, paths: PathSet) -> String {
    prefs.model_path(&paths).to_string_lossy().into_owned()
}
