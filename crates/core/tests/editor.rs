mod common;

use std::cell::RefCell;

use common::*;
use muesli_core::editor::{EditError, MeetingEditor};
use muesli_core::store::{MeetingStore, Store};
use muesli_core::MeetingState;

fn ready(env: &Env, id: &str, title: &str) {
    let m = meeting(id, MeetingState::Ready, 2, title, t0());
    MeetingStore::new(&env.paths).save(&m);
    Store::save(&m, &env.paths.transcripts);
}

fn exported(env: &Env) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(&env.paths.transcripts)
        .map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn rename_updates_the_store_and_renames_the_files() {
    let env = Env::new();
    ready(&env, "a", "Weekly sync");
    assert_eq!(exported(&env).len(), 2);
    let editor = MeetingEditor::new(&env.paths, &env.paths.transcripts);
    let renamed = editor.rename("a", "  Planning\nQ3 ").unwrap();
    assert_eq!(renamed.title, "Planning Q3");
    assert_eq!(MeetingStore::new(&env.paths).load("a").unwrap().title, "Planning Q3");
    let after = exported(&env);
    assert_eq!(after.len(), 2);
    assert!(after.iter().all(|n| n.contains("Planning-Q3")));
    let md = std::fs::read_to_string(env.paths.transcripts.join(format!("{}.md", Store::file_stem(&renamed)))).unwrap();
    assert!(md.starts_with("# Planning Q3"));
}

#[test]
fn rename_rejects_empty_and_too_long_titles() {
    let env = Env::new();
    ready(&env, "a", "Weekly sync");
    let editor = MeetingEditor::new(&env.paths, &env.paths.transcripts);
    assert_eq!(editor.rename("a", "  "), Err(EditError::Invalid("The title cannot be empty.".into())));
    assert!(editor.rename("a", &"x".repeat(201)).is_err());
    assert_eq!(MeetingStore::new(&env.paths).load("a").unwrap().title, "Weekly sync");
}

#[test]
fn rename_never_overwrites_another_meetings_files() {
    let env = Env::new();
    ready(&env, "a", "One");
    ready(&env, "b", "Two");
    let editor = MeetingEditor::new(&env.paths, &env.paths.transcripts);
    assert!(matches!(editor.rename("a", "Two"), Err(EditError::Invalid(_))));
    assert_eq!(MeetingStore::new(&env.paths).load("a").unwrap().title, "One");
}

#[test]
fn recording_meetings_are_busy() {
    let env = Env::new();
    MeetingStore::new(&env.paths).save(&simple("live", MeetingState::Recording));
    let editor = MeetingEditor::new(&env.paths, &env.paths.transcripts);
    assert_eq!(editor.rename("live", "x"), Err(EditError::Busy(MeetingState::Recording)));
    assert_eq!(editor.delete("live"), Err(EditError::Busy(MeetingState::Recording)));
    assert_eq!(editor.rename("nope", "x"), Err(EditError::NotFound));
}

#[test]
fn attendees_are_cleaned_and_deduplicated() {
    let env = Env::new();
    ready(&env, "a", "Weekly sync");
    let editor = MeetingEditor::new(&env.paths, &env.paths.transcripts);
    let m = editor.set_attendees("a", &["Ann".into(), " ann ".into(), "".into(), "Bob\nSmith".into()]).unwrap();
    assert_eq!(m.attendees, ["Ann", "Bob Smith"]);
}

#[test]
fn rename_speaker_changes_every_matching_segment() {
    let env = Env::new();
    ready(&env, "a", "Weekly sync");
    let editor = MeetingEditor::new(&env.paths, &env.paths.transcripts);
    let m = editor.rename_speaker("a", "Others", "Ann").unwrap();
    assert!(m.transcript.iter().any(|s| s.speaker == "Ann"));
    assert!(m.transcript.iter().all(|s| s.speaker != "Others"));
    let err = editor.rename_speaker("a", "Nobody", "X").unwrap_err();
    assert!(err.message().contains("Speakers: Ann, You."));
}

#[test]
fn delete_discards_exports_and_the_stored_copy() {
    let env = Env::new();
    ready(&env, "a", "Weekly sync");
    let discarded = RefCell::new(Vec::new());
    let editor = MeetingEditor::new(&env.paths, &env.paths.transcripts).with_discard(|p| {
        discarded.borrow_mut().push(p.file_name().unwrap().to_string_lossy().into_owned());
        let _ = std::fs::remove_file(p);
    });
    editor.delete("a").unwrap();
    assert_eq!(discarded.borrow().len(), 3);
    assert!(MeetingStore::new(&env.paths).load("a").is_none());
    assert!(exported(&env).is_empty());
}
