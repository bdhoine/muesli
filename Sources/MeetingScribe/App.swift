import SwiftUI
import ServiceManagement
import UserNotifications

@MainActor @Observable
final class AppState: NSObject, UNUserNotificationCenterDelegate {
    enum Status: Equatable { case idle, recording(String), saving, error(String) }

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
}

@main
struct MeetingScribeApp: App {
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
            case .error(let e): Text(e)
            }
            Divider()
            ForEach(state.recent, id: \.self) { url in
                Button(url.deletingPathExtension().lastPathComponent) { NSWorkspace.shared.open(url) }
            }
            Divider()
            Button("Open transcripts folder") { NSWorkspace.shared.open(Paths.transcripts) }
            Button("Quit") { NSApplication.shared.terminate(nil) }
        } label: {
            Image(systemName: { if case .recording = state.status { return "record.circle.fill" } else { return "waveform" } }())
        }
    }

    init() { DispatchQueue.main.async { [state] in state.boot() } }
}
