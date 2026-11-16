import AppKit
import CoreAudio
import EventKit
import MuesliCore

/// Combines three signals: calendar (EventKit, covers Outlook/Google accounts synced
/// into macOS Calendar), Teams process, and mic-in-use (CoreAudio).
///
/// It keeps running during a recording, so it always knows whether a meeting is in
/// progress (`inProgress`) and can tell when a call has ended.
final class MeetingDetector {
    /// A meeting just started and is not being recorded: time to suggest transcribing.
    var onDetect: ((DetectedMeeting) -> Void)?
    /// `inProgress` changed.
    var onChange: (() -> Void)?
    /// The call being recorded seems to be over (other apps released the mic).
    var onCallEnded: (() -> Void)?
    var isRecording: () -> Bool = { false }
    /// How long the mic must stay released before a call counts as ended.
    var endDelay: TimeInterval {
        get { logic.endDelay }
        set { logic.endDelay = newValue }
    }

    /// The meeting or call that is going on right now, whether or not it is recorded.
    var inProgress: (meeting: DetectedMeeting, since: Date)? { logic.inProgress }

    private var logic = DetectionLogic()            // the decisions; this class only samples the system
    private let store = Permissions.eventStore       // the store that was granted access
    /// Read from the system each time, so allowing it later in System Settings takes effect without a restart.
    private var calendarOK: Bool { EKEventStore.authorizationStatus(for: .event) == .fullAccess }
    private var timer: Timer?

    private let teamsBundles = ["com.microsoft.teams2", "com.microsoft.teams"]

    func start() {
        timer = Timer.scheduledTimer(withTimeInterval: 5, repeats: true) { [weak self] _ in self?.tick() }
    }

    private func tick() {
        let now = Date()
        let recording = isRecording()
        let teams = NSWorkspace.shared.runningApplications.contains { teamsBundles.contains($0.bundleIdentifier ?? "") }
        let events = logic.tick(now: now, recording: recording, calendar: currentCalendarMeeting(now),
                                micBusy: otherAppUsingMic(recording: recording), teamsRunning: teams)
        for event in events {
            switch event {
            case .suggest(let meeting): onDetect?(meeting)
            case .inProgressChanged: onChange?()
            case .callEnded: onCallEnded?()
            }
        }
    }

    // Calendar: an event that is running now (or starts within 2 minutes) and looks like a meeting.
    private func currentCalendarMeeting(_ now: Date) -> (meeting: DetectedMeeting, eventID: String)? {
        guard calendarOK else { return nil }
        let pred = store.predicateForEvents(withStart: now.addingTimeInterval(-12 * 3600), end: now.addingTimeInterval(120), calendars: nil)
        for e in store.events(matching: pred) where !e.isAllDay && e.endDate > now {
            let text = [e.location, e.notes, e.url?.absoluteString].compactMap { $0 }.joined(separator: " ")
            guard DetectionLogic.looksLikeMeeting(text: text, attendeeCount: e.attendees?.count ?? 0),
                  let id = e.eventIdentifier else { continue }
            return (DetectedMeeting(title: e.title ?? "Meeting", source: "calendar",
                                    attendees: (e.attendees ?? []).compactMap { $0.name }), id)
        }
        return nil
    }

    // MARK: Microphone use

    /// Whether an app other than Muesli is capturing from a microphone.
    /// Returns nil when that cannot be told apart from Muesli's own capture.
    private func otherAppUsingMic(recording: Bool) -> Bool? {
        if #available(macOS 14.2, *), let pids = pidsCapturingInput() {
            let me = ProcessInfo.processInfo.processIdentifier
            return pids.contains { $0 != me }
        }
        return recording ? nil : defaultInputDeviceRunning()
    }

    /// Per-process view: every process that currently has an input stream running.
    @available(macOS 14.2, *)
    private func pidsCapturingInput() -> [pid_t]? {
        let system = AudioObjectID(kAudioObjectSystemObject)
        var addr = AudioObjectPropertyAddress(mSelector: kAudioHardwarePropertyProcessObjectList,
                                              mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
        var size: UInt32 = 0
        guard AudioObjectGetPropertyDataSize(system, &addr, 0, nil, &size) == noErr else { return nil }
        let count = Int(size) / MemoryLayout<AudioObjectID>.size
        guard count > 0 else { return [] }
        var processes = [AudioObjectID](repeating: 0, count: count)
        guard AudioObjectGetPropertyData(system, &addr, 0, nil, &size, &processes) == noErr else { return nil }
        let live = processes.prefix(Int(size) / MemoryLayout<AudioObjectID>.size)
        return live.compactMap { process -> pid_t? in
            guard uint32(process, kAudioProcessPropertyIsRunningInput) == 1 else { return nil }
            return int32(process, kAudioProcessPropertyPID)
        }
    }

    /// Device-wide view (older systems): true while anything, including Muesli, uses the default mic.
    private func defaultInputDeviceRunning() -> Bool {
        guard let dev = uint32(AudioObjectID(kAudioObjectSystemObject), kAudioHardwarePropertyDefaultInputDevice) else { return false }
        return uint32(dev, kAudioDevicePropertyDeviceIsRunningSomewhere).map { $0 != 0 } ?? false
    }

    private func uint32(_ object: AudioObjectID, _ selector: AudioObjectPropertySelector) -> UInt32? {
        var value: UInt32 = 0
        var size = UInt32(MemoryLayout<UInt32>.size)
        var addr = AudioObjectPropertyAddress(mSelector: selector, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
        return AudioObjectGetPropertyData(object, &addr, 0, nil, &size, &value) == noErr ? value : nil
    }

    private func int32(_ object: AudioObjectID, _ selector: AudioObjectPropertySelector) -> Int32? {
        var value: Int32 = 0
        var size = UInt32(MemoryLayout<Int32>.size)
        var addr = AudioObjectPropertyAddress(mSelector: selector, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
        return AudioObjectGetPropertyData(object, &addr, 0, nil, &size, &value) == noErr ? value : nil
    }
}
