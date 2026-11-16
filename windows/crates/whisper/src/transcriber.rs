use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use muesli_core::{languages, AudioChunk, TranscriptSegment};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

/// On-device transcription with whisper.cpp. Chunks are consumed one at a time, so a single
/// Whisper state is never used concurrently.
pub struct Transcriber {
    context: WhisperContext,
    languages: Vec<String>,
    state: Mutex<whisper_rs::WhisperState>,
}

#[derive(Debug)]
pub struct TranscribeError(pub String);

impl std::fmt::Display for TranscribeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Transcriber {
    /// `None` when the file is missing or Whisper cannot load it.
    pub fn new(model: &Path, languages: Vec<String>) -> Option<Self> {
        if !model.exists() {
            return None;
        }
        let context = WhisperContext::new_with_params(model.to_str()?, WhisperContextParameters::default()).ok()?;
        let state = context.create_state().ok()?;
        Some(Self { context, languages, state: Mutex::new(state) })
    }

    pub fn transcribe(&self, chunk: &AudioChunk) -> Result<Vec<TranscriptSegment>, TranscribeError> {
        if chunk.samples.is_empty() {
            return Ok(Vec::new());
        }
        let mut state = self.state.lock().unwrap();
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get().min(8)) as i32;
        let language = self.detect_language(&mut state, &chunk.samples, threads);

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(threads);
        params.set_language(Some(&language));
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        params.set_print_timestamps(false);
        state.full(params, &chunk.samples).map_err(|e| TranscribeError(e.to_string()))?;

        let mut out = Vec::new();
        for segment in state.as_iter() {
            let text = segment.to_str_lossy().map_err(|e| TranscribeError(e.to_string()))?;
            // whisper.cpp timestamps are in units of 10 ms.
            if let Some(s) = TranscriptSegment::from_whisper(
                &chunk.speaker,
                chunk.offset_seconds,
                segment.start_timestamp() as f64 * 10.0,
                segment.end_timestamp() as f64 * 10.0,
                &text,
            ) {
                out.push(s);
            }
        }
        Ok(out)
    }

    /// Picks the most probable of the allowed languages; one allowed language needs no detection.
    fn detect_language(&self, state: &mut whisper_rs::WhisperState, samples: &[f32], threads: i32) -> String {
        let fallback = self.languages.first().cloned().unwrap_or_else(|| "en".into());
        if self.languages.len() <= 1 {
            return fallback;
        }
        if state.pcm_to_mel(samples, threads as usize).is_err() {
            return fallback;
        }
        let Ok((_, probs)) = state.lang_detect(0, threads as usize) else { return fallback };
        let mut by_code: HashMap<String, f32> = HashMap::new();
        for code in &self.languages {
            if let Some(id) = whisper_rs::get_lang_id(code) {
                if let Some(p) = probs.get(id as usize) {
                    by_code.insert(code.clone(), *p);
                }
            }
        }
        languages::best(&by_code, &self.languages).unwrap_or(fallback)
    }

    pub fn context(&self) -> &WhisperContext {
        &self.context
    }
}
