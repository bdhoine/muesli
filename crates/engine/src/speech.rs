use serde::Serialize;

pub use muesli_core::{EchoCanceller, SpeechDetector};
pub use muesli_import::SpeechToText;

/// A loaded speech model: turns blocks of audio into text. Moved to the thread that uses it.
pub type Transcriber = Box<dyn SpeechToText + Send>;

/// Where the speech model (and the voice activity model) stands.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ModelStatus {
    /// Nothing downloaded yet and no download running.
    Missing,
    /// Bytes over everything that is being downloaded.
    Downloading {
        received: u64,
        total: u64,
    },
    Ready,
    /// The last download failed: why, and the address it tried.
    Failed {
        message: String,
        url: Option<String>,
    },
}

impl ModelStatus {
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }

    /// 0..1 while downloading.
    pub fn fraction(&self) -> Option<f64> {
        match self {
            Self::Downloading { received, total } if *total > 0 => Some((*received as f64 / *total as f64).min(1.0)),
            _ => None,
        }
    }
}

/// The speech model: whisper.cpp in the app (`crates/whisper`), a fake in tests and the simulator.
/// Downloading is one of Muesli's two kinds of network request; the other is the update check.
pub trait SpeechModel: Send + Sync {
    fn status(&self) -> ModelStatus;
    /// Starts downloading whatever is missing, in the background. Does nothing when all is there
    /// or a download runs. `on_change` is called on progress and at the end.
    fn download(&self, on_change: Box<dyn Fn() + Send + Sync>);
    /// Loads the model for these languages. Only called while `status()` is ready.
    fn transcriber(&self, languages: Vec<String>) -> Result<Transcriber, String>;
    /// A voice activity detector for one live stream (each speaker gets their own). Only called
    /// while `status()` is ready. Never fails: without the model, a loudness detector stands in.
    fn speech_detector(&self) -> Box<dyn SpeechDetector>;
    /// A canceller for the microphone's echo of the call (live only), or `None` when there is
    /// none (it could not load): the microphone is then transcribed as it is.
    fn echo_canceller(&self) -> Option<Box<dyn EchoCanceller>> {
        None
    }
    /// Loads the model once in the background, so the first transcription does not wait for
    /// one-time work (on a Mac, Core ML compiling the encoder). Called at start and when a
    /// download moves on; does nothing when there is nothing (new) to prepare.
    fn prepare(&self) {}
    /// What speeds the model up on this computer and how far it is, for Settings ("Neural Engine
    /// (Core ML): ready"); `None` when there is nothing to say.
    fn acceleration(&self) -> Option<String> {
        None
    }
}
