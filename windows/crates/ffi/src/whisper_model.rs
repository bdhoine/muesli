use muesli_core::whisper_model::{self, DEFAULT_MODEL};

use crate::paths::PathSet;

/// What the UI shows about the one speech model every install uses.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ModelInfo {
    pub title: String,
    pub file_name: String,
    pub download_url: String,
    pub approximate_bytes: u64,
    pub summary: String,
}

#[uniffi::export]
pub fn model_info() -> ModelInfo {
    ModelInfo {
        title: DEFAULT_MODEL.title.into(),
        file_name: DEFAULT_MODEL.file_name.into(),
        download_url: DEFAULT_MODEL.url.into(),
        approximate_bytes: DEFAULT_MODEL.approximate_bytes,
        summary: DEFAULT_MODEL.summary.into(),
    }
}

/// Whether `path` exists and holds at least `minimum` bytes (and at least one).
#[uniffi::export]
pub fn model_file_is_at_least(path: String, minimum: u64) -> bool {
    whisper_model::file_is_at_least(std::path::Path::new(&path), minimum)
}

/// Old model files (earlier versions let the user pick Small, Medium or Large).
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct LegacyModels {
    pub files: u32,
    pub bytes: u64,
}

#[uniffi::export]
pub fn legacy_models(paths: PathSet) -> LegacyModels {
    let (files, bytes) = whisper_model::legacy_models(&paths);
    LegacyModels { files: files.len() as u32, bytes }
}

/// Deletes the old model files. Returns how many were removed.
#[uniffi::export]
pub fn legacy_models_remove(paths: PathSet) -> u32 {
    whisper_model::remove_legacy_models(&paths) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_fixed_model_and_legacy_files_are_found() {
        assert_eq!(model_info().file_name, "ggml-large-v3-turbo-q5_0.bin");
        let dir = tempfile::tempdir().unwrap();
        let paths = PathSet::new(dir.path(), dir.path().join("t"));
        assert_eq!(legacy_models(paths.clone()), LegacyModels { files: 0, bytes: 0 });
        std::fs::write(dir.path().join("ggml-small.bin"), [0u8; 9]).unwrap();
        assert_eq!(legacy_models(paths.clone()), LegacyModels { files: 1, bytes: 9 });
        assert_eq!(legacy_models_remove(paths), 1);
    }
}
