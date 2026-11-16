use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::json::{iso_date, iso_date_opt};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MeetingState {
    Detected,
    Recording,
    Finalizing,
    Ready,
    Failed,
}

impl std::str::FromStr for MeetingState {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, ()> {
        serde_json::from_value(serde_json::Value::String(s.into())).map_err(|_| ())
    }
}

impl MeetingState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Detected => "detected",
            Self::Recording => "recording",
            Self::Finalizing => "finalizing",
            Self::Ready => "ready",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptSegment {
    /// Seconds from the start of the meeting.
    pub start: f64,
    pub end: f64,
    /// "You" (microphone) or "Others" (system audio).
    pub speaker: String,
    pub text: String,
}

fn ready() -> MeetingState {
    MeetingState::Ready
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Meeting {
    pub id: String,
    pub title: String,
    /// calendar | teams | audio | manual
    pub source: String,
    #[serde(with = "iso_date")]
    pub started_at: DateTime<Utc>,
    #[serde(default, with = "iso_date_opt", skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub attendees: Vec<String>,
    #[serde(default)]
    pub transcript: Vec<TranscriptSegment>,
    /// Files written before states existed have none and count as ready.
    #[serde(default = "ready")]
    pub state: MeetingState,
    /// True when the transcript was cut short (recovered after a crash).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial: Option<bool>,
}

impl Meeting {
    /// True while recording or after an interruption: the transcript is not the whole meeting.
    pub fn is_partial(&self) -> bool {
        self.partial == Some(true) || matches!(self.state, MeetingState::Recording | MeetingState::Finalizing)
    }

    pub fn new(title: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            id: new_id(),
            title: title.into(),
            source: source.into(),
            started_at: Utc::now(),
            ended_at: None,
            attendees: Vec::new(),
            transcript: Vec::new(),
            state: MeetingState::Recording,
            partial: None,
        }
    }
}

/// A random id in the shape the Swift app uses (`UUID().uuidString`): letters, digits and dashes.
pub fn new_id() -> String {
    uuid::Uuid::new_v4().to_string().to_uppercase()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedMeeting {
    pub title: String,
    pub source: String,
    pub attendees: Vec<String>,
}

impl DetectedMeeting {
    pub fn new(title: impl Into<String>, source: impl Into<String>) -> Self {
        Self { title: title.into(), source: source.into(), attendees: Vec::new() }
    }
}

/// Audio chunk (16 kHz mono f32) from one stream.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioChunk {
    pub speaker: String,
    pub offset_seconds: f64,
    pub samples: Vec<f32>,
}
