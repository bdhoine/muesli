import SwiftUI
import ServiceManagement
import UserNotifications
import MuesliCore

@MainActor @Observable
final class AppState: NSObject, UNUserNotificationCenterDelegate {
    enum Status: Equatable { case idle, recording(String), saving, error(String), permissionMissing(Permission) }

    private(set) var status: Status = .idle
    private(set) var recent: [URL] = Store.recent()
    private(set) var settings = Preferences.load()
    let bridge = BridgeController()

    private var pending: DetectedMeeting?
    private var current: Meeting?
    private var lastReady: AppStatus.LastReady?
    private var capture: AudioCapture?
    private var chunks: AsyncStream<AudioChunk>.Continuation?
    private var consumer: Task<Void, Never>?
    private var heartbeat: Timer?
    private let detector = MeetingDetector()

    func boot() {
        try? SMAppService.mainApp.register()                       // always run: start at login
        recoverInterrupted()
        lastReady = MeetingStore.latestReady().map { .init(id: $0.id, title: $0.title, endedAt: $0.endedAt) }

        let center = UNUserNotificationCenter.current()
        center.delegate = self
        center.setNotificationCategories([
            UNNotificationCategory(identifier: "MEETING",
                                   actions: [UNNotificationAction(identifier: "START", title: "Start transcribing")], intentIdentifiers: []),
            UNNotificationCategory(identifier: "ENDED",
                                   actions: [UNNotificationAction(identifier: "STOP", title: "Stop and save")], intentIdentifiers: []),
        ])
        center.requestAuthorization(options: [.alert, .sound]) { _, _ in }

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
            Task { @MainActor in self?.publish() }
        }
        NotificationCenter.default.addObserver(forName: NSApplication.willTerminateNotification, object: nil, queue: .main) { [weak self] _ in
            MainActor.assumeIsolated { self?.bridge.shutdown() }
        }
        publish()
        bridge.apply(settings)
    }

    // MARK: State shared with muesli-mcp

    private func setStatus(_ s: Status) {
        status = s
        publish()
    }

    /// Writes state.json: what the app is doing right now.
    private func publish() {
        var out = AppStatus(appState: "idle", lastReady: lastReady)
        switch status {
        case .idle: break
        case .recording: out.appState = "recording"
        case .saving: out.appState = "finalizing"
        case .error(let e): out.appState = "error"; out.error = e
        case .permissionMissing(let p): out.appState = "permissionMissing"; out.error = p.message
        }
        if let d = detector.inProgress {
            out.detected = .init(title: d.meeting.title, source: d.meeting.source, since: d.since)
        }
        if let m = current {
            out.current = .init(id: m.id, title: m.title, source: m.source, startedAt: m.startedAt, segmentCount: m.transcript.count)
        }
        out.write()
    }

    /// A crash or forced quit mid-recording leaves a meeting in `recording`. Keep what was transcribed.
    private func recoverInterrupted() {
        for var m in MeetingStore.all() where m.state == .recording || m.state == .finalizing {
            if m.transcript.isEmpty {
                m.state = .failed
            } else {
                m.transcript.sort { $0.start < $1.start }
                m.endedAt = m.endedAt ?? m.startedAt.addingTimeInterval(m.transcript.map(\.end).max() ?? 0)
                m.state = .ready
                m.partial = true
                Store.save(m)
            }
            MeetingStore.save(m)
        }
        recent = Store.recent()
    }

    // MARK: Suggest

    private func suggest(_ m: DetectedMeeting) {
        guard status == .idle else { return }
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
        guard status == .idle else { return }
        if let missing = await Permissions.missingForRecording() {
            setStatus(.permissionMissing(missing)); return
        }
        guard let transcriber = Transcriber() else {
            setStatus(.error("Whisper model missing: \(Paths.model.lastPathComponent)")); return
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
        let cap = AudioCapture { cont.yield($0) }
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

    /// Transcribed segments arrive every ~30 s; persist them so they survive a crash
    /// and can be read while the meeting is still going on.
    private func append(_ segs: [TranscriptSegment]) {
        guard current != nil, !segs.isEmpty else { return }
        current?.transcript.append(contentsOf: segs)
        current?.transcript.sort { $0.start < $1.start }
        if let m = current { MeetingStore.save(m) }
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
        m.endedAt = Date()
        m.transcript.sort { $0.start < $1.start }
        m.state = .ready
        MeetingStore.save(m)
        Store.save(m)
        lastReady = .init(id: m.id, title: m.title, endedAt: m.endedAt)
        recent = Store.recent()
        current = nil
        UNUserNotificationCenter.current().removeDeliveredNotifications(withIdentifiers: ["ended"])
        setStatus(.idle)
    }

    func dismiss() { setStatus(.idle) }

    // MARK: Preferences

    func update(_ new: Preferences) {
        guard new != settings else { return }
        let bridgeChanged = new.mcpEnabled != settings.mcpEnabled || new.mcpMode != settings.mcpMode
            || new.httpPort != settings.httpPort || new.teamDomain != settings.teamDomain
            || new.audience != settings.audience || new.allowedEmail != settings.allowedEmail
        settings = new
        new.save()
        detector.endDelay = TimeInterval(new.endDelaySeconds)
        if bridgeChanged { bridge.apply(new) }
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
        recent = Store.recent()
    }

    func resetTranscriptsFolder() {
        Paths.setTranscripts(nil)
        recent = Store.recent()
    }
}

@main
struct MuesliApp: App {
    @State private var state = AppState()

    var body: some Scene {
        MenuBarExtra {
            switch state.status {
            case .idle:
                Button("Start transcribing") { Task { await state.start(.init(title: "Meeting", source: "manual")) } }
            case .recording(let t):
                Text("Recording: \(t)")
                Button("Stop and save transcript") { Task { await state.stop() } }
            case .saving: Text("Finishing transcript...")
            case .error(let e):
                Text(e)
                Button("Dismiss") { state.dismiss() }
            case .permissionMissing(let p):
                Text(p.message)
                Button("Open Privacy Settings") { p.openSettings() }
                Button("Try again") { state.dismiss() }
            }
            Divider()
            ForEach(state.recent, id: \.self) { url in
                Button(url.deletingPathExtension().lastPathComponent) { NSWorkspace.shared.open(url) }
            }
            Divider()
            Button("Open transcripts folder") { NSWorkspace.shared.open(Paths.transcripts) }
            if state.settings.mcpEnabled {
                switch state.settings.mcpMode {
                case .local: Text("Claude: local (Claude Desktop)")
                case .remote: Text(state.bridge.remoteRunning ? "Claude.ai bridge: running" : "Claude.ai bridge: not running")
                }
            }
            Button("Preferences…") { PreferencesWindow.show(state) }
                .keyboardShortcut(",")
            Button("Quit") { NSApplication.shared.terminate(nil) }
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
