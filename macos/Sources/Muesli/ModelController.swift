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

    func cancel() {
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
        case .failed(let message):
            lastError = message
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
