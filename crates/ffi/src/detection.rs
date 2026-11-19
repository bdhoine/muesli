use std::sync::Mutex;

use chrono::{DateTime, Duration, Utc};
use muesli_core::detection::{self, DetectionEvent, DetectionLogic};
use muesli_core::models::DetectedMeeting;

#[uniffi::remote(Enum)]
pub enum DetectionEvent {
    Suggest(DetectedMeeting),
    InProgressChanged,
    CallEnded,
}

/// The meeting or call going on right now.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct InProgress {
    pub meeting: DetectedMeeting,
    pub since: DateTime<Utc>,
}

/// A calendar meeting running now, with its event id.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct CalendarMeeting {
    pub meeting: DetectedMeeting,
    pub event_id: String,
}

fn seconds(d: Duration) -> f64 {
    d.num_milliseconds() as f64 / 1000.0
}

fn duration(seconds: f64) -> Duration {
    Duration::milliseconds((seconds * 1000.0) as i64)
}

/// The detection rules, with their state. Wraps `muesli_core::DetectionLogic`.
#[derive(uniffi::Object)]
pub struct Detector {
    logic: Mutex<DetectionLogic>,
}

#[uniffi::export]
impl Detector {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self { logic: Mutex::new(DetectionLogic::default()) }
    }

    pub fn mic_warmup(&self) -> f64 {
        seconds(self.logic.lock().unwrap().mic_warmup)
    }

    pub fn set_mic_warmup(&self, seconds: f64) {
        self.logic.lock().unwrap().mic_warmup = duration(seconds);
    }

    pub fn audio_prompt_cooldown(&self) -> f64 {
        seconds(self.logic.lock().unwrap().audio_prompt_cooldown)
    }

    pub fn set_audio_prompt_cooldown(&self, seconds: f64) {
        self.logic.lock().unwrap().audio_prompt_cooldown = duration(seconds);
    }

    pub fn end_delay(&self) -> f64 {
        seconds(self.logic.lock().unwrap().end_delay)
    }

    pub fn set_end_delay(&self, seconds: f64) {
        self.logic.lock().unwrap().end_delay = duration(seconds);
    }

    pub fn in_progress(&self) -> Option<InProgress> {
        self.logic.lock().unwrap().in_progress.clone().map(|(meeting, since)| InProgress { meeting, since })
    }

    /// `calendar`: the calendar meeting running now. `mic_busy`: whether an app other than Muesli
    /// uses a microphone, `None` when that cannot be told.
    pub fn tick(
        &self,
        now: DateTime<Utc>,
        recording: bool,
        calendar: Option<CalendarMeeting>,
        mic_busy: Option<bool>,
        teams_running: bool,
    ) -> Vec<DetectionEvent> {
        let calendar = calendar.map(|c| (c.meeting, c.event_id));
        self.logic.lock().unwrap().tick(now, recording, calendar, mic_busy, teams_running)
    }
}

impl Default for Detector {
    fn default() -> Self {
        Self::new()
    }
}

/// A calendar event counts as a meeting when it links to a known meeting service or has other
/// attendees.
#[uniffi::export]
pub fn detection_looks_like_meeting(text: String, attendee_count: u32) -> bool {
    DetectionLogic::looks_like_meeting(&text, attendee_count as usize)
}

#[uniffi::export]
pub fn detection_meeting_hosts() -> Vec<String> {
    detection::MEETING_HOSTS.iter().map(|h| h.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_calendar_meeting_is_suggested_once() {
        let detector = Detector::new();
        let now = Utc::now();
        let cal = || Some(CalendarMeeting { meeting: DetectedMeeting::new("Sync", "calendar"), event_id: "e1".into() });
        let first = detector.tick(now, false, cal(), None, false);
        assert!(first.contains(&DetectionEvent::Suggest(DetectedMeeting::new("Sync", "calendar"))));
        assert!(!detector.tick(now, false, cal(), None, false).iter().any(|e| matches!(e, DetectionEvent::Suggest(_))));
        assert_eq!(detector.in_progress().unwrap().meeting.title, "Sync");
        detector.set_end_delay(5.0);
        assert_eq!(detector.end_delay(), 5.0);
    }
}
