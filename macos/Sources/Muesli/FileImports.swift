import AppKit
import UniformTypeIdentifiers
import MuesliCore

/// Audio files waiting to be transcribed, one at a time: the ones picked from the menu and the ones
/// found in the drop folder. The work (decoding, chunking, Whisper) is the shared Rust core; this
/// class is the queue and what the UI shows.
@MainActor @Observable
final class FileImports {
    enum Origin { case manual, folder }
    enum State: Equatable { case waiting, running(Double?), done, failed(String), cancelled }

    struct Item: Identifiable, Equatable {
        let id = UUID()
        let url: URL
        let origin: Origin
        var state: State = .waiting
        var name: String { url.lastPathComponent }
        var isFinished: Bool {
            switch state { case .waiting, .running: false; default: true }
        }
    }

    private(set) var items: [Item] = []
    /// The last problem with a file from the drop folder, shown in the menu until dismissed.
    private(set) var folderError: String?

    /// Set by AppState.
    var canRun: () -> Bool = { true }
    var transcriber: () -> Transcriber? = { nil }
    var onFinished: (Meeting) -> Void = { _ in }
    var onChange: () -> Void = {}
    var onIdle: () -> Void = {}

    private var cleanup: AudioCleanup = .keep
    private var folder: URL?
    private var dropFolder: DropFolder?
    private var timer: Timer?
    private var runner: Task<Void, Never>?
    private var importer: FileImporter?

    // MARK: What the views show

    var running: Item? { items.first { if case .running = $0.state { true } else { false } } }
    var waitingCount: Int { items.filter { $0.state == .waiting }.count }
    var isBusy: Bool { running != nil || waitingCount > 0 }
    var manualItems: [Item] { items.filter { $0.origin == .manual } }

    /// 0 to 1 for the running file, nil while its length is unknown.
    var progress: Double? {
        if case .running(let p) = running?.state { p } else { nil }
    }

    // MARK: Settings

    func configure(folder newFolder: URL?, cleanup newCleanup: AudioCleanup) {
        cleanup = newCleanup
        guard newFolder != folder else { return }
        folder = newFolder
        timer?.invalidate(); timer = nil
        guard newFolder != nil else { dropFolder = nil; return }
        dropFolder = DropFolder(paths: Paths.current)
        timer = Timer.scheduledTimer(withTimeInterval: 5, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.poll() }
        }
        poll()
    }

    private func poll() {
        guard let folder, let dropFolder else { return }
        let found = dropFolder.poll(folder: folder.path).map { URL(fileURLWithPath: $0) }
        if !found.isEmpty { enqueue(found, origin: .folder) }
    }

    func dismissError() { folderError = nil }

    // MARK: Queue

    func enqueue(_ urls: [URL], origin: Origin) {
        items.removeAll { $0.isFinished && $0.origin == origin && origin == .manual && urls.contains($0.url) }
        items += urls.map { Item(url: $0, origin: origin) }
        onChange()
        if runner == nil { runner = Task { await run() } }
    }

    func cancel(_ id: Item.ID) {
        guard let index = items.firstIndex(where: { $0.id == id }) else { return }
        switch items[index].state {
        case .waiting: finish(index, .cancelled)
        case .running: importer?.cancel()
        default: break
        }
    }

    func cancelAllManual() {
        for item in manualItems where !item.isFinished { cancel(item.id) }
    }

    /// Forgets finished manual items (the small window was closed).
    func clearFinished() {
        items.removeAll { $0.isFinished && $0.origin == .manual }
        if items.count > 200 { items.removeAll { $0.isFinished } }
    }

    private func finish(_ index: Int, _ state: State) {
        items[index].state = state
        if items[index].origin == .folder { items.remove(at: index) }   // nothing to show for them afterwards
        onChange()
    }

    // MARK: Running

    private func run() async {
        defer { runner = nil; onChange(); onIdle() }
        while let next = items.first(where: { $0.state == .waiting }) {
            // A recording, or a model that is not ready, goes first.
            while !canRun() {
                try? await Task.sleep(for: .seconds(1))
                if !items.contains(where: { $0.id == next.id && $0.state == .waiting }) { break }
            }
            guard let index = items.firstIndex(where: { $0.id == next.id }), items[index].state == .waiting else { continue }
            await process(index)
        }
    }

    private func process(_ index: Int) async {
        let item = items[index]
        items[index].state = .running(nil)
        onChange()
        guard let transcriber = transcriber() else {
            fail(item, "The speech model could not be loaded.")
            return
        }
        let importer = FileImporter()
        self.importer = importer
        let observer = ProgressObserver { [weak self] fraction in
            Task { @MainActor in self?.setProgress(item.id, fraction < 0 ? nil : fraction) }
        }
        let url = item.url, paths = Paths.current, core = transcriber.core
        let result: Result<Meeting, Error> = await Task.detached {
            Result { try importer.import(transcriber: core, path: url.path, paths: paths, observer: observer) }
        }.value
        self.importer = nil

        switch result {
        case .success(let meeting):
            onFinished(meeting)
            if let index = items.firstIndex(where: { $0.id == item.id }) { finish(index, .done) }
            if item.origin == .folder { afterFolderFile(item.url) }
        case .failure(let error):
            if case ImportFailure.Cancelled = error {
                if item.origin == .folder { dropFolder?.markDone(path: item.url.path) }
                if let index = items.firstIndex(where: { $0.id == item.id }) { finish(index, .cancelled) }
            } else {
                fail(item, (error as? ImportFailure).flatMap(message) ?? error.localizedDescription)
            }
        }
    }

    private func message(_ failure: ImportFailure) -> String? {
        if case .Failed(let message) = failure { message } else { nil }
    }

    private func fail(_ item: Item, _ message: String) {
        if item.origin == .folder {
            dropFolder?.markDone(path: item.url.path)                // do not try the same file again
            folderError = "\(item.name): \(message)"
        }
        if let index = items.firstIndex(where: { $0.id == item.id }) { finish(index, .failed(message)) }
    }

    /// A folder file was transcribed: remember it, then apply the cleanup setting.
    private func afterFolderFile(_ url: URL) {
        dropFolder?.markDone(path: url.path)
        guard cleanup != .keep else { return }
        if let problem = audioCleanUp(path: url.path, mode: cleanup) {
            folderError = "\(url.lastPathComponent) was transcribed, but could not be removed: \(problem)"
        }
        dropFolder?.forget(path: url.path)
    }

    private func setProgress(_ id: Item.ID, _ fraction: Double?) {
        guard let index = items.firstIndex(where: { $0.id == id }), case .running = items[index].state else { return }
        items[index].state = .running(fraction)
        onChange()
    }

    // MARK: Picking files

    /// Asks for the folder to drop audio files in.
    static func chooseDropFolder(current: URL?) -> URL? {
        let panel = NSOpenPanel()
        panel.title = "Choose a folder to drop audio files in"
        panel.message = "Muesli transcribes audio files that appear in this folder."
        panel.prompt = "Use this folder"
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.canCreateDirectories = true
        panel.allowsMultipleSelection = false
        panel.directoryURL = current
        NSApp.activate(ignoringOtherApps: true)
        return panel.runModal() == .OK ? panel.url : nil
    }

    /// Asks for audio files and queues them. Returns whether any were chosen.
    @discardableResult
    func chooseFiles() -> Bool {
        let panel = NSOpenPanel()
        panel.title = "Choose audio files to transcribe"
        panel.prompt = "Transcribe"
        panel.canChooseFiles = true
        panel.canChooseDirectories = false
        panel.allowsMultipleSelection = true
        panel.allowedContentTypes = audioExtensions().compactMap { UTType(filenameExtension: $0) }
        NSApp.activate(ignoringOtherApps: true)
        guard panel.runModal() == .OK, !panel.urls.isEmpty else { return false }
        enqueue(panel.urls, origin: .manual)
        return true
    }
}

private final class ProgressObserver: ImportObserver, @unchecked Sendable {
    let handler: @Sendable (Double) -> Void
    init(_ handler: @escaping @Sendable (Double) -> Void) { self.handler = handler }
    func progress(fraction: Double) { handler(fraction) }          // on the import thread
}

extension Preferences {
    var dropFolderURL: URL? { dropFolder.map { URL(fileURLWithPath: $0) } }
}

extension AudioCleanup {
    var title: String {
        switch self {
        case .keep: "Keep the file"
        case .trash: "Move to the Trash"
        case .delete: "Delete the file"
        }
    }

    var detail: String {
        switch self {
        case .keep: "The file stays in the folder. It is not transcribed again."
        case .trash: "Once transcribed, the file goes to the Trash."
        case .delete: "Once transcribed, the file is deleted for good."
        }
    }
}
