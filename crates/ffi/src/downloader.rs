//! Downloads the speech model and the voice activity model, the same code the Windows app uses.

use std::sync::Arc;

use muesli_core::Preferences;
use muesli_whisper::{ModelManager, Phase};

use crate::paths::PathSet;

/// Where the files stand.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Enum)]
pub enum ModelPhase {
    /// Something has to be downloaded and nothing is running yet.
    Missing,
    /// Bytes over everything that is being downloaded.
    Downloading { received: u64, total: u64 },
    /// Everything is there; `path` is the speech model to load.
    Ready { path: String },
    Failed { message: String },
}

impl From<Phase> for ModelPhase {
    fn from(p: Phase) -> Self {
        match p {
            Phase::Missing => Self::Missing,
            Phase::Downloading { received, total } => Self::Downloading { received, total },
            Phase::Ready(path) => Self::Ready { path: path.to_string_lossy().into_owned() },
            Phase::Failed(message) => Self::Failed { message },
        }
    }
}

/// Told when the phase changes. Called from a download thread, not the main thread, so hop to
/// the main actor before touching the UI. Must not call back into the downloader.
#[uniffi::export(with_foreign)]
pub trait ModelObserver: Send + Sync {
    fn phase_changed(&self, phase: ModelPhase);
}

#[derive(uniffi::Object)]
pub struct ModelDownloader {
    manager: ModelManager,
    observer: Arc<dyn ModelObserver>,
}

#[uniffi::export]
impl ModelDownloader {
    #[uniffi::constructor]
    pub fn new(paths: PathSet, prefs: Preferences, observer: Arc<dyn ModelObserver>) -> Self {
        Self { manager: ModelManager::new(paths, prefs), observer }
    }

    pub fn phase(&self) -> ModelPhase {
        self.manager.phase().into()
    }

    /// The preferences changed (a custom model file was chosen or removed).
    pub fn apply(&self, prefs: Preferences) {
        self.manager.apply(prefs);
    }

    /// Downloads what is missing, unless a download is already running.
    pub fn ensure_ready(&self) {
        let (manager, observer) = (self.manager.clone(), self.observer.clone());
        self.manager.ensure_ready(move || observer.phase_changed(manager.phase().into()));
        self.observer.phase_changed(self.manager.phase().into());
    }

    pub fn cancel(&self) {
        self.manager.cancel();
        self.observer.phase_changed(self.manager.phase().into());
    }
}
