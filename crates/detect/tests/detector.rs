use chrono::{Duration, TimeZone, Utc};
use muesli_core::{DetectedMeeting, DetectionEvent};
use muesli_detect::{is_teams_process, Detector, Sampler};

struct Fake {
    mic: Option<bool>,
    teams: bool,
    calendar: Option<(DetectedMeeting, String)>,
}

impl Sampler for Fake {
    fn mic_busy(&mut self) -> Option<bool> {
        self.mic
    }
    fn teams_running(&mut self) -> bool {
        self.teams
    }
    fn calendar_now(&mut self, _: chrono::DateTime<Utc>) -> Option<(DetectedMeeting, String)> {
        self.calendar.clone()
    }
}

#[test]
fn a_call_on_teams_is_suggested_after_the_warmup() {
    let mut d = Detector::new(Fake { mic: Some(true), teams: true, calendar: None });
    let mut t = Utc.timestamp_opt(1_800_000_000, 0).unwrap();
    let mut suggested = None;
    for _ in 0..5 {
        t += Duration::seconds(5);
        for e in d.tick(t, false) {
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
    let mut d = Detector::new(Fake { mic: Some(false), teams: false, calendar: Some((meeting.clone(), "e1".into())) });
    let events = d.tick(Utc.timestamp_opt(1_800_000_000, 0).unwrap(), false);
    assert!(events.contains(&DetectionEvent::Suggest(meeting)));
}

#[test]
fn teams_process_names() {
    for name in ["ms-teams.exe", "Teams.exe", "MSTeams", "Microsoft Teams"] {
        assert!(is_teams_process(name), "{name}");
    }
    assert!(!is_teams_process("zoom.us"));
    assert!(!is_teams_process("steamwebhelper"));
}

#[test]
fn the_real_system_can_be_sampled() {
    use muesli_detect::{Sampler, SystemSampler};
    let mut s = SystemSampler::new();
    let _ = s.mic_busy(); // may be None; must not crash
    let _ = s.teams_running();
}
