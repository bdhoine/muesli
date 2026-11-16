use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::paths::Paths;

/// The Whisper models Muesli can download. All come from whisper.cpp's ggml releases.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum WhisperModel {
    #[default]
    Small,
    Medium,
    Large,
}

impl WhisperModel {
    pub const ALL: [WhisperModel; 3] = [Self::Small, Self::Medium, Self::Large];

    pub fn title(self) -> &'static str {
        match self {
            Self::Small => "Small",
            Self::Medium => "Medium",
            Self::Large => "Large",
        }
    }

    pub fn file_name(self) -> &'static str {
        match self {
            Self::Small => "ggml-small.bin",
            Self::Medium => "ggml-medium.bin",
            Self::Large => "ggml-large-v3.bin",
        }
    }

    pub fn download_url(self) -> String {
        format!("https://huggingface.co/ggerganov/whisper.cpp/resolve/main/{}", self.file_name())
    }

    /// Size of the published file.
    pub fn approximate_bytes(self) -> u64 {
        match self {
            Self::Small => 487_601_967,
            Self::Medium => 1_533_763_059,
            Self::Large => 3_095_033_483,
        }
    }

    /// Downloads smaller than 90% of the published size are rejected.
    pub fn minimum_valid_bytes(self) -> u64 {
        self.approximate_bytes() / 10 * 9
    }

    pub fn summary(self) -> &'static str {
        match self {
            Self::Small => "Good balance of speed and accuracy",
            Self::Medium => "More accurate, slower",
            Self::Large => "Most accurate, slowest, needs plenty of memory",
        }
    }

    pub fn path(self, paths: &Paths) -> PathBuf {
        paths.models.join(self.file_name())
    }

    /// On disk and large enough to be the whole file.
    pub fn is_installed(self, paths: &Paths) -> bool {
        file_is_at_least(&self.path(paths), self.minimum_valid_bytes())
    }

    /// Parses the value stored in `config.json`; anything unknown falls back to `Small`.
    pub fn from_config(value: &str) -> Self {
        serde_json::from_value(serde_json::Value::String(value.into())).unwrap_or_default()
    }
}

/// What to do when the user picks a model in Settings. The active model keeps working until the
/// new one is on disk, so a pick never leaves Muesli without a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelSwitch {
    /// Nothing to do: it is the active model already.
    Nothing,
    /// The file is there: make it the active model now.
    SwitchNow,
    /// Not on disk: download it, then switch.
    Download,
    /// Its download is already running: switch when it finishes.
    Wait,
}

impl ModelSwitch {
    pub fn decide(active: WhisperModel, wanted: WhisperModel, installed: &[WhisperModel], downloading: Option<WhisperModel>) -> Self {
        if wanted == active {
            Self::Nothing
        } else if installed.contains(&wanted) {
            Self::SwitchNow
        } else if downloading == Some(wanted) {
            Self::Wait
        } else {
            Self::Download
        }
    }
}

/// Downloaded models other than the active one, and the space removing them frees.
pub fn unused_models(active: WhisperModel, paths: &Paths) -> (Vec<WhisperModel>, u64) {
    let models: Vec<WhisperModel> =
        WhisperModel::ALL.into_iter().filter(|m| *m != active && m.is_installed(paths)).collect();
    let bytes = models.iter().map(|m| std::fs::metadata(m.path(paths)).map(|d| d.len()).unwrap_or(0)).sum();
    (models, bytes)
}

/// Whether `path` exists and holds at least `minimum` bytes (and at least one).
pub fn file_is_at_least(path: &std::path::Path, minimum: u64) -> bool {
    std::fs::metadata(path).map(|m| m.len() >= minimum.max(1)).unwrap_or(false)
}
