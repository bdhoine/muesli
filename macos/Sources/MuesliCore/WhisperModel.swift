import Foundation
import MuesliFFI

// WhisperModel is generated from the Rust core; this adds the Swift conveniences.
extension WhisperModel: CaseIterable, Identifiable, Sendable {
    public static var allCases: [WhisperModel] { [.small, .medium, .large] }

    public var id: String { rawValue }

    public var rawValue: String {
        switch self {
        case .small: "small"
        case .medium: "medium"
        case .large: "large"
        }
    }

    private var info: ModelInfo { modelInfo(m: self) }

    public var title: String { info.title }
    public var fileName: String { info.fileName }
    public var downloadURL: URL { URL(string: info.downloadUrl)! }

    /// Size of the published file. Downloads smaller than 90% of this are rejected.
    public var approximateBytes: Int64 { Int64(info.approximateBytes) }
    public var minimumValidBytes: Int64 { Int64(info.minimumValidBytes) }
    public var summary: String { info.summary }

    public var url: URL { Paths.support.appendingPathComponent(fileName) }

    /// On disk and large enough to be the whole file.
    public var isInstalled: Bool { WhisperModel.file(url, isAtLeast: minimumValidBytes) }

    /// Whether `url` exists and holds at least `minimum` bytes (and at least one).
    public static func file(_ url: URL, isAtLeast minimum: Int64) -> Bool {
        modelFileIsAtLeast(path: url.path, minimum: UInt64(max(minimum, 0)))
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
        switch modelSwitchDecide(active: active, wanted: wanted,
                                 installed: Array(installed), downloading: downloading) {
        case .nothing: .nothing
        case .switchNow: .switchNow
        case .download: .download
        case .wait: .wait
        }
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
    public var customModelURL: URL? { preferencesCustomModelPath(prefs: self).map { URL(fileURLWithPath: $0) } }

    /// The file Whisper will load: the custom model if set, else the selected download.
    public var modelURL: URL { URL(fileURLWithPath: preferencesModelPath(prefs: self, paths: Paths.current)) }
}
