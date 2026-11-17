import AppKit
import MuesliCore

/// Knows whether the speech model (or the user's own file) and the voice activity model are on
/// disk, and downloads what is missing. The downloading is done by the Rust core, the same code the
/// Windows app uses. Recording is only allowed while `isReady` is true.
@MainActor @Observable
final class ModelController {
    enum Phase: Equatable {
        case missing
        case downloading(received: Int64, total: Int64)
        case ready(URL)
        case failed(String)
    }

    private final class Observer: ModelObserver, @unchecked Sendable {
        let handler: @Sendable (ModelPhase) -> Void
        init(_ handler: @escaping @Sendable (ModelPhase) -> Void) { self.handler = handler }
        func phaseChanged(phase: ModelPhase) { handler(phase) }   // on a download thread
    }

    private(set) var settings = Preferences()
    /// What the views show. Kept up to date by the downloader, so rendering never touches the disk.
    private(set) var phase: Phase = .missing
    /// Model files from earlier versions that can be removed.
    private(set) var legacy = LegacyModels(files: 0, bytes: 0)

    private var downloader: ModelDownloader?

    var isReady: Bool { if case .ready = phase { true } else { false } }
    var isDownloading: Bool { if case .downloading = phase { true } else { false } }

    /// What the menu bar shows while the model is being prepared.
    var activity: (text: String, fraction: Double?)? {
        guard case .downloading(let received, let total) = phase else { return nil }
        return ("Downloading the \(SpeechModel.info.title) speech model", total > 0 ? min(Double(received) / Double(total), 1) : nil)
    }

    var lastError: String? { if case .failed(let message) = phase { message } else { nil } }

    private func core() -> ModelDownloader {
        if let downloader { return downloader }
        let made = ModelDownloader(paths: Paths.current, prefs: settings, observer: Observer { [weak self] phase in
            Task { @MainActor in self?.receive(phase) }
        })
        downloader = made
        return made
    }

    private func receive(_ new: ModelPhase) {
        phase = Self.map(new)
        if case .ready = phase { legacy = SpeechModel.legacy }
    }

    private static func map(_ p: ModelPhase) -> Phase {
        switch p {
        case .missing: .missing
        case .downloading(let received, let total): .downloading(received: Int64(received), total: Int64(total))
        case .ready(let path): .ready(URL(fileURLWithPath: path))
        case .failed(let message): .failed(message)
        }
    }

    /// Re-reads the disk. Called when a window comes forward (the user may have added or removed
    /// a model file in Finder).
    func refresh() {
        receive(core().phase())
        legacy = SpeechModel.legacy
    }

    func apply(_ settings: Preferences) {
        self.settings = settings
        core().apply(prefs: settings)
        refresh()
        if case .missing = phase { ensureReady() }                // e.g. the user went back from their own file
    }

    /// Downloads whatever is missing: the speech model (unless the user chose their own file)
    /// and the voice activity model.
    func ensureReady() {
        core().ensureReady()
        receive(core().phase())                                   // the observer's update arrives a moment later
    }

    func cancel() { core().cancel() }

    /// Deletes the model files from earlier versions.
    func removeLegacy() {
        SpeechModel.removeLegacy()
        legacy = SpeechModel.legacy
    }

    func chooseCustomModel(_ settings: Preferences) -> Preferences? {
        let panel = NSOpenPanel()
        panel.title = "Choose a Whisper model"
        panel.message = "Pick a whisper.cpp ggml model file (.bin)."
        panel.prompt = "Use this model"
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = false
        NSApp.activate(ignoringOtherApps: true)
        guard panel.runModal() == .OK, let url = panel.url else { return nil }
        var new = settings
        new.customModelPath = url.path
        return new
    }
}
