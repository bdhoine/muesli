use std::collections::HashMap;

use muesli_core::languages;

#[uniffi::export]
pub fn languages_supported() -> Vec<String> {
    languages::SUPPORTED.iter().map(|c| c.to_string()).collect()
}

#[uniffi::export]
pub fn language_is_supported(code: String) -> bool {
    languages::is_supported(&code)
}

#[uniffi::export]
pub fn language_base_code(identifier: String) -> String {
    languages::base_code(&identifier)
}

/// The OS language (when Whisper supports it) plus English. The caller passes the system's
/// preferred languages, so the core never reads the locale itself on macOS.
#[uniffi::export]
pub fn languages_defaults(preferred: Vec<String>) -> Vec<String> {
    languages::defaults(&preferred)
}

#[uniffi::export]
pub fn languages_best(probabilities: HashMap<String, f32>, allowed: Vec<String>) -> Option<String> {
    languages::best(&probabilities, &allowed)
}

#[uniffi::export]
pub fn languages_unique(codes: Vec<String>) -> Vec<String> {
    languages::unique(codes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forwards_to_the_core() {
        assert!(language_is_supported("nl".into()));
        assert_eq!(language_base_code("nl-BE".into()), "nl");
        assert_eq!(languages_defaults(vec!["nl-BE".into()]), vec!["nl", "en"]);
        assert_eq!(languages_supported().len(), muesli_core::languages::SUPPORTED.len());
    }
}
