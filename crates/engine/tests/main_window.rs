//! The main window: list, reader, rename, delete with undo, audio files.

mod common;

use std::path::PathBuf;

use common::{wait_for, Harness};
use muesli_core::ports::PlatformWords;
use muesli_engine::fake::{FakeSpeech, HostCall, RecordingHost};
use muesli_engine::{Engine, Intent, Options, Window, TOAST_SECONDS};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/audio").join(name)
}

/// A finished meeting with some text; returns its id.
fn finished_meeting(h: &Harness) -> String {
    h.engine.intent(Intent::StartTranscribing);
    wait_for("transcribing", || h.transcribing());
    h.talk(31.0);
    wait_for("text", || h.engine.main_view().reader.is_some_and(|r| !r.lines.is_empty()));
    h.engine.intent(Intent::StopAndSave);
    wait_for("saved", || h.idle());
    h.engine.main_view().meetings[0].id.clone()
}

fn exported_files(h: &Harness) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(&h.engine.paths().transcripts)
        .map(|d| d.flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn an_empty_list_says_what_will_happen() {
    let h = Harness::ready();
    let view = h.engine.main_view();
    assert!(view.meetings.is_empty() && view.reader.is_none());
    assert!(view.empty_text.unwrap().contains("Muesli asks"));
    assert_eq!(view.show_in_file_manager, "Show in Finder");
}

#[test]
fn the_reader_shows_a_finished_meeting() {
    let h = Harness::ready();
    let id = finished_meeting(&h);
    let reader = h.engine.main_view().reader.unwrap();
    assert_eq!(reader.id, id);
    assert!(reader.editable && !reader.live && reader.badge.is_none());
    assert_eq!(reader.source, "Started by hand");
    assert!(reader.lines.iter().all(|l| l.time.len() == 5 && !l.text.is_empty()));
}

#[test]
fn rename_renames_the_exported_files_too() {
    let h = Harness::ready();
    let id = finished_meeting(&h);
    assert!(exported_files(&h).iter().all(|f| f.contains("_Meeting.")));
    h.engine.intent(Intent::Rename { id: id.clone(), title: "Roadmap review".into() });
    assert_eq!(h.engine.main_view().reader.unwrap().title, "Roadmap review");
    let files = exported_files(&h);
    assert_eq!(files.len(), 2);
    assert!(files.iter().all(|f| f.contains("_Roadmap-review.")), "{files:?}");
}

#[test]
fn delete_hides_at_once_offers_undo_and_trashes_after_the_toast() {
    let h = Harness::ready();
    let id = finished_meeting(&h);
    h.engine.intent(Intent::Delete { id: id.clone() });
    let view = h.engine.main_view();
    assert!(view.meetings.is_empty(), "gone from the list without a confirmation");
    let toast = view.toast.unwrap();
    assert!(toast.undo && toast.text.contains("moved to the Trash"), "{}", toast.text);
    assert_eq!(exported_files(&h).len(), 2, "nothing is trashed while Undo is offered");

    h.engine.intent(Intent::Undo);
    let view = h.engine.main_view();
    assert_eq!(view.meetings.len(), 1);
    assert!(view.toast.is_none());
    assert_eq!(view.reader.unwrap().id, id);

    h.engine.intent(Intent::Delete { id });
    h.advance(TOAST_SECONDS + 1);
    assert!(h.engine.main_view().toast.is_none());
    assert!(exported_files(&h).is_empty());
    assert_eq!(h.fakes.trash.trashed().len(), 3, "the .md, the .json and the stored copy went to the Trash, as with delete_transcript");
}

#[test]
fn windows_says_recycle_bin_and_explorer() {
    let h = Harness::build(PlatformWords::WINDOWS, true, FakeSpeech::ready());
    let id = finished_meeting(&h);
    assert_eq!(h.engine.main_view().show_in_file_manager, "Show in Explorer");
    h.engine.intent(Intent::Delete { id });
    assert!(h.engine.main_view().toast.unwrap().text.contains("moved to the Recycle Bin"));
}

#[test]
fn a_meeting_being_transcribed_cannot_be_deleted_or_renamed() {
    let h = Harness::ready();
    h.engine.intent(Intent::StartTranscribing);
    wait_for("transcribing", || h.transcribing());
    let id = h.engine.main_view().live.unwrap().id;
    h.engine.intent(Intent::Delete { id: id.clone() });
    h.engine.intent(Intent::Rename { id, title: "No".into() });
    let view = h.engine.main_view();
    assert!(view.toast.is_some_and(|t| !t.undo), "rename explains why not");
    assert_eq!(view.live.unwrap().title, "Meeting");
}

#[test]
fn show_in_finder_reveals_the_transcript_file() {
    let h = Harness::ready();
    let id = finished_meeting(&h);
    h.engine.intent(Intent::ShowInFileManager { id: Some(id) });
    let Some(HostCall::Reveal(path)) = h.host.calls().into_iter().rev().find(|c| matches!(c, HostCall::Reveal(_))) else {
        panic!("nothing revealed")
    };
    assert_eq!(path.extension().unwrap(), "md");
    h.engine.intent(Intent::ShowInFileManager { id: None });
    assert_eq!(h.host.calls().last(), Some(&HostCall::Reveal(h.engine.paths().transcripts)));
}

#[test]
fn an_audio_file_is_transcribed_in_the_list_and_never_moved() {
    let h = Harness::ready();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("Interview.wav");
    std::fs::copy(fixture("tone-stereo-48k.wav"), &file).unwrap();
    h.engine.intent(Intent::TranscribeFiles { paths: vec![file.clone(), dir.path().join("notes.txt")] });
    assert!(h.host.calls().contains(&HostCall::Show(Window::Main)));
    wait_for("imported", || h.engine.main_view().imports.is_empty());
    let view = h.engine.main_view();
    assert_eq!(view.meetings.len(), 1);
    assert_eq!(view.meetings[0].title, "Interview");
    assert_eq!(view.meetings[0].source, "Audio file");
    assert!(file.exists(), "files are never moved or deleted");
}

#[test]
fn files_wait_for_a_recording_and_can_be_cancelled() {
    let h = Harness::ready();
    h.engine.intent(Intent::StartTranscribing);
    wait_for("transcribing", || h.transcribing());
    let file = fixture("tone-mono-22k.flac");
    h.engine.intent(Intent::TranscribeFiles { paths: vec![file.clone()] });
    let row = h.engine.main_view().imports[0].clone();
    assert_eq!(row.status, "Waiting for the meeting to end");
    assert!(!row.running);
    h.engine.intent(Intent::CancelImport { id: row.id });
    assert!(h.engine.main_view().imports.is_empty());
    h.engine.intent(Intent::StopAndSave);
    wait_for("saved", || h.idle());
    assert!(file.exists());
}

#[test]
fn cancelling_a_running_file_stops_it_at_once_and_leaves_no_transcript() {
    let h = Harness::ready();
    h.speech.hold_files.store(true, std::sync::atomic::Ordering::SeqCst); // a slow model
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("long.wav");
    std::fs::write(&wav, long_wav(70)).unwrap();
    h.engine.intent(Intent::TranscribeFiles { paths: vec![wav.clone()] });
    wait_for("the slow model is working on it", || h.speech.held_chunks.load(std::sync::atomic::Ordering::SeqCst) > 0);
    let id = h.engine.main_view().imports[0].id;

    h.engine.intent(Intent::CancelImport { id });
    let row = h.engine.main_view().imports[0].clone();
    assert!(row.cancelling, "the row says so right away");
    assert_eq!(row.status, "Cancelling…");
    assert!(!row.running);

    wait_for("stopped", || h.engine.main_view().imports.is_empty());
    assert!(h.engine.main_view().meetings.is_empty(), "no partial meeting is left in the list");
    assert!(wav.exists(), "the audio file is never touched");
}

/// A mono 16 kHz WAV of loud noise, so the scripted model has something to hear.
fn long_wav(seconds: usize) -> Vec<u8> {
    let data: Vec<u8> = (0..seconds * 16_000).flat_map(|i| (if i % 2 == 0 { 8000i16 } else { -8000 }).to_le_bytes()).collect();
    let mut out = Vec::new();
    out.extend(b"RIFF");
    out.extend((36 + data.len() as u32).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(1u16.to_le_bytes());
    out.extend(16_000u32.to_le_bytes());
    out.extend(32_000u32.to_le_bytes());
    out.extend(2u16.to_le_bytes());
    out.extend(16u16.to_le_bytes());
    out.extend(b"data");
    out.extend((data.len() as u32).to_le_bytes());
    out.extend(data);
    out
}

#[test]
fn a_file_that_is_not_audio_fails_with_a_reason() {
    let h = Harness::ready();
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("broken.mp3");
    std::fs::write(&file, b"not audio at all").unwrap();
    h.engine.intent(Intent::TranscribeFiles { paths: vec![file] });
    wait_for("failed", || h.engine.main_view().imports.first().is_some_and(|i| i.failed));
    let id = h.engine.main_view().imports[0].id;
    h.engine.intent(Intent::DismissImport { id });
    assert!(h.engine.main_view().imports.is_empty());
}

#[test]
fn transcribe_audio_file_opens_the_picker_only_when_the_model_is_ready() {
    let h = Harness::build(PlatformWords::MAC, true, FakeSpeech::missing(10));
    h.engine.intent(Intent::Tray { id: "transcribe_file".into() });
    assert!(!h.host.calls().contains(&HostCall::PickAudioFiles));
    h.speech.finish();
    h.engine.intent(Intent::Tray { id: "transcribe_file".into() });
    assert!(h.host.calls().contains(&HostCall::PickAudioFiles));
}

#[test]
fn a_recent_transcript_in_the_menu_opens_the_main_window_on_it() {
    let h = Harness::ready();
    let id = finished_meeting(&h);
    h.engine.intent(Intent::Select { id: "nothing".into() });
    h.engine.intent(Intent::Tray { id: format!("recent:{id}") });
    assert_eq!(h.host.calls().last(), Some(&HostCall::Show(Window::Main)));
    assert_eq!(h.engine.main_view().reader.unwrap().id, id);
}

#[test]
fn quitting_saves_a_running_transcription() {
    let h = Harness::ready();
    h.engine.intent(Intent::StartTranscribing);
    wait_for("transcribing", || h.transcribing());
    h.engine.intent(Intent::Quit);
    assert_eq!(h.host.calls().last(), Some(&HostCall::Quit));
    assert_eq!(exported_files(&h).len(), 2);
}

/// A second start of the same Muesli (as after a quit), with a host of its own.
fn start_again(h: &Harness, open_main_window: bool) -> std::sync::Arc<RecordingHost> {
    let host = std::sync::Arc::new(RecordingHost::default());
    let engine = Engine::new(h.fakes.platform(), std::sync::Arc::new(FakeSpeech::ready()), host.clone(), Options::new(common::languages()));
    engine.start_with(open_main_window);
    host
}

#[test]
fn the_main_window_opens_at_start_when_a_person_started_muesli() {
    let h = Harness::ready();
    assert!(!h.host.is_open(Window::Main), "a start by itself (at login) stays in the menu bar");
    assert!(start_again(&h, true).is_open(Window::Main));
    assert!(!start_again(&h, false).is_open(Window::Main));
}

#[test]
fn setup_comes_first_on_a_first_start_even_when_a_person_started_muesli() {
    let h = Harness::first_launch();
    let host = start_again(&h, true);
    assert!(host.is_open(Window::Setup));
    assert!(!host.is_open(Window::Main), "the main window waits until setup is done");
}

#[test]
fn the_settings_shortcut_in_the_main_window_opens_settings() {
    let h = Harness::ready();
    h.engine.intent(Intent::OpenSettings);
    assert!(h.host.is_open(Window::Settings));
}
