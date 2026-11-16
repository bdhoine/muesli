//! Where Muesli keeps its data. The Swift app asks for the current paths on every use, so the
//! `MUESLI_SUPPORT_DIR` / `TRANSCRIPT_DIR` environment overrides and the transcripts folder in
//! `config.json` always apply.

use std::path::PathBuf;

use muesli_core::paths::Paths;
use muesli_core::Preferences;

uniffi::custom_type!(PathBuf, String, {
    remote,
    lower: |p| p.to_string_lossy().into_owned(),
    try_lift: |s| Ok(PathBuf::from(s)),
});

/// Named `PathSet` in Swift, where `Paths` is the app's facade over it.
pub type PathSet = Paths;

#[uniffi::remote(Record)]
pub struct PathSet {
    pub support: PathBuf,
    pub transcripts: PathBuf,
    pub models: PathBuf,
}

/// The paths in effect now: environment overrides, then the folder chosen in `config.json`.
/// Creates the support and transcripts folders.
#[uniffi::export]
pub fn paths_current() -> PathSet {
    let base = Paths::from_env();
    let prefs = Preferences::load(&base);
    let paths = base.with_preferences(&prefs);
    let _ = std::fs::create_dir_all(&paths.support);
    let _ = std::fs::create_dir_all(&paths.transcripts);
    paths
}

/// Whether `TRANSCRIPT_DIR` fixes the transcripts folder.
#[uniffi::export]
pub fn paths_transcripts_set_by_environment() -> bool {
    Paths::transcripts_set_by_environment()
}

/// Remembers the user's transcripts folder in `config.json` (`None` goes back to the default).
#[uniffi::export]
pub fn paths_set_transcripts_folder(folder: Option<String>) {
    let base = Paths::from_env();
    let mut prefs = Preferences::load(&base);
    prefs.transcripts_folder = folder;
    let _ = prefs.save(&base);
}

#[uniffi::export]
pub fn paths_state_file(paths: PathSet) -> String {
    paths.state().to_string_lossy().into_owned()
}

#[uniffi::export]
pub fn paths_config_file(paths: PathSet) -> String {
    paths.config().to_string_lossy().into_owned()
}

#[uniffi::export]
pub fn paths_audit_log(paths: PathSet) -> String {
    paths.audit_log().to_string_lossy().into_owned()
}

#[uniffi::export]
pub fn paths_meetings_dir(paths: PathSet) -> String {
    paths.meetings().to_string_lossy().into_owned()
}

/// Where other processes find the exported transcripts.
#[uniffi::export]
pub fn paths_export_folder(paths: PathSet) -> String {
    paths.export_folder().to_string_lossy().into_owned()
}
