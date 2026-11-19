#![allow(dead_code)]

use chrono::{DateTime, Duration, TimeZone, Utc};
use muesli_core::{Meeting, MeetingState, Paths, TranscriptSegment};
use tempfile::TempDir;

/// Every test gets its own support and transcripts folders.
pub struct Env {
    pub dir: TempDir,
    pub paths: Paths,
}

impl Env {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::new(dir.path().join("support"), dir.path().join("transcripts"));
        Env { dir, paths }
    }
}

pub fn t0() -> DateTime<Utc> {
    Utc.timestamp_opt(1_800_000_000, 0).unwrap()
}

pub fn meeting(id: &str, state: MeetingState, segments: usize, title: &str, started: DateTime<Utc>) -> Meeting {
    Meeting {
        id: id.into(),
        title: title.into(),
        source: "calendar".into(),
        started_at: started,
        ended_at: (state == MeetingState::Ready).then(|| started + Duration::seconds(1800)),
        attendees: vec!["Ann".into()],
        transcript: (0..segments)
            .map(|i| TranscriptSegment {
                start: i as f64 * 5.0,
                end: i as f64 * 5.0 + 4.0,
                speaker: if i % 2 == 0 { "You" } else { "Others" }.into(),
                text: format!("Segment {i}"),
            })
            .collect(),
        state,
        partial: None,
    }
}

pub fn simple(id: &str, state: MeetingState) -> Meeting {
    meeting(id, state, 2, "Weekly sync", t0())
}
