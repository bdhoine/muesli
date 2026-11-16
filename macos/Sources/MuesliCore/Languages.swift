import Foundation

public enum Languages {
    public static let supported: [String] = [
        "en", "zh", "de", "es", "ru", "ko", "fr", "ja", "pt", "tr", "pl", "ca", "nl", "ar", "sv", "it", "id", "hi",
        "fi", "vi", "he", "uk", "el", "ms", "cs", "ro", "da", "hu", "ta", "no", "th", "ur", "hr", "bg", "lt", "la",
        "mi", "ml", "cy", "sk", "te", "fa", "lv", "bn", "sr", "az", "sl", "kn", "et", "mk", "br", "eu", "is", "hy",
        "ne", "mn", "bs", "kk", "sq", "sw", "gl", "mr", "pa", "si", "km", "sn", "yo", "so", "af", "oc", "ka", "be",
        "tg", "sd", "gu", "am", "yi", "lo", "uz", "fo", "ht", "ps", "tk", "nn", "mt", "sa", "lb", "my", "bo", "tl",
        "mg", "as", "tt", "haw", "ln", "ha", "ba", "jw", "su",
    ]

    public static func isSupported(_ code: String) -> Bool { supported.contains(code) }

    public static func baseCode(_ identifier: String) -> String {
        let base = identifier.lowercased().split(whereSeparator: { $0 == "-" || $0 == "_" }).first.map(String.init) ?? ""
        return base == "iw" ? "he" : base
    }

    public static func defaults(preferred: [String] = Locale.preferredLanguages) -> [String] {
        let os = preferred.first.map(baseCode).flatMap { isSupported($0) ? $0 : nil }
        return unique([os, "en"].compactMap { $0 })
    }

    public static func best(probabilities: [String: Float], allowed: [String]) -> String? {
        allowed.max { (probabilities[$0] ?? -1) < (probabilities[$1] ?? -1) }
    }

    public static func displayName(_ code: String, locale: Locale = .current) -> String {
        locale.localizedString(forLanguageCode: code)?.capitalized(with: locale) ?? code
    }

    public static func unique(_ codes: [String]) -> [String] {
        var seen = Set<String>()
        return codes.filter { seen.insert($0).inserted }
    }
}
