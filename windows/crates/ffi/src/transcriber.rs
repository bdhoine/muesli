//! On-device transcription for the macOS app: the same `muesli-whisper` the Windows app runs.

use std::path::Path;
use std::sync::Arc;

use muesli_core::models::{AudioChunk, TranscriptSegment};

/// Why a chunk could not be transcribed.
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum TranscribeFailure {
    #[error("{message}")]
    Failed { message: String },
}

/// One loaded Whisper model. Chunks are transcribed one at a time (an internal lock serialises
/// concurrent calls), and the call blocks, so call it off the main thread.
#[derive(uniffi::Object)]
pub struct Transcriber {
    inner: muesli_whisper::Transcriber,
}

#[uniffi::export]
impl Transcriber {
    /// 16 kHz mono samples in, segments on the meeting timeline out.
    pub fn transcribe(&self, chunk: AudioChunk) -> Result<Vec<TranscriptSegment>, TranscribeFailure> {
        self.inner.transcribe(&chunk).map_err(|e| TranscribeFailure::Failed { message: e.to_string() })
    }
}

/// `None` when a file is missing or Whisper cannot load it. `vad_path` is the voice activity model
/// that keeps Whisper from inventing text for silence. `languages` are the codes Whisper may pick
/// from; with more than one it detects the language of every chunk that contains speech.
#[uniffi::export]
pub fn transcriber_load(model_path: String, vad_path: String, languages: Vec<String>) -> Option<Arc<Transcriber>> {
    muesli_whisper::Transcriber::new(Path::new(&model_path), Some(Path::new(&vad_path)), languages)
        .map(|inner| Arc::new(Transcriber { inner }))
}
