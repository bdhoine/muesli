use chrono::{DateTime, Duration, TimeZone, Utc};
use muesli_core::{DetectedMeeting, DetectionEvent, DetectionLogic};

struct Harness {
    logic: DetectionLogic,
    t: DateTime<Utc>,
}

fn sync() -> (DetectedMeeting, String) {
    let mut m = DetectedMeeting::new("Weekly sync", "calendar");
    m.attendees = vec!["Ann".into(), "Bob".into()];
    (m, "ev1".into())
}

impl Harness {
    fn new() -> Self {
        Self { logic: DetectionLogic::default(), t: Utc.timestamp_opt(1_800_000_000, 0).unwrap() }
    }

    fn tick(
        &mut self,
        recording: bool,
        calendar: Option<(DetectedMeeting, String)>,
        mic: Option<bool>,
        teams: bool,
    ) -> Vec<DetectionEvent> {
        self.t += Duration::seconds(5);
        self.logic.tick(self.t, recording, calendar, mic, teams)
    }

    fn mic(&mut self, busy: bool) -> Vec<DetectionEvent> {
        self.tick(false, None, Some(busy), false)
    }

    fn rec(&mut self, busy: bool) -> Vec<DetectionEvent> {
        self.tick(true, None, Some(busy), false)
    }
}

fn suggestions(events: &[DetectionEvent]) -> Vec<DetectedMeeting> {
    events.iter().filter_map(|e| if let DetectionEvent::Suggest(m) = e { Some(m.clone()) } else { None }).collect()
}

#[test]
fn quiet_system_suggests_nothing() {
    let mut h = Harness::new();
    for _ in 0..10 {
        assert_eq!(h.mic(false), vec![]);
    }
    assert!(h.logic.in_progress.is_none());
}

#[test]
fn mic_must_be_busy_for_more_than_ten_seconds() {
    let mut h = Harness::new();
    assert_eq!(h.mic(true), vec![]); // 0 s
    assert_eq!(h.mic(true), vec![]); // 5 s
    assert_eq!(h.mic(true), vec![]); // 10 s: not yet "more than"
    let events = h.mic(true); // 15 s
    assert_eq!(suggestions(&events), vec![DetectedMeeting::new("Call", "audio")]);
}

#[test]
fn short_mic_use_is_ignored_and_the_clock_restarts() {
    let mut h = Harness::new();
    h.mic(true);
    h.mic(true);
    assert_eq!(h.mic(false), vec![]);
    h.mic(true);
    h.mic(true);
    assert_eq!(h.mic(true), vec![]);
    assert_eq!(suggestions(&h.mic(true)).len(), 1);
}

#[test]
fn teams_running_changes_the_label() {
    let mut h = Harness::new();
    for _ in 0..3 {
        h.tick(false, None, Some(true), true);
    }
    let events = h.tick(false, None, Some(true), true);
    assert_eq!(suggestions(&events), vec![DetectedMeeting::new("Teams call", "teams")]);
}

#[test]
fn a_call_is_suggested_once_however_long_it_lasts() {
    let mut h = Harness::new();
    let mut total = 0;
    for _ in 0..4 {
        total += suggestions(&h.mic(true)).len();
    }
    for _ in 0..720 {
        total += suggestions(&h.mic(true)).len(); // an hour, far beyond the ten minute cooldown
    }
    assert_eq!(total, 1);
}

#[test]
fn muting_for_a_moment_does_not_make_it_a_new_call() {
    let mut h = Harness::new();
    for _ in 0..4 {
        h.mic(true);
    }
    for _ in 0..5 {
        h.mic(false); // 25 s of silence: shorter than the end delay
    }
    let mut again = 0;
    for _ in 0..300 {
        again += suggestions(&h.mic(true)).len();
    }
    assert_eq!(again, 0);
}

#[test]
fn a_new_call_after_the_mic_was_free_is_suggested_again() {
    let mut h = Harness::new();
    for _ in 0..4 {
        h.mic(true);
    }
    for _ in 0..13 {
        h.mic(false); // more than the 60 s end delay
    }
    let mut again = 0;
    for _ in 0..140 {
        again += suggestions(&h.mic(true)).len(); // and beyond the ten minute cooldown
    }
    assert_eq!(again, 1);
}

#[test]
fn a_call_that_was_recorded_and_stopped_is_not_suggested_again() {
    let mut h = Harness::new();
    for _ in 0..4 {
        h.mic(true);
    }
    for _ in 0..10 {
        h.rec(true);
    }
    let mut again = 0;
    for _ in 0..300 {
        again += suggestions(&h.mic(true)).len(); // recording stopped, the call goes on
    }
    assert_eq!(again, 0);
}

#[test]
fn a_calendar_meeting_that_runs_over_is_not_suggested_again_as_a_call() {
    let mut h = Harness::new();
    let s = sync();
    let mut total = 0;
    for _ in 0..6 {
        total += suggestions(&h.tick(false, Some(s.clone()), Some(true), false)).len();
    }
    for _ in 0..300 {
        total += suggestions(&h.tick(false, None, Some(true), false)).len(); // the event is over, the call is not
    }
    assert_eq!(total, 1, "only the calendar suggestion");
}

#[test]
fn a_meeting_nobody_joined_is_suggested_once() {
    let mut h = Harness::new();
    let s = sync();
    let mut total = 0;
    for _ in 0..200 {
        total += suggestions(&h.tick(false, Some(s.clone()), Some(false), false)).len();
        // never joined, never dismissed
    }
    assert_eq!(total, 1);
}

#[test]
fn calendar_meeting_is_suggested_once_per_event() {
    let mut h = Harness::new();
    let s = sync();
    assert_eq!(suggestions(&h.tick(false, Some(s.clone()), Some(false), false)), vec![s.0.clone()]);
    for _ in 0..20 {
        assert!(suggestions(&h.tick(false, Some(s.clone()), Some(false), false)).is_empty());
    }
    let next = (DetectedMeeting::new("1:1", "calendar"), "ev2".to_string());
    assert_eq!(suggestions(&h.tick(false, Some(next.clone()), Some(false), false)), vec![next.0]);
}

#[test]
fn calendar_wins_over_audio_and_suppresses_the_audio_prompt() {
    let mut h = Harness::new();
    let s = sync();
    h.tick(false, Some(s.clone()), Some(true), false);
    let mut extra = 0;
    for _ in 0..10 {
        extra += suggestions(&h.tick(false, Some(s.clone()), Some(true), false)).len();
    }
    assert_eq!(extra, 0);
    assert_eq!(h.logic.in_progress.as_ref().unwrap().0.source, "calendar");
}

#[test]
fn nothing_is_suggested_while_recording() {
    let mut h = Harness::new();
    for _ in 0..10 {
        assert!(suggestions(&h.tick(true, Some(sync()), Some(true), false)).is_empty());
    }
}

#[test]
fn looks_like_meeting() {
    let l = DetectionLogic::looks_like_meeting;
    assert!(l("Join: https://Teams.Microsoft.com/l/meetup-join/abc", 0));
    assert!(l("https://meet.google.com/abc-defg-hij", 1));
    assert!(l("us02web.zoom.us/j/123", 0));
    assert!(l("Room 4.12", 3));
    assert!(!l("Dentist", 0));
    assert!(!l("Focus time", 1)); // only yourself
}

#[test]
fn in_progress_tracks_a_call_from_start_to_end() {
    let mut h = Harness::new();
    for _ in 0..3 {
        h.mic(true);
    }
    assert!(h.logic.in_progress.is_none());
    assert!(h.mic(true).contains(&DetectionEvent::InProgressChanged));
    let since = h.logic.in_progress.as_ref().unwrap().1;
    assert_eq!(h.logic.in_progress.as_ref().unwrap().0.source, "audio");

    assert!(!h.mic(true).contains(&DetectionEvent::InProgressChanged));
    assert_eq!(h.logic.in_progress.as_ref().unwrap().1, since);

    assert_eq!(h.mic(false), vec![DetectionEvent::InProgressChanged]);
    assert!(h.logic.in_progress.is_none());
}

#[test]
fn in_progress_is_reported_while_recording() {
    let mut h = Harness::new();
    assert!(h.tick(true, Some(sync()), Some(false), false).contains(&DetectionEvent::InProgressChanged));
    assert_eq!(h.logic.in_progress.as_ref().unwrap().0.title, "Weekly sync");
    assert_eq!(h.tick(true, None, Some(false), false), vec![DetectionEvent::InProgressChanged]);
    assert!(h.logic.in_progress.is_none());
}

#[test]
fn calendar_event_ending_falls_back_to_the_call_still_running() {
    let mut h = Harness::new();
    for _ in 0..4 {
        h.tick(false, Some(sync()), Some(true), false);
    }
    let events = h.tick(false, None, Some(true), false);
    assert!(events.contains(&DetectionEvent::InProgressChanged));
    assert_eq!(h.logic.in_progress.as_ref().unwrap().0.source, "audio");
}

#[test]
fn unknown_mic_state_keeps_the_last_answer() {
    let mut h = Harness::new();
    for _ in 0..4 {
        h.mic(true);
    }
    assert!(h.logic.in_progress.is_some());
    for _ in 0..5 {
        assert_eq!(h.tick(true, None, None, false), vec![]);
    }
    assert!(h.logic.in_progress.is_some());
}

#[test]
fn call_end_is_reported_once_after_the_delay() {
    let mut h = Harness::new();
    h.logic.end_delay = Duration::seconds(60);
    for _ in 0..4 {
        h.rec(true);
    }
    for _ in 0..12 {
        assert!(!h.rec(false).contains(&DetectionEvent::CallEnded));
    }
    assert!(h.rec(false).contains(&DetectionEvent::CallEnded));
    for _ in 0..20 {
        assert!(!h.rec(false).contains(&DetectionEvent::CallEnded));
    }
}

#[test]
fn brief_silence_does_not_end_the_call() {
    let mut h = Harness::new();
    h.logic.end_delay = Duration::seconds(60);
    h.rec(true);
    for _ in 0..10 {
        for _ in 0..8 {
            assert!(!h.rec(false).contains(&DetectionEvent::CallEnded));
        }
        h.rec(true);
    }
}

#[test]
fn call_end_can_fire_again_after_rejoining() {
    let mut h = Harness::new();
    h.logic.end_delay = Duration::seconds(10);
    h.rec(true);
    h.rec(false);
    h.rec(false);
    assert!(h.rec(false).contains(&DetectionEvent::CallEnded));
    h.rec(true);
    h.rec(false);
    h.rec(false);
    assert!(h.rec(false).contains(&DetectionEvent::CallEnded));
}

#[test]
fn manual_recording_without_a_call_never_ends() {
    let mut h = Harness::new();
    h.logic.end_delay = Duration::seconds(10);
    for _ in 0..50 {
        assert!(!h.rec(false).contains(&DetectionEvent::CallEnded));
    }
}

#[test]
fn end_tracking_resets_between_recordings() {
    let mut h = Harness::new();
    h.logic.end_delay = Duration::seconds(10);
    h.rec(true);
    h.mic(false); // stopped by hand
    for _ in 0..10 {
        assert!(!h.rec(false).contains(&DetectionEvent::CallEnded));
    }
}

#[test]
fn end_delay_is_configurable() {
    let mut h = Harness::new();
    h.logic.end_delay = Duration::seconds(20);
    h.rec(true);
    let mut ticks = 0;
    while !h.rec(false).contains(&DetectionEvent::CallEnded) {
        ticks += 1;
        assert!(ticks < 100);
    }
    assert_eq!(ticks, 4);
}

// One sample per tick, as a `MeetingDetector` port delivers it.
mod samples {
    use chrono::{Duration, TimeZone, Utc};
    use muesli_core::ports::DetectionSample;
    use muesli_core::{DetectedMeeting, DetectionEvent, DetectionLogic};

    #[test]
    fn a_call_on_teams_is_suggested_after_the_warmup() {
        let mut logic = DetectionLogic::default();
        let mut t = Utc.timestamp_opt(1_800_000_000, 0).unwrap();
        let mut suggested = None;
        for _ in 0..5 {
            t += Duration::seconds(5);
            let sample = DetectionSample { calendar: None, mic_busy: Some(true), teams_running: true };
            for e in logic.tick_sample(t, false, sample) {
                if let DetectionEvent::Suggest(m) = e {
                    suggested = Some(m);
                }
            }
        }
        assert_eq!(suggested, Some(DetectedMeeting::new("Teams call", "teams")));
    }

    #[test]
    fn calendar_meetings_are_passed_through() {
        let meeting = DetectedMeeting::new("Weekly sync", "calendar");
        let mut logic = DetectionLogic::default();
        let sample = DetectionSample { calendar: Some((meeting.clone(), "e1".into())), mic_busy: Some(false), teams_running: false };
        let events = logic.tick_sample(Utc.timestamp_opt(1_800_000_000, 0).unwrap(), false, sample);
        assert!(events.contains(&DetectionEvent::Suggest(meeting)));
    }
}
