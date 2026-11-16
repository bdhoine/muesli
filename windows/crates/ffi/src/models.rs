//! Records and enums that cross the boundary unchanged. They mirror the `muesli-core` definitions
//! with `#[uniffi::remote]`, so there is no conversion code; the compiler checks the fields match.

use std::time::SystemTime;

use chrono::{DateTime, Utc};
use muesli_core::models::{AudioChunk, DetectedMeeting, Meeting, MeetingState, TranscriptSegment};

type UtcDateTime = DateTime<Utc>;

uniffi::custom_type!(UtcDateTime, SystemTime, {
    remote,
    lower: |d| d.into(),
    try_lift: |t| Ok(DateTime::<Utc>::from(t)),
});

#[uniffi::remote(Enum)]
pub enum MeetingState {
    Detected,
    Recording,
    Finalizing,
    Ready,
    Failed,
}

#[uniffi::remote(Record)]
pub struct TranscriptSegment {
    pub start: f64,
    pub end: f64,
    pub speaker: String,
    pub text: String,
}

#[uniffi::remote(Record)]
pub struct Meeting {
    pub id: String,
    pub title: String,
    pub source: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub attendees: Vec<String>,
    pub transcript: Vec<TranscriptSegment>,
    pub state: MeetingState,
    pub partial: Option<bool>,
}

#[uniffi::remote(Record)]
pub struct DetectedMeeting {
    pub title: String,
    pub source: String,
    pub attendees: Vec<String>,
}

/// Audio chunk (16 kHz mono f32) from one stream.
#[uniffi::remote(Record)]
pub struct AudioChunk {
    pub speaker: String,
    pub offset_seconds: f64,
    pub samples: Vec<f32>,
}

/// Maps one Whisper segment (times in milliseconds, relative to its chunk) onto the meeting
/// timeline. `None` for silence markers and empty text.
#[uniffi::export]
pub fn transcript_segment_from_whisper(
    speaker: String,
    chunk_offset_seconds: f64,
    start_ms: f64,
    end_ms: f64,
    text: String,
) -> Option<TranscriptSegment> {
    TranscriptSegment::from_whisper(&speaker, chunk_offset_seconds, start_ms, end_ms, &text)
}

/// A fresh meeting that is recording now, with a new id.
#[uniffi::export]
pub fn meeting_new(title: String, source: String) -> Meeting {
    Meeting::new(title, source)
}

/// The Markdown transcript the app writes next to the JSON.
#[uniffi::export]
pub fn meeting_markdown(meeting: Meeting) -> String {
    muesli_core::store::Store::markdown(&meeting)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_meeting_is_recording() {
        let m = meeting_new("Sync".into(), "manual".into());
        assert_eq!(m.state, MeetingState::Recording);
        assert!(meeting_markdown(m).contains("Sync"));
    }
}
