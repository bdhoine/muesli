import Foundation

// Muesli uses one speech model, Large v3 Turbo, plus a small voice activity model. Where the files
// live, what they are called and how big they are is decided by the Rust core.

extension Preferences {
    /// A model file the user chose themselves; it replaces the downloaded speech model.
    public var customModelURL: URL? { preferencesCustomModelPath(prefs: self).map { URL(fileURLWithPath: $0) } }

    /// The file Whisper will load: the custom model if set, else the downloaded one.
    public var modelURL: URL { URL(fileURLWithPath: preferencesModelPath(prefs: self, paths: Paths.current)) }

    /// The voice activity model that keeps Whisper from inventing text for silence.
    public var vadModelURL: URL { URL(fileURLWithPath: preferencesVadModelPath(prefs: self, paths: Paths.current)) }
}

public enum SpeechModel {
    /// The one speech model every install uses: title, size and the like.
    public static var info: ModelInfo { modelInfo() }

    /// Whether `url` exists and holds at least `minimum` bytes (and at least one).
    public static func file(_ url: URL, isAtLeast minimum: Int64) -> Bool {
        modelFileIsAtLeast(path: url.path, minimum: UInt64(max(minimum, 0)))
    }

    /// Model files from earlier versions, which let the user pick Small, Medium or Large.
    public static var legacy: LegacyModels { legacyModels(paths: Paths.current) }

    @discardableResult
    public static func removeLegacy() -> Int { Int(legacyModelsRemove(paths: Paths.current)) }
}
