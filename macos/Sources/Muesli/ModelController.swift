import AppKit
import MuesliCore

/// Knows which Whisper model Muesli should use, whether it is on disk, and downloads it.
/// Recording is only allowed while `isReady` is true.
@MainActor @Observable
final class ModelController {
    enum Phase: Equatable {
        case missing(WhisperModel)
        case downloading(WhisperModel, received: Int64, total: Int64)
        case ready(URL)
        case failed(String)
    }

    private(set) var settings = Preferences()
    private(set) var received: Int64 = 0
    private(set) var total: Int64 = 0
    private(set) var downloading: WhisperModel?
    private(set) var lastError: String?
    /// What the views show. Kept up to date by `refresh()`, so rendering never touches the disk.
    private(set) var phase: Phase = .missing(.small)
    private(set) var installed: Set<WhisperModel> = []

    private var downloader: Downloader?

    /// The model picked in Settings that is not active yet: it becomes active once it is on disk.
    private(set) var wanted: WhisperModel?
    /// Called when a picked model is ready to become the active one.
    var onSwitch: ((WhisperModel) -> Void)?
    private var selection: Task<Void, Never>?
    /// Downloaded models that are not in use, and the space they take.
    private(set) var unused: (models: [WhisperModel], bytes: Int64) = ([], 0)

    /// What the picker shows: the pick in progress, else the active model.
    var shownModel: WhisperModel { wanted ?? settings.model }

    var isReady: Bool { if case .ready = phase { true } else { false } }
    var isDownloading: Bool { downloading != nil }

    /// What the menu bar shows while the model is being prepared.
    var activity: (text: String, fraction: Double?)? {
        guard let model = downloading else { return nil }
        let fraction = total > 0 ? Double(received) / Double(total) : nil
        return ("Downloading the \(model.title) Whisper model", fraction)
    }

    func isInstalled(_ model: WhisperModel) -> Bool { installed.contains(model) }

    /// Re-reads the disk. Called after anything that can change it, and when a window comes forward
    /// (the user may have added or removed a model file in Finder).
    func refresh() {
        installed = Set(WhisperModel.allCases.filter(\.isInstalled))
        unused = WhisperModel.unused(active: settings.model, installed: installed)
        phase = currentPhase()
    }

    private func currentPhase() -> Phase {
        if let custom = settings.customModelURL {
            return WhisperModel.file(custom, isAtLeast: 1) ? .ready(custom) : .failed("The chosen model file was not found: \(custom.path)")
        }
        let model = settings.model
        if downloading == model { return .downloading(model, received: received, total: total) }
        if installed.contains(model) { return .ready(model.url) }
        if let lastError { return .failed(lastError) }
        return .missing(model)
    }

    func apply(_ settings: Preferences) {
        self.settings = settings
        refresh()
    }

    /// Downloads the selected model unless it is there already.
    func downloadSelectedIfNeeded() {
        guard settings.customModelURL == nil, !isInstalled(settings.model) else { return }
        download(settings.model)
    }

    /// A pick in Settings. The active model stays in use until the picked one is on disk; a missing
    /// one is downloaded after `delay`, so a slip of the mouse downloads nothing.
    func select(_ model: WhisperModel, delay: Duration = .seconds(3)) {
        selection?.cancel()
        switch ModelSwitch.decide(active: settings.model, wanted: model, installed: installed, downloading: downloading) {
        case .nothing:
            abandonPick()
        case .switchNow:
            abandonPick()
            onSwitch?(model)
        case .wait:
            wanted = model
        case .download:
            wanted = model
            lastError = nil
            selection = Task { [weak self] in
                try? await Task.sleep(for: delay)
                guard let self, !Task.isCancelled, self.wanted == model else { return }
                if self.downloading != nil { self.cancelDownload() }
                self.download(model)
            }
        }
    }

    /// Drops a pick that is not active yet, and its download.
    private func abandonPick() {
        if let pick = wanted, downloading == pick { cancelDownload() }
        wanted = nil
    }

    func removeUnused() {
        for model in unused.models { try? FileManager.default.removeItem(at: model.url) }
        refresh()
    }

    func download(_ model: WhisperModel) {
        guard downloading == nil else { return }
        lastError = nil
        downloading = model
        received = 0
        total = model.approximateBytes
        phase = currentPhase()
        let d = Downloader(model: model) { [weak self] event in
            Task { @MainActor in self?.handle(event, model: model) }
        }
        downloader = d
        d.start()
    }

    /// Cancels the download and any pick that was waiting for it.
    func cancel() {
        selection?.cancel()
        wanted = nil
        cancelDownload()
    }

    private func cancelDownload() {
        downloader?.cancel()
        finish()
    }

    func delete(_ model: WhisperModel) {
        try? FileManager.default.removeItem(at: model.url)
        refresh()
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

    private func handle(_ event: Downloader.Event, model: WhisperModel) {
        guard downloading == model else { return }
        switch event {
        case .progress(let done, let expected):
            if expected > 0 { total = expected }
            // Only publish when the visible value changes (whole MB), not on every network callback.
            guard done / 1_000_000 != received / 1_000_000 else { return }
            received = done
            phase = currentPhase()
        case .finished:
            finish()
            if wanted == model {                                      // the pick is on disk: switch now
                wanted = nil
                onSwitch?(model)
            }
        case .failed(let message):
            lastError = message
            if wanted == model { wanted = nil }                       // keep the active model
            finish()
        }
    }

    private func finish() {
        downloading = nil
        downloader = nil
        refresh()
    }
}

/// One download into `<model>.part`, moved into place only when it is complete.
private final class Downloader: NSObject, URLSessionDownloadDelegate, @unchecked Sendable {
    enum Event { case progress(Int64, Int64), finished, failed(String) }

    private let model: WhisperModel
    private let report: (Event) -> Void
    private var task: URLSessionDownloadTask?
    private var session: URLSession?
    private var cancelled = false

    init(model: WhisperModel, report: @escaping (Event) -> Void) {
        self.model = model
        self.report = report
    }

    func start() {
        let s = URLSession(configuration: .default, delegate: self, delegateQueue: nil)
        session = s
        task = s.downloadTask(with: model.downloadURL)
        task?.resume()
    }

    func cancel() {
        cancelled = true
        task?.cancel()
        session?.invalidateAndCancel()
    }

    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didWriteData _: Int64,
                    totalBytesWritten written: Int64, totalBytesExpectedToWrite expected: Int64) {
        report(.progress(written, expected))
    }

    func urlSession(_ session: URLSession, downloadTask: URLSessionDownloadTask, didFinishDownloadingTo location: URL) {
        // The temporary file disappears when this returns, so validate and move it now.
        if let http = downloadTask.response as? HTTPURLResponse, !(200..<300).contains(http.statusCode) {
            report(.failed("The download server answered HTTP \(http.statusCode).")); return
        }
        let size = (try? FileManager.default.attributesOfItem(atPath: location.path)[.size] as? NSNumber)?.int64Value ?? 0
        let expected = downloadTask.countOfBytesExpectedToReceive
        guard size >= model.minimumValidBytes, expected <= 0 || size == expected else {
            report(.failed("The download is incomplete. Try again.")); return
        }
        do {
            let fm = FileManager.default
            try? fm.removeItem(at: model.url)
            try fm.moveItem(at: location, to: model.url)
            report(.finished)
        } catch {
            report(.failed("Could not save the model: \(error.localizedDescription)"))
        }
    }

    func urlSession(_ session: URLSession, task: URLSessionTask, didCompleteWithError error: Error?) {
        session.finishTasksAndInvalidate()
        guard let error, !cancelled else { return }
        report(.failed("Download failed: \(error.localizedDescription)"))
    }
}
