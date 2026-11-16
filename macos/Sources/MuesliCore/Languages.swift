import Foundation
import MuesliFFI

public enum Languages {
    public static let supported: [String] = languagesSupported()

    public static func isSupported(_ code: String) -> Bool { languageIsSupported(code: code) }

    public static func baseCode(_ identifier: String) -> String { languageBaseCode(identifier: identifier) }

    public static func defaults(preferred: [String] = Locale.preferredLanguages) -> [String] {
        languagesDefaults(preferred: preferred)
    }

    public static func best(probabilities: [String: Float], allowed: [String]) -> String? {
        languagesBest(probabilities: probabilities, allowed: allowed)
    }

    public static func displayName(_ code: String, locale: Locale = .current) -> String {
        locale.localizedString(forLanguageCode: code)?.capitalized(with: locale) ?? code
    }

    public static func unique(_ codes: [String]) -> [String] { languagesUnique(codes: codes) }
}
