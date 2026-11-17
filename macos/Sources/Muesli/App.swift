import SwiftUI
import ServiceManagement
import UserNotifications
import MuesliCore

@MainActor @Observable
final class AppState: NSObject, UNUserNotificationCenterDelegate {
    enum Status: Equatable { case idle, recording(String), saving, error(String), permissionMissing(Permission) }

    private(set) var status: Status = .idle
    private(set) var settings = Preferences.load()
    private(set) var recent: [RecentTranscript] = []
    let bridge = BridgeController()
    let model = ModelController()
    let files = FileImports()

    private var pending: DetectedMeeting?
    private var current: Meeting?
    private var lastReady: AppStatus.LastReady?
    private var capture: AudioCapture?
    private var chunks: AsyncStream<AudioChunk>.Continuation?
    private var consumer: Task<Void, Never>?
    private var heartbeat: Timer?
    private var loadedTranscriber: (key: String, value: Transcriber)?
    private let detector = MeetingDetector()

    func boot() {
        try? SMAppService.mainApp.register()                       // always run: start at login
        Paths.migrateLegacyTranscriptsFolder()                     // older versions kept the folder in UserDefaults
        settings = Preferences.load()
        MeetingStore.recoverInterrupted()                          // keep what an interrupted recording transcribed
        MeetingStore.importTranscripts()
        refreshRecent()
        lastReady = MeetingStore.latestReady().map { .init(id: $0.id, title: $0.title, endedAt: $0.endedAt) }

        let center = UNUserNotificationCenter.current()
        center.delegate = self
        center.setNotificationCategories([
            UNNotificationCategory(identifier: "MEETING",
                                   actions: [UNNotificationAction(identifier: "START", title: "Start transcribing")], intentIdentifiers: []),
            UNNotificationCategory(identifier: "ENDED",
                                   actions: [UNNotificationAction(identifier: "STOP", title: "Stop and save")], intentIdentifiers: []),
        ])

        detector.endDelay = TimeInterval(settings.endDelaySeconds)
        detector.isRecording = { [weak self] in
            if case .idle = self?.status ?? .idle { return false } else { return true }
        }
        detector.onDetect = { [weak self] m in Task { @MainActor in self?.suggest(m) } }
        detector.onChange = { [weak self] in Task { @MainActor in self?.publish() } }
        detector.onCallEnded = { [weak self] in Task { @MainActor in await self?.callEnded() } }
        detector.start()

        // state.json is the heartbeat other processes read; keep it fresh.
        heartbeat = Timer.scheduledTimer(withTimeInterval: 10, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.syncWithStore() }
        }
        files.canRun = { [weak self] in self.map { $0.status == .idle && $0.model.isReady } ?? false }
        files.transcriber = { [weak self] in self?.sharedTranscriber() }
        files.onFinished = { [weak self] m in
            self?.lastReady = .init(id: m.id, title: m.title, endedAt: m.endedAt)
            self?.refreshRecent()
        }
        files.onChange = { [weak self] in self?.publish() }
        files.onIdle = { [weak self] in self?.releaseTranscriberIfIdle() }
        files.configure(folder: settings.dropFolderURL, cleanup: settings.dropFolderCleanup)
        publish()
        bridge.apply(settings)
        model.apply(settings)
        model.ensureReady()                                        // downloads what is missing
        if !settings.onboardingDone {
            OnboardingWindow.show(self)
        } else if model.isDownloading {
            SettingsWindow.show(self, page: .transcription)               // the model went missing: show the progress
        }
    }

    // MARK: State shared with muesli-mcp

    private func setStatus(_ s: Status) {
        status = s
        publish()
    }

    /// Writes state.json: what the app is doing right now.
    private func publish() {
        var out = AppStatus(appState: "idle", lastReady: lastReady)
        let file = files.running
        out.transcriptsFolder = Paths.transcripts.path
        switch status {
        case .idle:
            if file != nil { out.appState = "transcribingFile" } else if model.isDownloading { out.appState = "preparing" }
        case .recording: out.appState = "recording"
        case .saving: out.appState = "finalizing"
        case .error(let e): out.appState = "error"; out.error = e
        case .permissionMissing(let p): out.appState = "permissionMissing"; out.error = p.message
        }
        if let d = detector.inProgress {
            out.detected = .init(title: d.meeting.title, source: d.meeting.source, since: d.since)
        }
        if let file {
            out.fileJob = .init(name: file.name, progress: files.progress, waiting: UInt32(files.waitingCount))
        }
        if let m = current {
            out.current = .init(id: m.id, title: m.title, source: m.source, startedAt: m.startedAt, segmentCount: UInt32(m.transcript.count))
        }
        out.write()
    }

    // MARK: Suggest

    private func suggest(_ m: DetectedMeeting) {
        guard status == .idle, model.isReady else { return }
        pending = m
        notify(id: "meeting", category: "MEETING", title: "Looks like a meeting: \(m.title)",
               body: "Detected via \(m.source). Start transcribing?")
    }

    private func callEnded() async {
        guard case .recording = status else { return }
        if settings.autoStop {
            await stop()
        } else {
            notify(id: "ended", category: "ENDED", title: "The call seems to have ended",
                   body: "Muesli is still recording. Stop and save the transcript?")
        }
    }

    private func notify(id: String, category: String, title: String, body: String) {
        let c = UNMutableNotificationContent()
        c.title = title
        c.body = body
        c.categoryIdentifier = category
        UNUserNotificationCenter.current().add(UNNotificationRequest(identifier: id, content: c, trigger: nil))
    }

    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive r: UNNotificationResponse) async {
        let action = r.actionIdentifier
        let category = r.notification.request.content.categoryIdentifier
        await MainActor.run {
            if category == "ENDED" {
                if action == "STOP" { Task { await stop() } }
            } else if action == "START" || action == UNNotificationDefaultActionIdentifier {
                if let p = pending { Task { await start(p) } }
            }
        }
    }

    // MARK: Record

    func start(_ d: DetectedMeeting) async {
        guard status == .idle, model.isReady else { return }
        if let missing = await Permissions.missingForRecording() {
            setStatus(.permissionMissing(missing)); return
        }
        guard let transcriber = sharedTranscriber() else {
            setStatus(.error("Whisper model could not be loaded: \(settings.modelURL.lastPathComponent)")); return
        }
        guard status == .idle else { return }                      // something else started while we were waiting
        let m = Meeting(title: d.title, source: d.source, attendees: d.attendees, state: .recording)
        current = m
        MeetingStore.save(m)
        let (stream, cont) = AsyncStream<AudioChunk>.makeStream()
        chunks = cont
        consumer = Task { [weak self] in
            for await chunk in stream {                           // sequential: one Whisper instance
                if let segs = try? await transcriber.transcribe(chunk) {
                    await MainActor.run { self?.append(segs) }
                }
            }
        }
        let cap = AudioCapture(emit: { cont.yield($0) }, onError: { [weak self] message in
            Task { @MainActor in await self?.captureBroke(message) }
        })
        do {
            try await cap.start()
            capture = cap
            UNUserNotificationCenter.current().removeDeliveredNotifications(withIdentifiers: ["meeting"])
            setStatus(.recording(d.title))
        } catch {
            cap.stop()
            cont.finish()
            consumer = nil; chunks = nil
            current?.state = .failed
            if let failed = current { MeetingStore.save(failed) }
            current = nil
            setStatus(.error("Capture failed: \(error.localizedDescription)"))
        }
    }

    /// One loaded Whisper model for live recording and files alike (its lock lets them take turns),
    /// kept while either is busy.
    private func sharedTranscriber() -> Transcriber? {
        let key = [settings.modelURL.path, settings.vadModelURL.path] + settings.effectiveLanguages
        if let loaded = loadedTranscriber, loaded.key == key.joined(separator: "|") { return loaded.value }
        guard let fresh = Transcriber(modelURL: settings.modelURL, vadURL: settings.vadModelURL, languages: settings.effectiveLanguages) else { return nil }
        loadedTranscriber = (key.joined(separator: "|"), fresh)
        return fresh
    }

    private func releaseTranscriberIfIdle() {
        if status == .idle, !files.isBusy { loadedTranscriber = nil }
    }

    /// Transcribed segments arrive every ~30 s; persist them so they survive a crash
    /// and can be read while the meeting is still going on.
    private func append(_ segs: [TranscriptSegment]) {
        guard var m = current, !segs.isEmpty else { return }
        MeetingStore.append(segs, to: &m)
        current = m
        publish()
    }

    func stop() async {
        guard case .recording = status, current != nil else { return }
        current?.state = .finalizing
        if let m = current { MeetingStore.save(m) }
        setStatus(.saving)
        capture?.stop(); capture = nil
        chunks?.finish()
        await consumer?.value                                      // finish the last chunks
        consumer = nil; chunks = nil
        guard var m = current else { setStatus(.idle); return }
        MeetingStore.finish(&m)
        lastReady = .init(id: m.id, title: m.title, endedAt: m.endedAt)
        refreshRecent()
        current = nil
        UNUserNotificationCenter.current().removeDeliveredNotifications(withIdentifiers: ["ended"])
        setStatus(.idle)
        releaseTranscriberIfIdle()
    }

    /// A stream broke mid-recording and could not be restarted: keep what was transcribed.
    private func captureBroke(_ message: String) async {
        guard case .recording = status else { return }
        await stop()
        setStatus(.error("Capture stopped: \(message) The transcript so far was saved."))
    }

    func dismiss() { setStatus(.idle) }


    // MARK: Preferences

    func update(_ edited: Preferences) {
        // The transcripts folder is changed only through Paths (the folder buttons in Settings), so a
        // preferences copy that was opened earlier must not write an old value back.
        var new = edited
        new.transcriptsFolder = Preferences.load().transcriptsFolder
        guard new != settings else { return }
        let bridgeChanged = new.mcpEnabled != settings.mcpEnabled || new.claudeDesktopConfigEntry != settings.claudeDesktopConfigEntry
        settings = new
        new.save()
        model.apply(new)
        files.configure(folder: new.dropFolderURL, cleanup: new.dropFolderCleanup)
        detector.endDelay = TimeInterval(new.endDelaySeconds)
        refreshRecent()
        if bridgeChanged { bridge.apply(new) }
    }

    private func syncWithStore() {
        if current == nil {
            lastReady = MeetingStore.latestReady().map { .init(id: $0.id, title: $0.title, endedAt: $0.endedAt) }
        }
        refreshRecent()
        publish()
    }

    func refreshRecent() {
        recent = Store.recentTranscripts(limit: Int(settings.effectiveRecentLimit))
    }

    func chooseTranscriptsFolder() {
        let panel = NSOpenPanel()
        panel.title = "Choose where Muesli saves transcripts"
        panel.prompt = "Use this folder"
        panel.canChooseDirectories = true
        panel.canChooseFiles = false
        panel.canCreateDirectories = true
        panel.allowsMultipleSelection = false
        panel.directoryURL = Paths.transcripts
        NSApp.activate(ignoringOtherApps: true)
        guard panel.runModal() == .OK, let url = panel.url else { return }
        Paths.setTranscripts(url)
        settings = Preferences.load()
        refreshRecent()
    }

    func resetTranscriptsFolder() {
        Paths.setTranscripts(nil)
        settings = Preferences.load()
        refreshRecent()
    }
}

@main
struct MuesliApp: App {
    @State private var state = AppState()

    var body: some Scene {
        MenuBarExtra {
            switch state.status {
            case .idle:
                if let activity = state.model.activity {
                    Section("Getting Ready") {
                        Text(activity.text + (activity.fraction.map { " · \(Int($0 * 100))%" } ?? "…"))
                    }
                } else if !state.model.isReady {
                    Section("Model Needed") {
                        Text("Transcribing needs a Whisper model.")
                        Button { SettingsWindow.show(state, page: .transcription) } label: {
                            Label("Open Model Settings…", systemImage: "arrow.down.circle")
                        }
                    }
                }
                Button { Task { await state.start(.init(title: "Meeting", source: "manual")) } } label: {
                    Label("Start Transcribing", systemImage: "record.circle")
                }
                .disabled(!state.model.isReady)
            case .recording(let title):
                Section("Recording · \(title)") {
                    Button { Task { await state.stop() } } label: {
                        Label("Stop and Save", systemImage: "stop.circle")
                    }
                }
            case .saving:
                Text("Saving Transcript…")
            case .error(let message):
                Section("Something Went Wrong") {
                    Text(message)
                    Button("Dismiss") { state.dismiss() }
                }
            case .permissionMissing(let permission):
                Section("Permission Needed") {
                    Text(permission.message)
                    Button { permission.openSettings() } label: {
                        Label("Open Privacy Settings…", systemImage: "lock.shield")
                    }
                    Button("Show All Permissions…") { SettingsWindow.show(state, page: .permissions) }
                    Button("Try Again") { state.dismiss() }
                }
            }

            Divider()
            Button { if state.files.chooseFiles() { AudioFileWindow.show(state) } } label: {
                Label("Transcribe Audio File…", systemImage: "waveform.badge.plus")
            }
            .disabled(!state.model.isReady)
            FileStatusItems(files: state.files)

            if state.settings.effectiveRecentLimit > 0 {
                Divider()
                Section("Recent Transcripts") {
                    if state.recent.isEmpty {
                        Text("No transcripts yet")
                    } else {
                        ForEach(state.recent) { transcript in
                            Button { NSWorkspace.shared.open(transcript.url) } label: {
                                Text(Store.shortenTitle(transcript.title))
                                Text(transcript.subtitle())
                            }
                        }
                    }
                }
            }

            Divider()
            Button { NSWorkspace.shared.open(Paths.transcripts) } label: {
                Label("Open Transcripts Folder", systemImage: "folder")
            }

            Divider()
            Button { SettingsWindow.show(state) } label: {
                Label("Settings…", systemImage: "gearshape")
            }
            .keyboardShortcut(",")
            Button { NSApplication.shared.terminate(nil) } label: {
                Label("Quit Muesli", systemImage: "power")
            }
            .keyboardShortcut("q")
        } label: {
            if case .recording = state.status {
                Image(systemName: "record.circle.fill")
            } else {
                Image(nsImage: Self.menuBarIcon)
            }
        }
    }

    private static let menuBarIcon: NSImage = {
        let image = NSImage(named: "muesliTemplate") ?? NSImage(systemSymbolName: "waveform", accessibilityDescription: "Muesli")!
        image.isTemplate = true
        image.size = NSSize(width: 18, height: 18)
        return image
    }()

    init() { DispatchQueue.main.async { [state] in state.boot() } }
}

/// What the menu says about audio files: the one being transcribed, how many wait, and the last
/// problem with a file from the drop folder.
struct FileStatusItems: View {
    let files: FileImports

    var body: some View {
        if let running = files.running {
            Section("Audio Files") {
                Text("Transcribing \(Store.shortenTitle(running.name))" + (files.progress.map { " · \(Int($0 * 100))%" } ?? "…"))
                if files.waitingCount > 0 { Text("\(files.waitingCount) more waiting") }
            }
        } else if files.waitingCount > 0 {
            Section("Audio Files") { Text("\(files.waitingCount) waiting for the model or the recording") }
        }
        if let problem = files.folderError {
            Section("Audio File Problem") {
                Text(problem)
                Button("Dismiss") { files.dismissError() }
            }
        }
    }
}
