use std::path::PathBuf;

use crate::paths::Paths;

/// A file Muesli downloads once: where it comes from, how big it should be, where it is kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModelSpec {
    pub title: &'static str,
    pub summary: &'static str,
    pub file_name: &'static str,
    pub url: &'static str,
    /// Size of the published file.
    pub approximate_bytes: u64,
}

/// The speech model every install uses: Whisper large-v3-turbo, quantised to 5 bits (574 MB).
/// Another model is only possible through `Preferences::custom_model_path`.
pub const DEFAULT_MODEL: ModelSpec = ModelSpec {
    title: "Large v3 Turbo",
    summary: "Accurate and fast, runs on your computer",
    file_name: "ggml-large-v3-turbo-q5_0.bin",
    url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
    approximate_bytes: 574_041_195,
};

/// The Silero voice activity model. Whisper invents text for silence and noise; this tells it
/// where people actually speak. Downloaded together with the speech model.
pub const VAD_MODEL: ModelSpec = ModelSpec {
    title: "Voice activity model",
    summary: "Finds the parts of the audio that contain speech",
    file_name: "ggml-silero-v5.1.2.bin",
    url: "https://huggingface.co/ggml-org/whisper-vad/resolve/main/ggml-silero-v5.1.2.bin",
    approximate_bytes: 885_098,
};

/// Models earlier versions let the user pick. Nothing uses them any more.
pub const LEGACY_FILES: [&str; 3] = ["ggml-small.bin", "ggml-medium.bin", "ggml-large-v3.bin"];

impl ModelSpec {
    /// Downloads smaller than 90% of the published size are rejected.
    pub fn minimum_valid_bytes(&self) -> u64 {
        self.approximate_bytes / 10 * 9
    }

    pub fn path(&self, paths: &Paths) -> PathBuf {
        paths.models.join(self.file_name)
    }

    /// On disk and large enough to be the whole file.
    pub fn is_installed(&self, paths: &Paths) -> bool {
        file_is_at_least(&self.path(paths), self.minimum_valid_bytes())
    }
}

/// Old model files still on disk, and the space removing them frees.
pub fn legacy_models(paths: &Paths) -> (Vec<PathBuf>, u64) {
    let files: Vec<PathBuf> = LEGACY_FILES.iter().map(|f| paths.models.join(f)).filter(|p| p.is_file()).collect();
    let bytes = files.iter().map(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)).sum();
    (files, bytes)
}

/// Deletes the old model files. Returns how many were removed.
pub fn remove_legacy_models(paths: &Paths) -> usize {
    legacy_models(paths).0.iter().filter(|p| std::fs::remove_file(p).is_ok()).count()
}

/// Whether `path` exists and holds at least `minimum` bytes (and at least one).
pub fn file_is_at_least(path: &std::path::Path, minimum: u64) -> bool {
    std::fs::metadata(path).map(|m| m.len() >= minimum.max(1)).unwrap_or(false)
}
