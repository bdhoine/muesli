import SwiftUI
import ServiceManagement
import UserNotifications

@MainActor @Observable
final class AppState: NSObject, UNUserNotificationCenterDelegate {
    enum Status: Equatable { case idle, recording(String), saving, error(String), permissionMissing(Permission) }

    var status: Status = .idle
    var recent: [URL] = Store.recent()
    private var pending: DetectedMeeting?
    private var current: Meeting?
    private var capture: AudioCapture?
    private var chunks: AsyncStream<AudioChunk>.Continuation?
    private var consumer: Task<Void, Never>?
    private let detector = MeetingDetector()

    func boot() {
        try? SMAppService.mainApp.register()                       // always run: start at login
        let center = UNUserNotificationCenter.current()
        center.delegate = self
        center.setNotificationCategories([UNNotificationCategory(
            identifier: "MEETING", actions: [UNNotificationAction(identifier: "START", title: "Start transcribing")],
            intentIdentifiers: [])])
        center.requestAuthorization(options: [.alert, .sound]) { _, _ in }
        detector.isRecording = { [weak self] in
            if case .idle = self?.status ?? .idle { return false } else { return true }
        }
        detector.onDetect = { [weak self] m in Task { @MainActor in self?.suggest(m) } }
        detector.start()
    }

    // MARK: Suggest
    private func suggest(_ m: DetectedMeeting) {
        pending = m
        let c = UNMutableNotificationContent()
        c.title = "Looks like a meeting: \(m.title)"
        c.body = "Detected via \(m.source). Start transcribing?"
        c.categoryIdentifier = "MEETING"
        UNUserNotificationCenter.current().add(UNNotificationRequest(identifier: "meeting", content: c, trigger: nil))
    }

    nonisolated func userNotificationCenter(_ center: UNUserNotificationCenter, didReceive r: UNNotificationResponse) async {
        guard r.actionIdentifier == "START" || r.actionIdentifier == UNNotificationDefaultActionIdentifier else { return }
        await MainActor.run { if let p = pending { Task { await start(p) } } }
    }

    // MARK: Record
    func start(_ d: DetectedMeeting) async {
        guard status == .idle else { return }
        if let missing = await Permissions.missingForRecording() {
            status = .permissionMissing(missing); return
        }
        guard let transcriber = Transcriber() else {
            status = .error("Whisper model missing: \(Paths.model.lastPathComponent)"); return
        }
        var m = Meeting(title: d.title, source: d.source)
        m.attendees = d.attendees
        current = m
        let (stream, cont) = AsyncStream<AudioChunk>.makeStream()
        chunks = cont
        consumer = Task { [weak self] in
            for await chunk in stream {                           // sequential: one Whisper instance
                if let segs = try? await transcriber.transcribe(chunk) {
                    await MainActor.run { self?.current?.transcript.append(contentsOf: segs) }
                }
            }
        }
        let cap = AudioCapture { cont.yield($0) }
        do { try await cap.start(); capture = cap; status = .recording(d.title) }
        catch { cont.finish(); status = .error("Capture failed: \(error.localizedDescription)") }
    }

    func stop() async {
        guard case .recording = status, var m = current else { return }
        status = .saving
        capture?.stop(); capture = nil
        chunks?.finish()
        await consumer?.value                                      // finish the last chunks
        m = current ?? m
        m.endedAt = Date()
        m.transcript.sort { $0.start < $1.start }
        Store.save(m)
        recent = Store.recent()
        current = nil
        status = .idle
    }

    // MARK: Settings
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
                Button("Dismiss") { state.status = .idle }
            case .permissionMissing(let p):
                Text(p.message)
                Button("Open Privacy Settings") { p.openSettings() }
                Button("Try again") { state.status = .idle }
            }
            Divider()
            ForEach(state.recent, id: \.self) { url in
                Button(url.deletingPathExtension().lastPathComponent) { NSWorkspace.shared.open(url) }
            }
            Divider()
            Button("Open transcripts folder") { NSWorkspace.shared.open(Paths.transcripts) }
            if Paths.transcriptsOverriddenByEnvironment {
                Text("Folder set by TRANSCRIPT_DIR")
            } else {
                Button("Choose transcripts folder…") { state.chooseTranscriptsFolder() }
                if UserDefaults.standard.string(forKey: Paths.transcriptDirKey) != nil {
                    Button("Reset to default folder") { state.resetTranscriptsFolder() }
                }
            }
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
