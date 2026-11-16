import Foundation

/// The Whisper models Muesli can download. All come from whisper.cpp's ggml releases.
public enum WhisperModel: String, Codable, CaseIterable, Sendable, Identifiable {
    case small, medium, large

    public var id: String { rawValue }

    public var title: String {
        switch self {
        case .small: "Small"
        case .medium: "Medium"
        case .large: "Large"
        }
    }

    public var fileName: String {
        switch self {
        case .small: "ggml-small.bin"
        case .medium: "ggml-medium.bin"
        case .large: "ggml-large-v3.bin"
        }
    }

    public var downloadURL: URL {
        URL(string: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/\(fileName)")!
    }

    /// Size of the published file. Downloads smaller than 90% of this are rejected.
    public var approximateBytes: Int64 {
        switch self {
        case .small: 487_601_967
        case .medium: 1_533_763_059
        case .large: 3_095_033_483
        }
    }

    public var minimumValidBytes: Int64 { approximateBytes / 10 * 9 }

    public var summary: String {
        switch self {
        case .small: "Good balance of speed and accuracy"
        case .medium: "More accurate, slower"
        case .large: "Most accurate, slowest, needs plenty of memory"
        }
    }

    public var url: URL { Paths.support.appendingPathComponent(fileName) }

    /// On disk and large enough to be the whole file.
    public var isInstalled: Bool { WhisperModel.file(url, isAtLeast: minimumValidBytes) }

    /// Whether `url` exists and holds at least `minimum` bytes (and at least one).
    public static func file(_ url: URL, isAtLeast minimum: Int64) -> Bool {
        let size = (try? FileManager.default.attributesOfItem(atPath: url.path)[.size] as? NSNumber)?.int64Value ?? 0
        return size >= max(minimum, 1)
    }
}

/// What to do when the user picks a model in Settings. The active model keeps working until the
/// new one is on disk, so a pick never leaves Muesli without a model.
public enum ModelSwitch: Equatable, Sendable {
    /// Nothing to do: it is the active model already.
    case nothing
    /// The file is there: make it the active model now.
    case switchNow
    /// Not on disk: download it, then switch.
    case download
    /// Its download is already running: switch when it finishes.
    case wait

    public static func decide(active: WhisperModel, wanted: WhisperModel, installed: Set<WhisperModel>,
                              downloading: WhisperModel?) -> ModelSwitch {
        if wanted == active { return .nothing }
        if installed.contains(wanted) { return .switchNow }
        return downloading == wanted ? .wait : .download
    }
}

extension WhisperModel {
    /// Downloaded models other than the active one, and the space removing them frees.
    public static func unused(active: WhisperModel, installed: Set<WhisperModel>) -> (models: [WhisperModel], bytes: Int64) {
        let models = allCases.filter { $0 != active && installed.contains($0) }
        let bytes = models.reduce(Int64(0)) { sum, m in
            sum + (((try? FileManager.default.attributesOfItem(atPath: m.url.path)[.size]) as? NSNumber)?.int64Value ?? 0)
        }
        return (models, bytes)
    }
}

extension Preferences {
    /// A model file the user chose themselves; it replaces the downloadable models.
    public var customModelURL: URL? {
        guard let path = customModelPath, !path.isEmpty else { return nil }
        return URL(fileURLWithPath: (path as NSString).expandingTildeInPath)
    }

    /// The file Whisper will load: the custom model if set, else the selected download.
    public var modelURL: URL { customModelURL ?? model.url }
}
