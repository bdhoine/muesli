//! Transcribing an audio file: decode it (any supported format), cut it into 30 second chunks,
//! transcribe them one by one and store the result as a meeting (source `"file"`).
//!
//! Used by both apps, so a file gives the same transcript on macOS and Windows.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use chrono::{DateTime, Duration, Utc};
use muesli_audio::{decode_file, DecodeError};
use muesli_core::store::{MeetingStore, Store};
use muesli_core::{AudioChunk, Meeting, MeetingState, Paths, TranscriptSegment};

pub const SAMPLE_RATE: usize = 16_000;
pub const CHUNK_SECONDS: usize = 30;
/// The speaker label: a file has no "You" and "Others".
pub const SPEAKER: &str = "Speaker";
pub const SOURCE: &str = "file";

/// Anything that turns a chunk of 16 kHz mono audio into text (the whisper transcriber, or a stub in tests).
pub trait SpeechToText {
    fn transcribe(&self, chunk: &AudioChunk) -> Result<Vec<TranscriptSegment>, String>;
}

#[cfg(feature = "whisper")]
impl SpeechToText for muesli_whisper::Transcriber {
    fn transcribe(&self, chunk: &AudioChunk) -> Result<Vec<TranscriptSegment>, String> {
        muesli_whisper::Transcriber::transcribe(self, chunk).map_err(|e| e.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportError {
    Decode(DecodeError),
    Transcribe(String),
    Cancelled,
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Decode(e) => write!(f, "{e}"),
            Self::Transcribe(why) => write!(f, "Transcribing failed: {why}"),
            Self::Cancelled => write!(f, "Cancelled."),
        }
    }
}

/// The title of a meeting made from `path`: the file name without its extension.
pub fn title_for(path: &Path) -> String {
    path.file_stem().map(|s| s.to_string_lossy().into_owned()).filter(|s| !s.is_empty()).unwrap_or_else(|| "Audio file".into())
}

/// Transcribes `path` and stores it as a ready meeting. `progress` gets 0..1 (or `None` while the
/// file's length is unknown) after every chunk. A cancelled or failed import leaves a meeting
/// with what was transcribed so far, marked failed when nothing was.
pub fn import_file(
    path: &Path,
    stt: &dyn SpeechToText,
    paths: &Paths,
    cancel: &AtomicBool,
    mut progress: impl FnMut(Option<f64>),
) -> Result<Meeting, ImportError> {
    let store = MeetingStore::new(paths);
    let modified: DateTime<Utc> = std::fs::metadata(path).and_then(|m| m.modified()).unwrap_or_else(|_| SystemTime::now()).into();

    let title = title_for(path);
    // An import that was cut off (the app quit or crashed) left a partial meeting of this file:
    // carry on from where it stopped instead of starting a second meeting from the beginning.
    let (mut meeting, mut offset) = match take_over_stale(&store, paths, &title, modified) {
        Some(stale) => {
            let resume_at = resume_offset(&stale);
            (stale, resume_at)
        }
        None => (Meeting::new(&title, SOURCE), 0.0),
    };
    meeting.started_at = modified;
    meeting.state = MeetingState::Recording;
    meeting.partial = None;
    meeting.ended_at = None;
    store.save(&meeting); // visible as "recording" while it runs; a crash is recovered at the next start

    let skip_samples = (offset * SAMPLE_RATE as f64) as usize; // audio that is already transcribed
    let mut seen = 0usize;
    let mut pending: Vec<f32> = Vec::new();
    let mut failure: Option<ImportError> = None;

    let transcribe_chunk = |samples: Vec<f32>, meeting: &mut Meeting, offset: &mut f64| -> Result<(), ImportError> {
        let seconds = samples.len() as f64 / SAMPLE_RATE as f64;
        let chunk = AudioChunk { speaker: SPEAKER.to_string(), offset_seconds: *offset, samples };
        let segments = stt.transcribe(&chunk).map_err(ImportError::Transcribe)?;
        store.append(meeting, segments);
        *offset += seconds;
        Ok(())
    };

    let decoded = decode_file(path, cancel, |block, p| {
        if failure.is_some() {
            return;
        }
        let block = if seen >= skip_samples {
            block
        } else if seen + block.len() <= skip_samples {
            let before = seen / (CHUNK_SECONDS * SAMPLE_RATE);
            seen += block.len();
            if seen / (CHUNK_SECONDS * SAMPLE_RATE) != before {
                // once per skipped chunk, not per decoded block: a block is a few milliseconds of audio
                progress(p.total_seconds.filter(|t| *t > 0.0).map(|t| (seen as f64 / SAMPLE_RATE as f64 / t).min(1.0)));
            }
            return;
        } else {
            let from = skip_samples - seen;
            seen = skip_samples;
            &block[from..]
        };
        seen += block.len();
        pending.extend_from_slice(block);
        while pending.len() >= CHUNK_SECONDS * SAMPLE_RATE {
            let chunk: Vec<f32> = pending.drain(..CHUNK_SECONDS * SAMPLE_RATE).collect();
            if let Err(e) = transcribe_chunk(chunk, &mut meeting, &mut offset) {
                failure = Some(e);
                return;
            }
            progress(p.total_seconds.filter(|t| *t > 0.0).map(|t| (offset / t).min(1.0)));
        }
    });

    let outcome = match (decoded, failure) {
        (_, Some(e)) => Err(e),
        (Err(DecodeError::Cancelled), None) => Err(ImportError::Cancelled),
        (Err(e), None) => Err(ImportError::Decode(e)),
        (Ok(seconds), None) => {
            // The tail: anything of half a second or more is still speech worth transcribing.
            if pending.len() >= SAMPLE_RATE / 2 {
                let tail = std::mem::take(&mut pending);
                if let Err(e) = transcribe_chunk(tail, &mut meeting, &mut offset) {
                    return fail(&store, meeting, e);
                }
            }
            Ok(seconds)
        }
    };

    match outcome {
        Ok(seconds) => {
            progress(Some(1.0));
            // A resumed import may have exported its partial transcript under the old start time.
            let partial_stem = Store::file_stem(&meeting);
            meeting.started_at = modified - Duration::milliseconds((seconds * 1000.0) as i64);
            store.finish(&mut meeting, modified);
            let stem = Store::file_stem(&meeting);
            if stem != partial_stem {
                for ext in ["md", "json"] {
                    let _ = std::fs::remove_file(paths.transcripts_dir().join(format!("{partial_stem}.{ext}")));
                }
            }
            Ok(meeting)
        }
        Err(e) => fail(&store, meeting, e),
    }
}

/// Where a resumed import carries on: the first 30 s chunk after the one the last transcribed
/// segment belongs to. (A segment ending exactly on a chunk boundary belongs to the chunk before it.)
fn resume_offset(meeting: &Meeting) -> f64 {
    let Some(last_end) = meeting.transcript.iter().map(|s| s.end).reduce(f64::max) else { return 0.0 };
    let chunk = CHUNK_SECONDS as f64;
    (((last_end - 0.001).max(0.0) / chunk).floor() + 1.0) * chunk
}

/// Finds what an interrupted import of this file left: partial meetings (and empty failed ones)
/// with the same title that start at the file's date (to the second, the precision they are stored with), which only unfinished imports do.
/// Returns the one that got furthest and removes the others, so the file ends up as one meeting.
fn take_over_stale(store: &MeetingStore, paths: &Paths, title: &str, modified: DateTime<Utc>) -> Option<Meeting> {
    let mut stale: Vec<Meeting> = store
        .all()
        .into_iter()
        .filter(|m| m.source == SOURCE && m.title == title && m.started_at.timestamp() == modified.timestamp())
        .filter(|m| {
            m.partial == Some(true)
                || matches!(m.state, MeetingState::Recording | MeetingState::Finalizing)
                || (m.state == MeetingState::Failed && m.transcript.is_empty())
        })
        .collect();
    let progress = |m: &Meeting| m.transcript.iter().map(|s| s.end).fold(0.0, f64::max);
    stale.sort_by(|a, b| progress(a).total_cmp(&progress(b)));
    let best = stale.pop()?;
    let keep_stem = Store::file_stem(&best);
    for other in stale {
        if let Some(file) = store.url(&other.id) {
            let _ = std::fs::remove_file(file);
        }
        let stem = Store::file_stem(&other);
        if stem != keep_stem {
            for ext in ["md", "json"] {
                let _ = std::fs::remove_file(paths.transcripts_dir().join(format!("{stem}.{ext}")));
            }
        }
    }
    Some(best)
}

/// Keeps what was transcribed (a partial meeting). With nothing transcribed a cancelled import
/// leaves nothing behind, and a failed one is kept as a failed meeting so it can be seen.
fn fail(store: &MeetingStore, mut meeting: Meeting, error: ImportError) -> Result<Meeting, ImportError> {
    if meeting.transcript.is_empty() {
        if error == ImportError::Cancelled {
            if let Some(file) = store.url(&meeting.id) {
                let _ = std::fs::remove_file(file);
            }
        } else {
            meeting.state = MeetingState::Failed;
            store.save(&meeting);
        }
    } else {
        meeting.state = MeetingState::Ready;
        meeting.partial = Some(true);
        meeting.ended_at = Some(Utc::now());
        store.save(&meeting);
        Store::save(&meeting, store.paths.transcripts_dir());
    }
    Err(error)
}

/// True while `cancel` asks to stop (for callers that poll it between files).
pub fn cancelled(cancel: &AtomicBool) -> bool {
    cancel.load(Ordering::Relaxed)
}
