//! The end-of-meeting pass and the vocabulary: the meeting's speech is transcribed again after
//! Stop and replaces the live text; Whisper is prompted with the vocabulary and the meeting's names.

mod common;

use std::sync::atomic::Ordering;

use common::{wait_for, Harness};
use muesli_core::store::MeetingStore;
use muesli_core::{DetectedMeeting, Meeting, MeetingState, Vocabulary};
use muesli_engine::fake::IMPROVED;
use muesli_engine::Intent;

/// Starts by hand, both sides say something with a pause after it, and the live text is in.
fn a_short_meeting(h: &Harness) {
    h.engine.intent(Intent::StartTranscribing);
    wait_for("transcribing", || h.transcribing());
    h.talk(3.0);
    h.fakes.mic.feed_silence(2.0);
    h.fakes.system_audio.feed_silence(2.0);
    wait_for("the live text", || h.engine.main_view().reader.is_some_and(|r| r.lines.len() == 2));
}

fn saved(h: &Harness) -> Meeting {
    let id = h.engine.main_view().meetings[0].id.clone();
    MeetingStore::new(&h.engine.paths()).load(&id).unwrap()
}

fn texts(m: &Meeting) -> Vec<String> {
    m.transcript.iter().map(|s| s.text.clone()).collect()
}

#[test]
fn after_stop_the_end_pass_text_replaces_the_live_text() {
    let h = Harness::ready();
    a_short_meeting(&h);
    h.engine.intent(Intent::Tray { id: "stop".into() });
    wait_for("saved", || h.idle());
    let m = saved(&h);
    assert_eq!(m.state, MeetingState::Ready);
    assert_eq!(m.partial, None);
    assert_eq!(m.transcript.len(), 2);
    assert!(texts(&m).iter().all(|t| IMPROVED.contains(&t.as_str())), "{:?}", texts(&m));
    let exported =
        std::fs::read_dir(&h.engine.paths().transcripts).unwrap().flatten().find(|e| e.path().extension().is_some_and(|x| x == "md"));
    assert!(std::fs::read_to_string(exported.unwrap().path()).unwrap().contains("Improved:"), "the exported files have it too");
}

#[test]
fn whisper_is_prompted_with_the_vocabulary_and_the_meetings_names() {
    let h = Harness::ready();
    let mut vocabulary = Vocabulary::default();
    vocabulary.add(&["LocalVQE".to_string()]).unwrap();
    vocabulary.save(&h.engine.paths()).unwrap();
    let mut meeting = DetectedMeeting::new("Budget review", "calendar");
    meeting.attendees = vec!["Anna Peeters".into()];
    h.fakes.detector.set_calendar(Some((meeting, "event-1".into())));
    h.advance(6);
    assert!(h.fakes.notifier.answer(true));
    wait_for("transcribing", || h.transcribing());
    h.talk(3.0);
    h.fakes.mic.feed_silence(2.0);
    h.fakes.system_audio.feed_silence(2.0);
    wait_for("the live text", || h.engine.main_view().reader.is_some_and(|r| r.lines.len() == 2));
    h.engine.intent(Intent::Tray { id: "stop".into() });
    wait_for("saved", || h.idle());

    let prompts = h.speech.prompts.lock().unwrap().clone();
    let glossary = "Glossary: LocalVQE, Budget review, Anna Peeters.";
    let live: Vec<&String> = prompts.iter().filter(|(best, _)| !best).map(|(_, p)| p).collect();
    assert!(!live.is_empty() && live.iter().all(|p| *p == glossary), "{live:?}");
    // The end pass: the same, plus the speaker's text before (none here: one chunk each).
    let best: Vec<&String> = prompts.iter().filter(|(best, _)| *best).map(|(_, p)| p).collect();
    assert_eq!(best.len(), 2);
    assert!(best.iter().all(|p| p.starts_with(glossary)), "{best:?}");
}

#[test]
fn starting_again_stops_the_pass_and_keeps_the_live_text() {
    let h = Harness::ready();
    h.speech.hold_best.store(true, Ordering::SeqCst); // a slow computer
    a_short_meeting(&h);
    let live = h.engine.main_view().reader.unwrap().lines.iter().map(|l| l.text.clone()).collect::<Vec<_>>();
    h.engine.intent(Intent::Tray { id: "stop".into() });
    wait_for("improving", || h.menu_labels().iter().any(|l| l.starts_with("# Improving the transcript…")));
    assert_eq!(h.engine.app_status().app_state, "finalizing");
    let view = h.engine.main_view();
    assert!(view.can_start, "Start is offered while the transcript is improved");
    let reader = view.reader.unwrap();
    assert!(!reader.live && !reader.editable);
    assert!(reader.badge.unwrap().starts_with("Improving the transcript · "));
    assert!(view.live.unwrap().status.starts_with("Improving… "));

    h.engine.intent(Intent::StartTranscribing);
    wait_for("the next meeting", || h.transcribing());
    let first = MeetingStore::new(&h.engine.paths())
        .all()
        .into_iter()
        .find(|m| m.state == MeetingState::Ready)
        .expect("the first meeting is saved");
    assert_eq!(texts(&first), live, "with its live text");
    assert_eq!(first.partial, None);
}

#[test]
fn a_pass_that_fails_keeps_the_live_text() {
    let h = Harness::ready();
    h.speech.fail_best.store(true, Ordering::SeqCst);
    a_short_meeting(&h);
    h.engine.intent(Intent::Tray { id: "stop".into() });
    wait_for("saved", || h.idle());
    let m = saved(&h);
    assert_eq!(m.transcript.len(), 2);
    assert!(texts(&m).iter().all(|t| !t.starts_with("Improved:")));
}

#[test]
fn quitting_during_the_pass_saves_the_live_text() {
    let h = Harness::ready();
    h.speech.hold_best.store(true, Ordering::SeqCst);
    a_short_meeting(&h);
    h.engine.intent(Intent::Tray { id: "stop".into() });
    wait_for("improving", || {
        h.engine.app_status().app_state == "finalizing" && h.menu_labels().iter().any(|l| l.starts_with("# Improving"))
    });
    h.engine.shutdown();
    let all = MeetingStore::new(&h.engine.paths()).all();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].state, MeetingState::Ready);
    assert_eq!(all[0].transcript.len(), 2);
}

#[test]
fn a_silent_meeting_has_nothing_to_improve() {
    let h = Harness::ready();
    h.engine.intent(Intent::StartTranscribing);
    wait_for("transcribing", || h.transcribing());
    h.fakes.mic.feed_silence(10.0);
    h.fakes.system_audio.feed_silence(10.0);
    h.engine.intent(Intent::Tray { id: "stop".into() });
    wait_for("saved", || h.idle());
    assert!(h.speech.prompts.lock().unwrap().iter().all(|(best, _)| !best));
}
