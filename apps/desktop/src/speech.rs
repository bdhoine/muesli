//! The engine's `SpeechModel`: the model download (`crates/whisper`, the only network request)
//! and whisper.cpp.

use muesli_core::whisper_model::{DEFAULT_MODEL, VAD_MODEL};
use muesli_core::Paths;
use muesli_engine::{ModelStatus, SpeechModel, Transcriber};
use muesli_whisper::{ModelManager, Phase};

pub struct WhisperSpeech {
    paths: Paths,
    manager: ModelManager,
}

impl WhisperSpeech {
    pub fn new(paths: Paths) -> Self {
        Self { manager: ModelManager::new(paths.clone()), paths }
    }
}

impl SpeechModel for WhisperSpeech {
    fn status(&self) -> ModelStatus {
        match self.manager.phase() {
            Phase::Missing => ModelStatus::Missing,
            Phase::Downloading { received, total } => ModelStatus::Downloading { received, total },
            Phase::Ready(_) => ModelStatus::Ready,
            Phase::Failed(message) => ModelStatus::Failed { message, url: self.manager.last_url() },
        }
    }

    fn download(&self, on_change: Box<dyn Fn() + Send + Sync>) {
        self.manager.ensure_ready(on_change);
    }

    fn transcriber(&self, languages: Vec<String>) -> Result<Transcriber, String> {
        let model = DEFAULT_MODEL.path(&self.paths);
        let vad = VAD_MODEL.path(&self.paths);
        muesli_whisper::Transcriber::new(&model, Some(&vad), languages)
            .map(|t| Box::new(t) as Transcriber)
            .ok_or_else(|| format!("The speech model could not be loaded: {}", model.display()))
    }
}
