use chrono::{DateTime, Duration, Utc};

use crate::models::DetectedMeeting;

#[derive(Debug, Clone, PartialEq)]
pub enum DetectionEvent {
    /// A meeting just started and is not being recorded: suggest transcribing.
    Suggest(DetectedMeeting),
    /// `in_progress` changed.
    InProgressChanged,
    /// The call being recorded seems to be over.
    CallEnded,
}

pub const MEETING_HOSTS: [&str; 5] = ["teams.microsoft.com", "teams.live.com", "zoom.us", "meet.google.com", "webex.com"];

/// The decisions behind meeting detection, free of OS APIs so they can be tested.
/// A platform sampler reads the system every few seconds and feeds `tick`.
#[derive(Debug, Clone)]
pub struct DetectionLogic {
    /// How long other apps must have used the mic before it counts as a call.
    pub mic_warmup: Duration,
    /// Minimum time between two suggestions that come from mic activity alone.
    pub audio_prompt_cooldown: Duration,
    /// How long the mic must stay released before a recorded call counts as ended.
    pub end_delay: Duration,
    /// The meeting or call going on right now, whether or not it is recorded.
    pub in_progress: Option<(DetectedMeeting, DateTime<Utc>)>,

    seen_events: std::collections::HashSet<String>,
    mic_hot_since: Option<DateTime<Utc>>,
    last_audio_prompt: Option<DateTime<Utc>>,
    /// The call going on now was already suggested (or recorded, or is a calendar meeting that was
    /// suggested), so it is not suggested again. Cleared once the mic has been free for `end_delay`.
    call_handled: bool,
    mic_free_since: Option<DateTime<Utc>>,
    call_heard_while_recording: bool,
    mic_quiet_since: Option<DateTime<Utc>>,
    end_reported: bool,
}

impl Default for DetectionLogic {
    fn default() -> Self {
        Self {
            mic_warmup: Duration::seconds(10),
            audio_prompt_cooldown: Duration::seconds(600),
            end_delay: Duration::seconds(60),
            in_progress: None,
            seen_events: Default::default(),
            mic_hot_since: None,
            last_audio_prompt: None,
            call_handled: false,
            mic_free_since: None,
            call_heard_while_recording: false,
            mic_quiet_since: None,
            end_reported: false,
        }
    }
}

impl DetectionLogic {
    /// A calendar event counts as a meeting when it links to a known meeting service or has
    /// other attendees.
    pub fn looks_like_meeting(text: &str, attendee_count: usize) -> bool {
        let haystack = text.to_lowercase();
        MEETING_HOSTS.iter().any(|h| haystack.contains(h)) || attendee_count > 1
    }

    /// * `calendar`: the calendar meeting running now, if any, with its event id.
    /// * `mic_busy`: whether an app other than Muesli is using a microphone; `None` when that
    ///   cannot be told.
    pub fn tick(
        &mut self,
        now: DateTime<Utc>,
        recording: bool,
        calendar: Option<(DetectedMeeting, String)>,
        mic_busy: Option<bool>,
        teams_running: bool,
    ) -> Vec<DetectionEvent> {
        let mut events = Vec::new();

        match mic_busy {
            Some(true) => {
                self.mic_hot_since.get_or_insert(now);
            }
            Some(false) => self.mic_hot_since = None,
            None => {}
        }
        // The same call is handled once, however long it lasts and however often it is muted or
        // dismissed: only a mic that stays free for `end_delay` makes the next call a new one.
        match mic_busy {
            Some(true) => self.mic_free_since = None,
            Some(false) => {
                let free_since = *self.mic_free_since.get_or_insert(now);
                if now - free_since >= self.end_delay {
                    self.call_handled = false;
                }
            }
            None => {}
        }
        let call_active = self.mic_hot_since.is_some_and(|since| now - since > self.mic_warmup);
        let audio = DetectedMeeting::new(if teams_running { "Teams call" } else { "Call" }, if teams_running { "teams" } else { "audio" });

        // What is going on right now. The calendar gives the better title.
        let current: Option<DetectedMeeting> =
            calendar.as_ref().map(|(m, _)| m.clone()).or(if call_active { Some(audio.clone()) } else { None });
        if current.as_ref() != self.in_progress.as_ref().map(|(m, _)| m) {
            self.in_progress = current.map(|m| (m, now));
            events.push(DetectionEvent::InProgressChanged);
        }

        if (recording || calendar.is_some()) && call_active {
            self.call_handled = true; // recording it, or it is the calendar meeting that was suggested
        }
        if recording {
            // Only recordings in which another app used the mic can "end"; an in-person
            // meeting recorded manually never triggers this.
            if let Some(busy) = mic_busy {
                if busy {
                    self.call_heard_while_recording = true;
                    self.mic_quiet_since = None;
                    self.end_reported = false;
                } else if self.call_heard_while_recording {
                    let quiet_since = *self.mic_quiet_since.get_or_insert(now);
                    if !self.end_reported && now - quiet_since >= self.end_delay {
                        self.end_reported = true;
                        events.push(DetectionEvent::CallEnded);
                    }
                }
            }
            return events;
        }
        self.call_heard_while_recording = false;
        self.mic_quiet_since = None;
        self.end_reported = false;

        if let Some((meeting, event_id)) = calendar {
            if self.seen_events.insert(event_id) {
                events.push(DetectionEvent::Suggest(meeting));
            }
        } else if call_active && !self.call_handled && self.last_audio_prompt.is_none_or(|t| now - t > self.audio_prompt_cooldown) {
            self.last_audio_prompt = Some(now);
            self.call_handled = true;
            events.push(DetectionEvent::Suggest(audio));
        }
        events
    }
}
