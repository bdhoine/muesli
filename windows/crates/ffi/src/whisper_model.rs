use muesli_core::whisper_model::{self, ModelSwitch, WhisperModel};

/// Mirror of `muesli_core::whisper_model::WhisperModel` for Swift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum)]
pub enum ModelKind {
    Small,
    Medium,
    Large,
}

impl From<ModelKind> for WhisperModel {
    fn from(m: ModelKind) -> Self {
        match m {
            ModelKind::Small => Self::Small,
            ModelKind::Medium => Self::Medium,
            ModelKind::Large => Self::Large,
        }
    }
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
pub fn model_info(model: ModelKind) -> ModelInfo {
    let m = WhisperModel::from(model);
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
    active: ModelKind,
    wanted: ModelKind,
    installed: Vec<ModelKind>,
    downloading: Option<ModelKind>,
) -> ModelSwitchAction {
    let installed: Vec<WhisperModel> = installed.into_iter().map(Into::into).collect();
    match ModelSwitch::decide(active.into(), wanted.into(), &installed, downloading.map(Into::into)) {
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
        let info = model_info(ModelKind::Large);
        assert_eq!(info.file_name, "ggml-large-v3.bin");
        assert_eq!(info.minimum_valid_bytes, info.approximate_bytes / 10 * 9);
        assert_eq!(
            model_switch_decide(ModelKind::Small, ModelKind::Medium, vec![ModelKind::Medium], None),
            ModelSwitchAction::SwitchNow
        );
        assert_eq!(
            model_switch_decide(ModelKind::Small, ModelKind::Medium, vec![], Some(ModelKind::Medium)),
            ModelSwitchAction::Wait
        );
    }
}
