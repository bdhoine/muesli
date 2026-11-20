//! Noticing a meeting, asking, transcribing, the end of the call and saving.

mod common;

use common::{wait_for, Harness};
use muesli_core::ports::{Click, Notice, Question, TrayItem};
use muesli_engine::fake::FakeSpeech;
use muesli_engine::{Intent, Window, ANSWER_TIMEOUT, CALL_END_DELAY};

fn start_question(title: &str, source: &str) -> Question {
    Question {
        title: format!("Looks like a meeting: {title}"),
        body: format!("Detected via {source}. Start transcribing?"),
        accept: "Start transcribing".into(),
        decline: "Not now".into(),
        click: Click::Accept,
    }
}

fn stop_question() -> Question {
    Question {
        title: "The call seems to have ended".into(),
        body: "Muesli is still transcribing. Stop and save the transcript?".into(),
        accept: "Stop and save".into(),
        decline: "Keep transcribing".into(),
        click: Click::Open,
    }
}

/// A Teams call that Muesli was asked to transcribe, and is transcribing.
fn transcribing_a_call() -> Harness {
    let h = Harness::ready();
    h.call_for(20, true);
    assert!(h.fakes.notifier.answer(true));
    wait_for("transcribing", || h.transcribing());
    h.advance(10); // the call goes on for a while
    h
}

#[test]
fn a_call_is_suggested_with_a_question_and_never_started_by_itself() {
    let h = Harness::ready();
    h.call_for(20, true);
    assert_eq!(h.fakes.notifier.questions(), vec![start_question("Teams call", "teams")]);
    h.advance(600);
    assert!(h.idle(), "no auto-start: only the answer starts");
    assert_eq!(h.fakes.notifier.questions().len(), 1, "suggested once per meeting");
}

#[test]
fn clicking_the_meeting_notification_itself_starts_transcribing() {
    let h = Harness::ready();
    h.call_for(20, true);
    assert!(h.fakes.notifier.click_question());
    wait_for("transcribing", || h.transcribing());
}

#[test]
fn clicking_the_end_of_call_notification_only_opens_muesli() {
    let h = transcribing_a_call();
    h.mic_free();
    h.advance(CALL_END_DELAY + 5);
    assert_eq!(h.fakes.notifier.questions().last(), Some(&stop_question()));
    assert!(h.fakes.notifier.click_question());
    assert!(h.host.calls().contains(&muesli_engine::fake::HostCall::Show(Window::Main)), "Muesli comes forward");
    assert!(h.transcribing(), "nothing was stopped");
    assert!(h.fakes.notifier.pending_question().is_some(), "the question is still open");
    // and it can still be answered, or runs out, as before
    assert!(h.fakes.notifier.answer(true));
    wait_for("saved", || h.idle());
}

#[test]
fn not_now_leaves_it_alone_for_the_rest_of_the_call() {
    let h = Harness::ready();
    h.call_for(20, false);
    assert!(h.fakes.notifier.answer(false));
    h.call_for(900, false);
    assert!(h.idle());
    assert_eq!(h.fakes.notifier.questions().len(), 1);
}

#[test]
fn a_calendar_meeting_is_suggested_by_its_title() {
    let h = Harness::ready();
    h.calendar("Weekly sync", "event-1");
    h.advance(6);
    assert_eq!(h.fakes.notifier.questions(), vec![start_question("Weekly sync", "calendar")]);
}

#[test]
fn starting_shows_the_meeting_in_the_menu_with_a_red_dot() {
    let h = transcribing_a_call();
    let menu = h.engine.menu();
    assert!(menu.transcribing, "red dot");
    assert!(h.menu_labels().contains(&"# Transcribing · Teams call".to_string()));
    assert!(h.menu_labels().contains(&"Stop and Save".to_string()));
    assert!(!h.menu_labels().iter().any(|l| l.contains("Start Transcribing")));
}

#[test]
fn the_live_transcript_grows_in_the_main_window_and_is_read_only() {
    let h = transcribing_a_call();
    h.talk(31.0);
    let both = |h: &Harness| {
        h.engine.main_view().reader.is_some_and(|r| {
            let speakers: Vec<_> = r.lines.iter().map(|l| l.speaker.clone()).collect();
            speakers.contains(&"You".to_string()) && speakers.contains(&"Others".to_string())
        })
    };
    wait_for("text from both", || both(&h));
    let view = h.engine.main_view();
    let live = view.live.expect("the live meeting is at the top");
    assert_eq!(live.title, "Teams call");
    let reader = view.reader.unwrap();
    assert!(reader.live && !reader.editable);
    assert_eq!(reader.badge.as_deref(), Some("Transcribing · live, read-only"));
    assert_eq!(reader.lines[0].time, "00:00");
    let speakers: Vec<_> = reader.lines.iter().map(|l| l.speaker.as_str()).collect();
    assert!(speakers.contains(&"You") && speakers.contains(&"Others"));
}

#[test]
fn text_appears_soon_after_a_pause_not_after_thirty_seconds() {
    let h = transcribing_a_call();
    h.fakes.mic.feed_seconds(3.0); // a sentence
    h.fakes.mic.feed_silence(1.5); // and a pause
    wait_for("the sentence", || h.engine.main_view().reader.is_some_and(|r| r.lines.len() == 1));
    let reader = h.engine.main_view().reader.unwrap();
    assert_eq!(reader.lines[0].speaker, "You");
    assert_eq!(reader.lines[0].time, "00:00");
}

#[test]
fn silence_is_never_transcribed() {
    let h = Harness::ready();
    let speech = h.speech.clone();
    h.engine.intent(Intent::StartTranscribing);
    wait_for("transcribing", || h.transcribing());
    for _ in 0..30 {
        h.fakes.mic.feed_silence(60.0);
        h.fakes.system_audio.feed_silence(60.0);
    }
    h.engine.intent(Intent::Tray { id: "stop".into() });
    wait_for("saved", || h.idle());
    assert_eq!(speech.transcribed.load(std::sync::atomic::Ordering::SeqCst), 0, "no block reached Whisper");
    let meeting = &h.engine.main_view().meetings[0];
    assert_eq!(muesli_core::store::MeetingStore::new(&h.engine.paths()).load(&meeting.id).unwrap().transcript.len(), 0);
}

/// The call from the speakers: what the system audio plays, and the microphone hearing it
/// 10 ms later, quieter. Saved and returns the speakers of the transcript.
fn call_through_the_speakers(echo_cancelling: bool) -> Vec<String> {
    let h = Harness::ready();
    h.speech.echo.store(echo_cancelling, std::sync::atomic::Ordering::SeqCst);
    h.engine.intent(Intent::StartTranscribing);
    wait_for("transcribing", || h.transcribing());
    // 4 s of the call, then 2 s of silence; the echo ends 10 ms after the call does.
    let call: Vec<f32> = (0..6 * 16_000).map(|i| if i < 4 * 16_000 { 0.1 * (i as f32 * 0.05).sin() } else { 0.0 }).collect();
    let heard: Vec<f32> = (0..call.len()).map(|i| if i >= 160 { 0.4 * call[i - 160] } else { 0.0 }).collect();
    h.fakes.system_audio.feed(&call);
    h.fakes.mic.feed(&heard);
    h.engine.intent(Intent::Tray { id: "stop".into() });
    wait_for("saved", || h.idle());
    let meeting = &h.engine.main_view().meetings[0];
    muesli_core::store::MeetingStore::new(&h.engine.paths()).load(&meeting.id).unwrap().transcript.into_iter().map(|s| s.speaker).collect()
}

#[test]
fn without_headphones_the_microphone_copy_of_the_call_is_removed_before_transcribing() {
    assert_eq!(call_through_the_speakers(true), ["Others"]);
}

#[test]
fn without_echo_cancelling_the_microphone_copy_of_the_call_would_be_you() {
    // The same call without a canceller: the microphone's copy comes in as "You" (the text filter
    // in `echo.rs` is what catches it then, once the words are known).
    assert!(call_through_the_speakers(false).contains(&"You".to_string()));
}

#[test]
fn stopping_sends_the_sentence_still_being_spoken() {
    let h = transcribing_a_call();
    h.fakes.system_audio.feed_seconds(4.0); // no pause yet
    h.engine.intent(Intent::Tray { id: "stop".into() });
    wait_for("saved", || h.idle());
    let meeting = &h.engine.main_view().meetings[0];
    let segments = muesli_core::store::MeetingStore::new(&h.engine.paths()).load(&meeting.id).unwrap().transcript;
    assert_eq!(segments.len(), 1);
    assert_eq!(segments[0].speaker, "Others");
}

#[test]
fn the_end_of_a_call_is_asked_after_sixty_seconds_and_keep_transcribing_keeps_going() {
    let h = transcribing_a_call();
    h.mic_free();
    h.advance(CALL_END_DELAY - 10);
    assert_eq!(h.fakes.notifier.questions().len(), 1, "not yet");
    h.advance(15);
    assert_eq!(h.fakes.notifier.questions().last(), Some(&stop_question()));
    assert!(h.fakes.notifier.answer(false));
    h.advance(ANSWER_TIMEOUT + 10);
    assert!(h.transcribing(), "Keep transcribing means keep going");
}

#[test]
fn stop_and_save_writes_the_transcript() {
    let h = transcribing_a_call();
    h.talk(31.0);
    h.mic_free();
    h.advance(CALL_END_DELAY + 5);
    assert!(h.fakes.notifier.answer(true));
    wait_for("saved", || h.idle());
    let view = h.engine.main_view();
    assert!(view.live.is_none());
    assert_eq!(view.meetings.len(), 1);
    let exported: Vec<_> = std::fs::read_dir(&h.engine.paths().transcripts).unwrap().flatten().collect();
    assert_eq!(exported.len(), 2, "a .md and a .json");
}

#[test]
fn no_answer_for_five_minutes_stops_saves_and_says_so() {
    let h = transcribing_a_call();
    h.talk(31.0);
    h.mic_free();
    h.advance(CALL_END_DELAY + 5);
    h.advance(ANSWER_TIMEOUT - 10);
    assert!(h.transcribing());
    h.advance(15);
    wait_for("saved", || h.idle());
    wait_for("notice", || !h.fakes.notifier.notices().is_empty());
    assert_eq!(
        h.fakes.notifier.notices(),
        vec![Notice {
            title: "Saved: Teams call".into(),
            body: "No answer for 5 minutes, so Muesli stopped and saved the transcript.".into(),
            action: Some("Open".into()),
        }]
    );
    // A late answer to the old question changes nothing.
    assert!(!h.fakes.notifier.answer(true) || h.idle());
    h.host.take_calls();
    assert!(h.fakes.notifier.click_action());
    assert!(h.host.calls().contains(&muesli_engine::fake::HostCall::Show(Window::Main)));
    let view = h.engine.main_view();
    assert!(view.meetings[0].selected);
}

#[test]
fn without_notifications_the_prompt_window_asks_the_same_question() {
    let h = Harness::ready();
    h.fakes.notifier.set_enabled(false);
    h.call_for(20, true);
    assert!(h.host.is_open(Window::Prompt));
    let prompt = h.engine.prompt_view().unwrap();
    assert_eq!(prompt.title, "Looks like a meeting: Teams call");
    assert_eq!((prompt.accept.as_str(), prompt.decline.as_str()), ("Start transcribing", "Not now"));
    h.engine.intent(Intent::AnswerPrompt { prompt: None, accept: true });
    assert!(!h.host.is_open(Window::Prompt));
    wait_for("transcribing", || h.transcribing());
    h.advance(10);

    h.mic_free();
    h.advance(CALL_END_DELAY + 5);
    assert!(h.host.is_open(Window::Prompt));
    assert_eq!(h.engine.prompt_view().unwrap().decline, "Keep transcribing");
    h.advance(ANSWER_TIMEOUT + 5);
    assert!(!h.host.is_open(Window::Prompt), "the window goes when Muesli stops by itself");
    wait_for("saved", || h.idle());
}

#[test]
fn a_late_answer_to_an_older_question_is_ignored() {
    let h = Harness::ready();
    h.engine.intent(Intent::AnswerPrompt { prompt: Some(999), accept: true });
    assert!(h.idle());
}

#[test]
fn nothing_is_suggested_or_started_before_the_model_is_ready() {
    let h = Harness::build(muesli_core::ports::PlatformWords::MAC, true, FakeSpeech::missing(1000));
    assert_eq!(h.speech.downloads(), 1, "the download starts by itself");
    h.speech.progress(420);
    assert_eq!(&h.menu_labels()[..3], ["# Getting Ready", "# Downloading the speech model · 42%", "(Start Transcribing)"]);
    assert!(h.menu_labels().contains(&"(Transcribe Audio File…)".to_string()));
    h.call_for(30, true);
    assert!(h.fakes.notifier.questions().is_empty());
    h.engine.intent(Intent::StartTranscribing);
    std::thread::sleep(std::time::Duration::from_millis(50));
    assert!(!h.transcribing());
    h.speech.finish();
    assert_eq!(h.menu_labels()[0], "Start Transcribing");
}

#[test]
fn starting_by_hand_from_the_menu() {
    let h = Harness::ready();
    h.engine.intent(Intent::Tray { id: "start".into() });
    wait_for("transcribing", || h.transcribing());
    assert!(h.menu_labels().contains(&"# Transcribing · Meeting".to_string()));
    h.engine.intent(Intent::Tray { id: "stop".into() });
    wait_for("saved", || h.idle());
    assert_eq!(h.engine.main_view().meetings[0].source, "Started by hand");
}

#[test]
fn a_microphone_that_cannot_start_is_reported_and_nothing_runs() {
    let h = Harness::ready();
    *h.fakes.mic.fail_with.lock().unwrap() = Some("No microphone was found.".into());
    h.engine.intent(Intent::StartTranscribing);
    wait_for("error", || h.engine.main_view().error.is_some());
    assert!(h.engine.main_view().error.unwrap().contains("No microphone was found."));
    assert!(h.menu_labels().contains(&"Dismiss".to_string()));
    h.engine.intent(Intent::DismissError);
    assert!(h.idle());
}

#[test]
fn a_broken_stream_saves_what_was_transcribed() {
    let h = transcribing_a_call();
    h.talk(31.0);
    wait_for("text", || h.engine.main_view().reader.is_some_and(|r| !r.lines.is_empty()));
    h.fakes.system_audio.break_stream("The device was unplugged.");
    wait_for("stopped", || h.engine.main_view().error.is_some());
    assert_eq!(h.engine.main_view().meetings.len(), 1);
}

#[test]
fn state_json_keeps_its_values() {
    let h = transcribing_a_call();
    let status = muesli_core::status::AppStatus::read(&h.engine.paths()).unwrap();
    assert_eq!(status.app_state, "recording");
    assert_eq!(status.current.unwrap().title, "Teams call");
    assert_eq!(status.updated_at, h.fakes.clock_now());
}

#[test]
fn the_three_latest_transcripts_are_in_the_menu() {
    let h = Harness::ready();
    for _ in 0..4 {
        h.engine.intent(Intent::StartTranscribing);
        wait_for("transcribing", || h.transcribing());
        h.advance(60);
        h.engine.intent(Intent::StopAndSave);
        wait_for("saved", || h.idle());
    }
    let recent = h.engine.menu().items.iter().filter(|i| matches!(i, TrayItem::Action { id, .. } if id.starts_with("recent:"))).count();
    assert_eq!(recent, 3);
}

trait ClockNow {
    fn clock_now(&self) -> chrono::DateTime<chrono::Utc>;
}

impl ClockNow for muesli_platform_fake::FakePlatform {
    fn clock_now(&self) -> chrono::DateTime<chrono::Utc> {
        use muesli_core::ports::Clock;
        self.clock.now()
    }
}
