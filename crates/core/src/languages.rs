//! Whisper's supported languages and the defaults derived from the system language.

pub const SUPPORTED: &[&str] = &[
    "en", "zh", "de", "es", "ru", "ko", "fr", "ja", "pt", "tr", "pl", "ca", "nl", "ar", "sv", "it", "id", "hi", "fi", "vi", "he", "uk",
    "el", "ms", "cs", "ro", "da", "hu", "ta", "no", "th", "ur", "hr", "bg", "lt", "la", "mi", "ml", "cy", "sk", "te", "fa", "lv", "bn",
    "sr", "az", "sl", "kn", "et", "mk", "br", "eu", "is", "hy", "ne", "mn", "bs", "kk", "sq", "sw", "gl", "mr", "pa", "si", "km", "sn",
    "yo", "so", "af", "oc", "ka", "be", "tg", "sd", "gu", "am", "yi", "lo", "uz", "fo", "ht", "ps", "tk", "nn", "mt", "sa", "lb", "my",
    "bo", "tl", "mg", "as", "tt", "haw", "ln", "ha", "ba", "jw", "su",
];

pub fn is_supported(code: &str) -> bool {
    SUPPORTED.contains(&code)
}

/// `nl-BE`, `nl_BE` and `NL` all become `nl`. The old code `iw` becomes `he`.
pub fn base_code(identifier: &str) -> String {
    let base = identifier.to_lowercase().split(['-', '_']).next().unwrap_or("").to_string();
    if base == "iw" {
        "he".into()
    } else {
        base
    }
}

/// The OS language (when Whisper supports it) plus English.
pub fn defaults(preferred: &[String]) -> Vec<String> {
    let os = preferred.first().map(|p| base_code(p)).filter(|c| is_supported(c));
    unique(os.into_iter().chain(std::iter::once("en".to_string())).collect())
}

/// `defaults` for this machine's preferred languages.
pub fn system_defaults() -> Vec<String> {
    let preferred: Vec<String> = sys_locale::get_locales().collect();
    defaults(&preferred)
}

/// The most probable of the allowed languages.
pub fn best(probabilities: &std::collections::HashMap<String, f32>, allowed: &[String]) -> Option<String> {
    let score = |c: &String| probabilities.get(c).copied().unwrap_or(-1.0);
    let mut best: Option<&String> = None;
    for code in allowed {
        // Like Swift's `max(by:)`: on a tie the later entry wins.
        if best.is_none_or(|b| score(b) <= score(code)) {
            best = Some(code);
        }
    }
    best.cloned()
}

pub fn unique(codes: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    codes.into_iter().filter(|c| seen.insert(c.clone())).collect()
}

/// English name of a language code, for places without a localisation library. The UI can
/// localise names itself (e.g. `Intl.DisplayNames`).
pub fn english_name(code: &str) -> &str {
    match code {
        "en" => "English",
        "zh" => "Chinese",
        "de" => "German",
        "es" => "Spanish",
        "ru" => "Russian",
        "ko" => "Korean",
        "fr" => "French",
        "ja" => "Japanese",
        "pt" => "Portuguese",
        "tr" => "Turkish",
        "pl" => "Polish",
        "ca" => "Catalan",
        "nl" => "Dutch",
        "ar" => "Arabic",
        "sv" => "Swedish",
        "it" => "Italian",
        "id" => "Indonesian",
        "hi" => "Hindi",
        "fi" => "Finnish",
        "vi" => "Vietnamese",
        "he" => "Hebrew",
        "uk" => "Ukrainian",
        "el" => "Greek",
        "ms" => "Malay",
        "cs" => "Czech",
        "ro" => "Romanian",
        "da" => "Danish",
        "hu" => "Hungarian",
        "ta" => "Tamil",
        "no" => "Norwegian",
        "th" => "Thai",
        "ur" => "Urdu",
        "hr" => "Croatian",
        "bg" => "Bulgarian",
        "lt" => "Lithuanian",
        "la" => "Latin",
        "mi" => "Maori",
        "ml" => "Malayalam",
        "cy" => "Welsh",
        "sk" => "Slovak",
        "te" => "Telugu",
        "fa" => "Persian",
        "lv" => "Latvian",
        "bn" => "Bengali",
        "sr" => "Serbian",
        "az" => "Azerbaijani",
        "sl" => "Slovenian",
        "kn" => "Kannada",
        "et" => "Estonian",
        "mk" => "Macedonian",
        "br" => "Breton",
        "eu" => "Basque",
        "is" => "Icelandic",
        "hy" => "Armenian",
        "ne" => "Nepali",
        "mn" => "Mongolian",
        "bs" => "Bosnian",
        "kk" => "Kazakh",
        "sq" => "Albanian",
        "sw" => "Swahili",
        "gl" => "Galician",
        "mr" => "Marathi",
        "pa" => "Punjabi",
        "si" => "Sinhala",
        "km" => "Khmer",
        "sn" => "Shona",
        "yo" => "Yoruba",
        "so" => "Somali",
        "af" => "Afrikaans",
        "oc" => "Occitan",
        "ka" => "Georgian",
        "be" => "Belarusian",
        "tg" => "Tajik",
        "sd" => "Sindhi",
        "gu" => "Gujarati",
        "am" => "Amharic",
        "yi" => "Yiddish",
        "lo" => "Lao",
        "uz" => "Uzbek",
        "fo" => "Faroese",
        "ht" => "Haitian Creole",
        "ps" => "Pashto",
        "tk" => "Turkmen",
        "nn" => "Nynorsk",
        "mt" => "Maltese",
        "sa" => "Sanskrit",
        "lb" => "Luxembourgish",
        "my" => "Myanmar",
        "bo" => "Tibetan",
        "tl" => "Tagalog",
        "mg" => "Malagasy",
        "as" => "Assamese",
        "tt" => "Tatar",
        "haw" => "Hawaiian",
        "ln" => "Lingala",
        "ha" => "Hausa",
        "ba" => "Bashkir",
        "jw" => "Javanese",
        "su" => "Sundanese",
        other => other,
    }
}
