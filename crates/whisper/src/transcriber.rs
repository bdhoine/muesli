use std::collections::HashMap;
use std::ffi::c_void;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use muesli_core::{languages, AudioChunk, TranscriptSegment};
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperVadContext, WhisperVadContextParams, WhisperVadParams,
};

/// On-device transcription with whisper.cpp. Chunks are consumed one at a time, so a single
/// Whisper state is never used concurrently.
pub struct Transcriber {
    context: WhisperContext,
    languages: Vec<String>,
    state: Mutex<whisper_rs::WhisperState>,
    vad: Option<Vad>,
    /// The language of the last chunk long enough to judge it by (see `JUDGE_SECONDS`).
    last_language: Mutex<Option<String>>,
}

/// Voice activity detection. Whisper invents text ("Thank you.", subtitle credits) for silence and
/// noise, so audio without speech is never decoded and silence inside a chunk is cut out.
struct Vad {
    model: String,
    context: Mutex<WhisperVadContext>,
}

/// Speech needs 250 ms, a pause needs half a second to split it, and every part of speech keeps
/// 200 ms around it so word edges survive.
fn vad_params() -> WhisperVadParams {
    let mut p = WhisperVadParams::new();
    p.set_threshold(0.5);
    p.set_min_speech_duration(250);
    p.set_min_silence_duration(500);
    p.set_speech_pad(200);
    p
}

const SAMPLE_RATE: usize = 16_000;

/// Live chunks end at pauses and can be a second or two long, too little to tell Dutch from
/// English by: speech shorter than this keeps the language of the last chunk that was long enough.
const JUDGE_SECONDS: usize = 3;

/// whisper.cpp's abort callback: `data` is the `AtomicBool` passed to `transcribe_until`.
unsafe extern "C" fn abort_requested(data: *mut c_void) -> bool {
    (*(data as *const AtomicBool)).load(Ordering::Relaxed)
}

#[derive(Debug)]
pub struct TranscribeError(pub String);

impl std::fmt::Display for TranscribeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Transcriber {
    /// `None` when a file is missing or Whisper cannot load it.
    ///
    /// `vad_model` is the Silero voice activity model. Pass `None` only to see what Whisper does
    /// without it (tests): real recordings need it.
    pub fn new(model: &Path, vad_model: Option<&Path>, languages: Vec<String>) -> Option<Self> {
        if !model.exists() {
            return None;
        }
        let vad = match vad_model {
            Some(path) => {
                let model = path.to_str()?.to_string();
                let context = WhisperVadContext::new(&model, WhisperVadContextParams::default()).ok()?;
                Some(Vad { model, context: Mutex::new(context) })
            }
            None => None,
        };
        let context = WhisperContext::new_with_params(model.to_str()?, WhisperContextParameters::default()).ok()?;
        let state = context.create_state().ok()?;
        Some(Self { context, languages, state: Mutex::new(state), vad, last_language: Mutex::new(None) })
    }

    /// The parts of `samples` that contain speech, or `None` when voice detection is off or fails
    /// (then everything is decoded, as without it).
    fn speech(&self, samples: &[f32]) -> Option<Vec<f32>> {
        let vad = self.vad.as_ref()?;
        let mut context = vad.context.lock().unwrap();
        let segments = context.segments_from_samples(vad_params(), samples).ok()?;
        let mut speech = Vec::new();
        for segment in segments {
            // Segment times are in centiseconds.
            let from = (segment.start * 0.01 * SAMPLE_RATE as f32) as usize;
            let to = ((segment.end * 0.01 * SAMPLE_RATE as f32) as usize).min(samples.len());
            if from < to {
                speech.extend_from_slice(&samples[from..to]);
            }
        }
        Some(speech)
    }

    pub fn transcribe(&self, chunk: &AudioChunk) -> Result<Vec<TranscriptSegment>, TranscribeError> {
        self.transcribe_until(chunk, &AtomicBool::new(false))
    }

    /// Like `transcribe`, but Whisper stops within moments (with an error) once `cancel` is set,
    /// instead of finishing the chunk.
    pub fn transcribe_until(&self, chunk: &AudioChunk, cancel: &AtomicBool) -> Result<Vec<TranscriptSegment>, TranscribeError> {
        if chunk.samples.is_empty() || cancel.load(Ordering::Relaxed) {
            return Ok(Vec::new());
        }
        // No speech: nothing to transcribe, and nothing for Whisper to make up.
        let speech = self.speech(&chunk.samples);
        if speech.as_ref().is_some_and(|s| s.is_empty()) {
            return Ok(Vec::new());
        }
        let mut state = self.state.lock().unwrap();
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get().min(8)) as i32;
        // The language is judged on the speech alone, not on the noise around it.
        let heard = speech.as_deref().unwrap_or(&chunk.samples);
        let last = self.last_language.lock().unwrap().clone(); // not held across the match below
        let language = match last {
            Some(last) if heard.len() < JUDGE_SECONDS * SAMPLE_RATE && self.languages.contains(&last) => last,
            _ => {
                let found = self.detect_language(&mut state, heard, threads);
                if heard.len() >= JUDGE_SECONDS * SAMPLE_RATE || self.languages.len() <= 1 {
                    *self.last_language.lock().unwrap() = Some(found.clone());
                }
                found
            }
        };

        let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
        params.set_n_threads(threads);
        params.set_language(Some(&language));
        // Each chunk stands alone: carrying text over invites repetition loops.
        params.set_no_context(true);
        params.set_suppress_blank(true);
        params.set_suppress_nst(true);
        params.set_no_speech_thold(0.6);
        if let Some(vad) = &self.vad {
            // Cuts silence inside the chunk; whisper.cpp maps the timestamps back to the chunk.
            params.set_vad_model_path(Some(&vad.model));
            params.enable_vad(true);
            params.set_vad_params(vad_params());
        }
        // whisper.cpp asks between steps of its computation whether to stop. `cancel` outlives
        // `state.full` below, which is the only place the callback is called from.
        unsafe {
            params.set_abort_callback(Some(abort_requested));
            params.set_abort_callback_user_data(cancel as *const AtomicBool as *mut c_void);
        }
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
