use std::fs;
use std::path::{Path, PathBuf};

use crate::json;
use crate::models::{Meeting, MeetingState};
use crate::paths::Paths;
use crate::store::{MeetingStore, Store};

pub const MAX_TITLE_LENGTH: usize = 200;
pub const MAX_SPEAKER_LENGTH: usize = 60;
pub const MAX_ATTENDEES: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditError {
    NotFound,
    Busy(MeetingState),
    Invalid(String),
}

impl EditError {
    pub fn message(&self) -> String {
        match self {
            Self::NotFound => "No meeting with that id. Use list_transcripts to find ids.".into(),
            Self::Busy(state) => format!("This meeting is still {}. Try again once it is ready.", state.as_str()),
            Self::Invalid(reason) => reason.clone(),
        }
    }
}

type Discard<'a> = Box<dyn Fn(&Path) + 'a>;

/// Changes to a finished meeting: rename, attendees, speakers, delete. Updates the canonical
/// store and the exported files together.
pub struct MeetingEditor<'a> {
    store: MeetingStore<'a>,
    /// Folder holding the exported transcript files.
    folder: PathBuf,
    discard: Discard<'a>,
}

impl<'a> MeetingEditor<'a> {
    pub fn new(paths: &'a Paths, folder: impl Into<PathBuf>) -> Self {
        Self {
            store: MeetingStore::new(paths),
            folder: folder.into(),
            discard: Box::new(|p| {
                let _ = fs::remove_file(p);
            }),
        }
    }

    /// Replaces how deleted files are discarded (the app moves them to the trash).
    pub fn with_discard(mut self, discard: impl Fn(&Path) + 'a) -> Self {
        self.discard = Box::new(discard);
        self
    }

    pub fn rename(&self, id: &str, title: &str) -> Result<Meeting, EditError> {
        let clean = single_line(title);
        if clean.is_empty() {
            return Err(EditError::Invalid("The title cannot be empty.".into()));
        }
        if clean.chars().count() > MAX_TITLE_LENGTH {
            return Err(EditError::Invalid(format!("The title is longer than {MAX_TITLE_LENGTH} characters.")));
        }
        self.edit(id, |m| {
            m.title = clean;
            Ok(())
        })
    }

    pub fn set_attendees(&self, id: &str, attendees: &[String]) -> Result<Meeting, EditError> {
        let mut seen = std::collections::HashSet::new();
        let clean: Vec<String> = attendees
            .iter()
            .map(|a| single_line(a))
            .filter(|a| !a.is_empty() && seen.insert(a.to_lowercase()))
            .collect();
        if clean.len() > MAX_ATTENDEES {
            return Err(EditError::Invalid(format!("At most {MAX_ATTENDEES} attendees.")));
        }
        if clean.iter().any(|a| a.chars().count() > MAX_TITLE_LENGTH) {
            return Err(EditError::Invalid(format!(
                "An attendee name is longer than {MAX_TITLE_LENGTH} characters."
            )));
        }
        self.edit(id, |m| {
            m.attendees = clean;
            Ok(())
        })
    }

    pub fn rename_speaker(&self, id: &str, from: &str, to: &str) -> Result<Meeting, EditError> {
        let clean = single_line(to);
        if clean.is_empty() {
            return Err(EditError::Invalid("The new speaker name cannot be empty.".into()));
        }
        if clean.chars().count() > MAX_SPEAKER_LENGTH {
            return Err(EditError::Invalid(format!(
                "The speaker name is longer than {MAX_SPEAKER_LENGTH} characters."
            )));
        }
        self.edit(id, |m| {
            let mut speakers: Vec<String> = m.transcript.iter().map(|s| s.speaker.clone()).collect();
            speakers.sort();
            speakers.dedup();
            if !speakers.iter().any(|s| s == from) {
                return Err(EditError::Invalid(format!(
                    "No speaker called \"{from}\" in this transcript. Speakers: {}.",
                    speakers.join(", ")
                )));
            }
            for segment in m.transcript.iter_mut().filter(|s| s.speaker == from) {
                segment.speaker = clean.clone();
            }
            Ok(())
        })
    }

    pub fn delete(&self, id: &str) -> Result<Meeting, EditError> {
        let m = self.store.load(id).ok_or(EditError::NotFound)?;
        if let Some(busy) = busy_state(&m) {
            return Err(EditError::Busy(busy));
        }
        let stem = self.folder.join(Store::file_stem(&m));
        if owned_by(&m.id, &stem) {
            (self.discard)(&stem.with_extension("md"));
            (self.discard)(&stem.with_extension("json"));
        }
        if let Some(stored) = self.store.url(&m.id) {
            (self.discard)(&stored);
        }
        Ok(m)
    }

    fn edit(&self, id: &str, change: impl FnOnce(&mut Meeting) -> Result<(), EditError>) -> Result<Meeting, EditError> {
        let mut m = self.store.load(id).ok_or(EditError::NotFound)?;
        if let Some(busy) = busy_state(&m) {
            return Err(EditError::Busy(busy));
        }
        let old = m.clone();
        change(&mut m)?;

        let old_stem = self.folder.join(Store::file_stem(&old));
        let new_stem = self.folder.join(Store::file_stem(&m));
        if new_stem != old_stem && !owned_by(&m.id, &new_stem) && exists(&new_stem) {
            return Err(EditError::Invalid(
                "Another transcript that started in the same minute already has that title.".into(),
            ));
        }

        self.store.save(&m);
        if m.state == MeetingState::Ready {
            if new_stem != old_stem && owned_by(&m.id, &old_stem) {
                let _ = fs::remove_file(old_stem.with_extension("md"));
                let _ = fs::remove_file(old_stem.with_extension("json"));
            }
            Store::save(&m, &self.folder);
        }
        Ok(m)
    }
}

fn busy_state(m: &Meeting) -> Option<MeetingState> {
    matches!(m.state, MeetingState::Recording | MeetingState::Finalizing | MeetingState::Detected).then_some(m.state)
}

fn exists(stem: &Path) -> bool {
    stem.with_extension("md").exists() || stem.with_extension("json").exists()
}

fn owned_by(id: &str, stem: &Path) -> bool {
    fs::read_to_string(stem.with_extension("json"))
        .ok()
        .and_then(|t| json::from_str::<Meeting>(&t))
        .is_some_and(|other| other.id == id)
}

/// Replaces every line break with a space and trims blanks at both ends.
pub fn single_line(s: &str) -> String {
    let is_break = |c: char| matches!(c, '\n' | '\u{0B}' | '\u{0C}' | '\r' | '\u{85}' | '\u{2028}' | '\u{2029}');
    s.chars().map(|c| if is_break(c) { ' ' } else { c }).collect::<String>().trim().to_string()
}
