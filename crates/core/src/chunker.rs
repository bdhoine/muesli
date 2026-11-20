use std::collections::HashMap;
use std::sync::Mutex;

use crate::models::{AudioChunk, TranscriptSegment};

/// How a live transcription is cut where the speaker pauses (see `AudioChunker::live`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pauses {
    /// A pause shorter than this does not end a chunk.
    pub pause_seconds: f64,
    /// A chunk is never cut before it holds this much audio.
    pub min_seconds: f64,
    /// A chunk is cut at the latest here, at its quietest moment, pause or not.
    pub max_seconds: f64,
    /// Loudness (RMS, 0..1) below which a 20 ms frame counts as silence. Only decides where to
    /// cut: the speech model's voice detection still decides what is speech.
    pub silence_rms: f32,
}

impl Default for Pauses {
    fn default() -> Self {
        Self { pause_seconds: 0.8, min_seconds: 2.0, max_seconds: 10.0, silence_rms: 0.01 }
    }
}

/// Audio kept in front of speech and the shortest tail that is still sent (seconds).
const PRE_ROLL: f64 = 0.3;
const LAST_TAIL: f64 = 0.5;
/// How far back from the limit a forced cut looks for the quietest moment (seconds).
const CUT_SEARCH: f64 = 3.0;
const FRAME_SECONDS: f64 = 0.02;

/// Collects 16 kHz mono samples per speaker and emits chunks for Whisper: fixed-length ones
/// (`new`, imports) or cut at pauses in speech (`live`).
/// Thread-safe: the microphone and system-audio callbacks append concurrently.
pub struct AudioChunker {
    emit: Box<dyn Fn(AudioChunk) + Send + Sync>,
    chunk_seconds: f64,
    rate: f64,
    pauses: Option<Pauses>,
    inner: Mutex<Inner>,
}

struct Inner {
    buffers: HashMap<String, Vec<f32>>,
    emitted: HashMap<String, f64>,
    /// Pause mode: how far the buffer has been listened to, whether it holds speech, and how
    /// many samples of silence end it.
    heard: HashMap<String, Heard>,
}

#[derive(Default, Clone, Copy)]
struct Heard {
    frames_to: usize,
    speech: bool,
    silent_tail: usize,
}

impl AudioChunker {
    pub fn new(emit: impl Fn(AudioChunk) + Send + Sync + 'static) -> Self {
        Self::with_options(30.0, 16_000.0, &["You", "Others"], emit)
    }

    /// For a live transcription: a chunk ends when the speaker pauses, so text follows a sentence
    /// within moments, and at the latest after `Pauses::max_seconds`. Stretches without speech
    /// are not sent at all.
    pub fn live(emit: impl Fn(AudioChunk) + Send + Sync + 'static) -> Self {
        Self::with_pauses(Pauses::default(), 16_000.0, &["You", "Others"], emit)
    }

    pub fn with_pauses(pauses: Pauses, rate: f64, speakers: &[&str], emit: impl Fn(AudioChunk) + Send + Sync + 'static) -> Self {
        let mut chunker = Self::with_options(pauses.max_seconds, rate, speakers, emit);
        chunker.pauses = Some(pauses);
        chunker
    }

    pub fn with_options(chunk_seconds: f64, rate: f64, speakers: &[&str], emit: impl Fn(AudioChunk) + Send + Sync + 'static) -> Self {
        let buffers = speakers.iter().map(|s| (s.to_string(), Vec::new())).collect();
        let emitted = speakers.iter().map(|s| (s.to_string(), 0.0)).collect();
        Self {
            emit: Box::new(emit),
            chunk_seconds,
            rate,
            pauses: None,
            inner: Mutex::new(Inner { buffers, emitted, heard: HashMap::new() }),
        }
    }

    pub fn append(&self, speaker: &str, samples: &[f32]) {
        let mut inner = self.inner.lock().unwrap();
        inner.buffers.entry(speaker.to_string()).or_default().extend_from_slice(samples);
        if let Some(pauses) = self.pauses {
            self.flush_on_pauses(&mut inner, speaker, pauses, false);
        } else {
            self.flush(&mut inner, speaker, false);
        }
    }

    /// Emits what is left of each stream. Tails of one second or less are dropped.
    pub fn finish(&self) {
        let mut inner = self.inner.lock().unwrap();
        let mut speakers: Vec<String> = inner.buffers.keys().cloned().collect();
        speakers.sort();
        for speaker in speakers {
            if let Some(pauses) = self.pauses {
                self.flush_on_pauses(&mut inner, &speaker, pauses, true);
            } else {
                self.flush(&mut inner, &speaker, true);
            }
        }
    }

    /// Sends the first `n` samples of the speaker's buffer on and moves the buffer on.
    fn take(&self, inner: &mut Inner, speaker: &str, n: usize, send: bool) {
        let buf = inner.buffers.get_mut(speaker).unwrap();
        let chunk: Vec<f32> = buf.drain(..n).collect();
        let offset = inner.emitted.get(speaker).copied().unwrap_or(0.0);
        inner.emitted.insert(speaker.to_string(), offset + n as f64 / self.rate);
        inner.heard.insert(speaker.to_string(), Heard::default());
        if send {
            (self.emit)(AudioChunk { speaker: speaker.to_string(), offset_seconds: offset, samples: chunk });
        }
    }

    /// Pause mode: listens to the new audio in 20 ms frames, drops silence before speech, and
    /// sends a chunk when speech is followed by a pause, or when it gets too long.
    fn flush_on_pauses(&self, inner: &mut Inner, speaker: &str, p: Pauses, finishing: bool) {
        let frame = ((self.rate * FRAME_SECONDS) as usize).max(1);
        let samples = |seconds: f64| (seconds * self.rate) as usize;
        loop {
            let mut heard = inner.heard.get(speaker).copied().unwrap_or_default();
            let buf = inner.buffers.get(speaker).map_or(&[][..], Vec::as_slice);
            while heard.frames_to + frame <= buf.len() {
                let part = &buf[heard.frames_to..heard.frames_to + frame];
                if rms(part) >= p.silence_rms {
                    heard.speech = true;
                    heard.silent_tail = 0;
                } else {
                    heard.silent_tail += frame;
                }
                heard.frames_to += frame;
            }
            let len = buf.len();
            inner.heard.insert(speaker.to_string(), heard);

            if !heard.speech {
                // Nothing but silence so far: keep a little in front of the next word, drop the rest.
                let drop = len.saturating_sub(samples(PRE_ROLL));
                if drop > 0 {
                    self.take(inner, speaker, drop, false);
                }
                return;
            }
            if finishing {
                if len >= samples(LAST_TAIL) {
                    self.take(inner, speaker, len, true);
                }
                return;
            }
            if len >= samples(p.min_seconds) && heard.silent_tail >= samples(p.pause_seconds) {
                self.take(inner, speaker, len, true);
                return;
            }
            if len >= samples(p.max_seconds) {
                let cut = quietest_cut(buf, frame, samples(CUT_SEARCH));
                self.take(inner, speaker, cut, true);
                continue; // what is left is listened to again from its start
            }
            return;
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

/// Loudness of a stretch of samples.
fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

/// Where to cut a buffer that got too long: after the quietest frame within the last `search`
/// samples (the latest one when several are equally quiet), so a word is rarely split.
fn quietest_cut(buf: &[f32], frame: usize, search: usize) -> usize {
    let frames = buf.len() / frame;
    let first = frames.saturating_sub(search / frame);
    let mut best = (f32::MAX, frames);
    for i in first..frames {
        let level = rms(&buf[i * frame..(i + 1) * frame]);
        if level <= best.0 {
            best = (level, i + 1);
        }
    }
    (best.1 * frame).min(buf.len())
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
