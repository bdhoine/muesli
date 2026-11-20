//! The end-of-meeting pass.
//!
//! Live blocks are short and each is transcribed alone, greedily, so the text comes fast. When
//! the meeting stops, the same speech is transcribed again: regrouped into chunks of up to 30 s
//! per speaker, decoded with beam search, with the vocabulary and the text just before as the
//! prompt. The result replaces the live text; when it is cancelled or fails, the live text stays.
//!
//! Only the blocks that were transcribed are kept (speech, the microphone already without the
//! call's echo), as 16-bit samples in memory. Audio is never written to disk.

use std::sync::atomic::{AtomicBool, Ordering};

use muesli_core::segmenter::SAMPLE_RATE;
use muesli_core::{AudioChunk, TranscriptSegment};
use muesli_import::SpeechToText;

/// The longest chunk of the pass: what Whisper sees at once.
pub const CHUNK_SECONDS: f64 = 30.0;
/// Blocks closer together than this are joined (with the silence between them).
pub const JOIN_GAP_SECONDS: f64 = 2.0;
/// How much of a speaker's previous text goes into the next chunk's prompt.
pub const CONTEXT_CHARS: usize = 200;

/// A live block, kept for the pass.
#[derive(Debug, Clone, PartialEq)]
pub struct Kept {
    pub speaker: String,
    pub offset_seconds: f64,
    pub samples: Vec<i16>,
}

impl Kept {
    pub fn of(chunk: &AudioChunk) -> Self {
        let samples = chunk.samples.iter().map(|s| (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).collect();
        Self { speaker: chunk.speaker.clone(), offset_seconds: chunk.offset_seconds, samples }
    }

    fn seconds(&self) -> f64 {
        self.samples.len() as f64 / SAMPLE_RATE as f64
    }
}

/// The kept blocks as the pass's chunks: per speaker (in order of first appearance), in time
/// order, joined while the gap is short and the chunk stays within [`CHUNK_SECONDS`].
pub fn regroup(kept: &[Kept]) -> Vec<AudioChunk> {
    let mut speakers: Vec<&str> = Vec::new();
    for k in kept {
        if !speakers.contains(&k.speaker.as_str()) {
            speakers.push(&k.speaker);
        }
    }
    let mut chunks = Vec::new();
    for speaker in speakers {
        let mut blocks: Vec<&Kept> = kept.iter().filter(|k| k.speaker == speaker).collect();
        blocks.sort_by(|a, b| a.offset_seconds.total_cmp(&b.offset_seconds));
        let mut current: Option<AudioChunk> = None;
        for block in blocks {
            let fits = current.as_ref().is_some_and(|c| {
                let end = c.offset_seconds + c.samples.len() as f64 / SAMPLE_RATE as f64;
                block.offset_seconds - end < JOIN_GAP_SECONDS && block.offset_seconds + block.seconds() - c.offset_seconds <= CHUNK_SECONDS
            });
            if !fits {
                chunks.extend(current.take());
                current = Some(AudioChunk {
                    speaker: speaker.to_string(),
                    offset_seconds: block.offset_seconds,
                    best: true,
                    ..Default::default()
                });
            }
            let chunk = current.as_mut().unwrap();
            let at = ((block.offset_seconds - chunk.offset_seconds) * SAMPLE_RATE as f64).round().max(0.0) as usize;
            if chunk.samples.len() < at {
                chunk.samples.resize(at, 0.0); // the pause between the two blocks
            }
            chunk.samples.extend(block.samples.iter().map(|s| *s as f32 / i16::MAX as f32));
        }
        chunks.extend(current);
    }
    chunks
}

/// The end of `text`, at most [`CONTEXT_CHARS`], starting at a word.
fn tail(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= CONTEXT_CHARS {
        return text.trim().to_string();
    }
    let end: String = chars[chars.len() - CONTEXT_CHARS..].iter().collect();
    end.split_once(' ').map_or(end.clone(), |(_, rest)| rest.to_string()).trim().to_string()
}

/// Transcribes `chunks` (from [`regroup`]) one by one. `prompt` is the meeting's (vocabulary,
/// title, attendees); each chunk adds the end of its speaker's previous text. `progress` gets
/// (done, total) after each chunk. `None` when cancelled or when a chunk fails: the live text
/// is then kept.
pub fn run(
    stt: &dyn SpeechToText,
    chunks: Vec<AudioChunk>,
    prompt: &str,
    cancel: &AtomicBool,
    mut progress: impl FnMut(usize, usize),
) -> Option<Vec<TranscriptSegment>> {
    let total = chunks.len();
    let mut out: Vec<TranscriptSegment> = Vec::new();
    let mut previous: Option<(String, String)> = None; // speaker, text
    for (done, mut chunk) in chunks.into_iter().enumerate() {
        if cancel.load(Ordering::SeqCst) {
            return None;
        }
        let context = previous.as_ref().filter(|(speaker, _)| *speaker == chunk.speaker).map(|(_, text)| tail(text));
        chunk.prompt =
            [prompt.to_string(), context.unwrap_or_default()].into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" ");
        let segments = stt.transcribe_until(&chunk, cancel).ok()?;
        if cancel.load(Ordering::SeqCst) {
            return None;
        }
        let text = segments.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");
        previous = Some((chunk.speaker.clone(), text));
        out.extend(segments);
        progress(done + 1, total);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kept(speaker: &str, at: f64, seconds: f64) -> Kept {
        Kept { speaker: speaker.into(), offset_seconds: at, samples: vec![1000; (seconds * SAMPLE_RATE as f64) as usize] }
    }

    fn shape(chunks: &[AudioChunk]) -> Vec<(String, f64, f64)> {
        chunks.iter().map(|c| (c.speaker.clone(), c.offset_seconds, c.samples.len() as f64 / SAMPLE_RATE as f64)).collect()
    }

    #[test]
    fn blocks_close_together_are_joined_up_to_thirty_seconds_per_speaker() {
        let blocks =
            [kept("You", 0.0, 8.0), kept("Others", 2.0, 5.0), kept("You", 9.0, 10.0), kept("You", 20.0, 9.0), kept("You", 30.0, 5.0)];
        let chunks = regroup(&blocks);
        assert_eq!(
            shape(&chunks),
            [
                ("You".to_string(), 0.0, 29.0), // 0-8, a 1 s pause, 9-19, a 1 s pause, 20-29
                ("You".to_string(), 30.0, 5.0), // would pass 30 s
                ("Others".to_string(), 2.0, 5.0),
            ]
        );
        assert!(chunks.iter().all(|c| c.best));
        assert_eq!(chunks[0].samples[8 * SAMPLE_RATE + 100], 0.0, "the pause is silence");
    }

    #[test]
    fn a_long_pause_starts_a_new_chunk() {
        let chunks = regroup(&[kept("You", 0.0, 3.0), kept("You", 10.0, 3.0)]);
        assert_eq!(shape(&chunks), [("You".to_string(), 0.0, 3.0), ("You".to_string(), 10.0, 3.0)]);
    }

    #[test]
    fn the_context_is_the_end_of_the_text_from_a_word_on() {
        let long = "word ".repeat(100);
        let t = tail(&long);
        assert!(t.len() <= CONTEXT_CHARS && t.starts_with("word"));
        assert_eq!(tail(" Short. "), "Short.");
    }

    struct Echo;
    impl SpeechToText for Echo {
        fn transcribe(&self, chunk: &AudioChunk) -> Result<Vec<TranscriptSegment>, String> {
            Ok(vec![TranscriptSegment {
                start: chunk.offset_seconds,
                end: chunk.offset_seconds + 1.0,
                speaker: chunk.speaker.clone(),
                text: format!("[{}]", chunk.prompt),
            }])
        }
    }

    #[test]
    fn each_chunk_gets_the_meeting_prompt_and_its_speakers_text_before() {
        let chunks = regroup(&[kept("You", 0.0, 3.0), kept("You", 10.0, 3.0), kept("Others", 1.0, 3.0)]);
        let mut seen = Vec::new();
        let out = run(&Echo, chunks, "Glossary: Muesli.", &AtomicBool::new(false), |d, t| seen.push((d, t))).unwrap();
        let prompts: Vec<&str> = out.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(prompts, ["[Glossary: Muesli.]", "[Glossary: Muesli. [Glossary: Muesli.]]", "[Glossary: Muesli.]"]);
        assert_eq!(seen, [(1, 3), (2, 3), (3, 3)]);
    }

    #[test]
    fn a_cancel_gives_nothing() {
        let chunks = regroup(&[kept("You", 0.0, 3.0)]);
        assert!(run(&Echo, chunks, "", &AtomicBool::new(true), |_, _| {}).is_none());
    }
}
