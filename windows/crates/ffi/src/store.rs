use std::path::Path;

use chrono::{DateTime, Utc};
use muesli_core::models::{Meeting, TranscriptSegment};
use muesli_core::store::{self, MeetingStore, RecentTranscript, Store};

use crate::paths::PathSet;

pub type RecentRecord = RecentTranscript;

#[uniffi::remote(Record)]
pub struct RecentRecord {
    pub path: std::path::PathBuf,
    pub title: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

/// Writes the `.md` / `.json` pair; returns the Markdown file.
#[uniffi::export]
pub fn store_save(meeting: Meeting, folder: String) -> String {
    Store::save(&meeting, Path::new(&folder)).to_string_lossy().into_owned()
}

#[uniffi::export]
pub fn store_recent(folder: String, limit: u32) -> Vec<String> {
    Store::recent(Path::new(&folder), limit as usize)
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

#[uniffi::export]
pub fn store_recent_transcripts(folder: String, limit: u32) -> Vec<RecentRecord> {
    Store::recent_transcripts(Path::new(&folder), limit as usize)
}

#[uniffi::export]
pub fn store_file_stem(meeting: Meeting) -> String {
    Store::file_stem(&meeting)
}

#[uniffi::export]
pub fn store_shorten_title(title: String, limit: u32) -> String {
    store::shorten_title(&title, limit as usize)
}

#[uniffi::export]
pub fn meeting_store_save(paths: PathSet, meeting: Meeting) {
    MeetingStore::new(&paths).save(&meeting);
}

#[uniffi::export]
pub fn meeting_store_load(paths: PathSet, id: String) -> Option<Meeting> {
    MeetingStore::new(&paths).load(&id)
}

#[uniffi::export]
pub fn meeting_store_all(paths: PathSet) -> Vec<Meeting> {
    MeetingStore::new(&paths).all()
}

#[uniffi::export]
pub fn meeting_store_recover_interrupted(paths: PathSet) -> Vec<Meeting> {
    MeetingStore::new(&paths).recover_interrupted()
}

#[uniffi::export]
pub fn meeting_store_import_transcripts(paths: PathSet, folder: String) -> Vec<Meeting> {
    MeetingStore::new(&paths).import_transcripts(Path::new(&folder))
}

/// The meeting with `segments` added (and saved). Unchanged when there are none.
#[uniffi::export]
pub fn meeting_store_append(paths: PathSet, mut meeting: Meeting, segments: Vec<TranscriptSegment>) -> Meeting {
    MeetingStore::new(&paths).append(&mut meeting, segments);
    meeting
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Finished {
    pub meeting: Meeting,
    /// The exported Markdown file.
    pub markdown: String,
}

#[uniffi::export]
pub fn meeting_store_finish(paths: PathSet, mut meeting: Meeting, now: DateTime<Utc>) -> Finished {
    let markdown = MeetingStore::new(&paths).finish(&mut meeting, now).to_string_lossy().into_owned();
    Finished { meeting, markdown }
}

#[uniffi::export]
pub fn meeting_store_latest_ready(paths: PathSet) -> Option<Meeting> {
    MeetingStore::new(&paths).latest_ready()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::meeting_new;
    use crate::preferences::{preferences_default, preferences_load, preferences_save};

    #[test]
    fn a_meeting_survives_the_store_and_preferences_survive_config_json() {
        let dir = tempfile::tempdir().unwrap();
        let paths = PathSet::new(dir.path().join("support"), dir.path().join("transcripts"));

        let mut m = meeting_new("Sync".into(), "manual".into());
        m.transcript.push(TranscriptSegment { start: 1.0, end: 2.0, speaker: "You".into(), text: "hi".into() });
        let done = meeting_store_finish(paths.clone(), m.clone(), Utc::now());
        assert!(done.markdown.ends_with(".md"));
        assert_eq!(meeting_store_load(paths.clone(), m.id.clone()).unwrap().transcript.len(), 1);
        assert_eq!(meeting_store_latest_ready(paths.clone()).unwrap().id, m.id);
        let folder = paths.transcripts.to_string_lossy().into_owned();
        assert_eq!(store_recent_transcripts(folder, 5).len(), 1);

        assert!(!preferences_load(paths.clone()).onboarding_done);
        let mut prefs = preferences_default();
        prefs.mcp_enabled = true;
        preferences_save(prefs.clone(), paths.clone());
        assert_eq!(preferences_load(paths), prefs);
    }
}
