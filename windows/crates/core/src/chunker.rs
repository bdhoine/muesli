use std::collections::HashMap;
use std::sync::Mutex;

use crate::models::{AudioChunk, TranscriptSegment};

/// Collects 16 kHz mono samples per speaker and emits fixed-length chunks for Whisper.
/// Thread-safe: the microphone and system-audio callbacks append concurrently.
pub struct AudioChunker {
    emit: Box<dyn Fn(AudioChunk) + Send + Sync>,
    chunk_seconds: f64,
    rate: f64,
    inner: Mutex<Inner>,
}

struct Inner {
    buffers: HashMap<String, Vec<f32>>,
    emitted: HashMap<String, f64>,
}

impl AudioChunker {
    pub fn new(emit: impl Fn(AudioChunk) + Send + Sync + 'static) -> Self {
        Self::with_options(30.0, 16_000.0, &["You", "Others"], emit)
    }

    pub fn with_options(
        chunk_seconds: f64,
        rate: f64,
        speakers: &[&str],
        emit: impl Fn(AudioChunk) + Send + Sync + 'static,
    ) -> Self {
        let buffers = speakers.iter().map(|s| (s.to_string(), Vec::new())).collect();
        let emitted = speakers.iter().map(|s| (s.to_string(), 0.0)).collect();
        Self { emit: Box::new(emit), chunk_seconds, rate, inner: Mutex::new(Inner { buffers, emitted }) }
    }

    pub fn append(&self, speaker: &str, samples: &[f32]) {
        let mut inner = self.inner.lock().unwrap();
        inner.buffers.entry(speaker.to_string()).or_default().extend_from_slice(samples);
        self.flush(&mut inner, speaker, false);
    }

    /// Emits what is left of each stream. Tails of one second or less are dropped.
    pub fn finish(&self) {
        let mut inner = self.inner.lock().unwrap();
        let mut speakers: Vec<String> = inner.buffers.keys().cloned().collect();
        speakers.sort();
        for speaker in speakers {
            self.flush(&mut inner, &speaker, true);
        }
    }

    fn flush(&self, inner: &mut Inner, speaker: &str, force: bool) {
        let need = (self.chunk_seconds * self.rate) as usize;
        let rate = self.rate as usize;
        loop {
            let len = inner.buffers.get(speaker).map_or(0, Vec::len);
            if !(len >= need || (force && len > rate)) {
                break;
            }
            let n = need.min(len);
            let buf = inner.buffers.get_mut(speaker).unwrap();
            let chunk: Vec<f32> = buf.drain(..n).collect();
            let empty = buf.is_empty();
            let offset = inner.emitted.get(speaker).copied().unwrap_or(0.0);
            inner.emitted.insert(speaker.to_string(), offset + n as f64 / self.rate);
            (self.emit)(AudioChunk { speaker: speaker.to_string(), offset_seconds: offset, samples: chunk });
            if empty {
                break;
            }
        }
    }
}

impl TranscriptSegment {
    /// Maps one Whisper segment (times in milliseconds, relative to its chunk) onto the meeting
    /// timeline. Returns `None` for silence markers and empty text.
    pub fn from_whisper(speaker: &str, chunk_offset_seconds: f64, start_ms: f64, end_ms: f64, text: &str) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() || text == "[BLANK_AUDIO]" {
            return None;
        }
        Some(Self {
            start: chunk_offset_seconds + start_ms / 1000.0,
            end: chunk_offset_seconds + end_ms / 1000.0,
            speaker: speaker.to_string(),
            text: text.to_string(),
        })
    }
}
