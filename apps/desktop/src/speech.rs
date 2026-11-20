//! The engine's `SpeechModel`: the model download (`crates/whisper`; the update check is the other network request),
//! whisper.cpp, and LocalVQE for the microphone's echo of the call (bundled, `crates/aec`).

use muesli_core::whisper_model::{DEFAULT_MODEL, VAD_MODEL};
use std::path::PathBuf;

use muesli_core::Paths;
use muesli_core::{EchoCanceller, EnergyDetector, SpeechDetector};
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

    fn speech_detector(&self) -> Box<dyn SpeechDetector> {
        match muesli_whisper::SileroDetector::new(&VAD_MODEL.path(&self.paths)) {
            Some(silero) => Box::new(silero),
            None => Box::new(EnergyDetector::default()),
        }
    }

    /// LocalVQE from the app's resources; in development the library this build made and the
    /// model from `scripts/fetch-aec-model.py`. Missing or broken: none, the microphone goes as it is.
    fn echo_canceller(&self) -> Option<Box<dyn EchoCanceller>> {
        let find = |name: &str| echo_dirs().into_iter().map(|dir| dir.join(name)).find(|p| p.exists());
        let (library, model) = (find(muesli_aec::LIBRARY)?, find(muesli_aec::MODEL)?);
        match muesli_aec::LocalVqe::load(&library, &model) {
            Ok(canceller) => Some(Box::new(canceller)),
            Err(why) => {
                eprintln!("Echo cancellation is off: {why}");
                None
            }
        }
    }
}

/// Where the bundled echo canceller may be: `aec/` in the app's resources (macOS:
/// Contents/Resources, Windows: next to the executable), then what a development build has.
fn echo_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(exe_dir) = std::env::current_exe().ok().and_then(|exe| exe.parent().map(PathBuf::from)) {
        dirs.push(exe_dir.join("../Resources/aec"));
        dirs.push(exe_dir.join("aec"));
    }
    dirs.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/aec"));
    dirs.extend(muesli_aec::built_library_dir());
    dirs.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/aec/model"));
    dirs
}
