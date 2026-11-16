use muesli_core::whisper_model::{self, ModelSwitch, WhisperModel};

#[uniffi::remote(Enum)]
pub enum WhisperModel {
    Small,
    Medium,
    Large,
}

/// What the UI shows and the downloader checks for one model.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record)]
pub struct ModelInfo {
    pub title: String,
    pub file_name: String,
    pub download_url: String,
    pub approximate_bytes: u64,
    pub minimum_valid_bytes: u64,
    pub summary: String,
}

#[uniffi::export]
pub fn model_info(m: WhisperModel) -> ModelInfo {
    ModelInfo {
        title: m.title().into(),
        file_name: m.file_name().into(),
        download_url: m.download_url(),
        approximate_bytes: m.approximate_bytes(),
        minimum_valid_bytes: m.minimum_valid_bytes(),
        summary: m.summary().into(),
    }
}

/// Mirror of `ModelSwitch`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ModelSwitchAction {
    Nothing,
    SwitchNow,
    Download,
    Wait,
}

#[uniffi::export]
pub fn model_switch_decide(
    active: WhisperModel,
    wanted: WhisperModel,
    installed: Vec<WhisperModel>,
    downloading: Option<WhisperModel>,
) -> ModelSwitchAction {
    match ModelSwitch::decide(active, wanted, &installed, downloading) {
        ModelSwitch::Nothing => ModelSwitchAction::Nothing,
        ModelSwitch::SwitchNow => ModelSwitchAction::SwitchNow,
        ModelSwitch::Download => ModelSwitchAction::Download,
        ModelSwitch::Wait => ModelSwitchAction::Wait,
    }
}

/// Whether `path` exists and holds at least `minimum` bytes (and at least one).
#[uniffi::export]
pub fn model_file_is_at_least(path: String, minimum: u64) -> bool {
    whisper_model::file_is_at_least(std::path::Path::new(&path), minimum)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn info_and_decision_match_the_core() {
        let info = model_info(WhisperModel::Large);
        assert_eq!(info.file_name, "ggml-large-v3.bin");
        assert_eq!(info.minimum_valid_bytes, info.approximate_bytes / 10 * 9);
        assert_eq!(
            model_switch_decide(WhisperModel::Small, WhisperModel::Medium, vec![WhisperModel::Medium], None),
            ModelSwitchAction::SwitchNow
        );
        assert_eq!(
            model_switch_decide(WhisperModel::Small, WhisperModel::Medium, vec![], Some(WhisperModel::Medium)),
            ModelSwitchAction::Wait
        );
    }
}
