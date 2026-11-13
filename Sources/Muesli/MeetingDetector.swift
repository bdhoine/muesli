import AppKit
import CoreAudio
import EventKit

/// Combines three signals: calendar (EventKit, covers Outlook/Google accounts synced
/// into macOS Calendar), Teams process, and mic-in-use (CoreAudio).
final class MeetingDetector {
    var onDetect: ((DetectedMeeting) -> Void)?
    var isRecording: () -> Bool = { false }

    private let store = EKEventStore()
    private var calendarOK = false
    private var seenEvents = Set<String>()
    private var micHotSince: Date?
    private var lastAudioPrompt = Date.distantPast
    private var timer: Timer?

    private let teamsBundles = ["com.microsoft.teams2", "com.microsoft.teams"]
    private let meetingHosts = ["teams.microsoft.com", "teams.live.com", "zoom.us", "meet.google.com", "webex.com"]

    func start() {
        store.requestFullAccessToEvents { [weak self] ok, _ in self?.calendarOK = ok }
        timer = Timer.scheduledTimer(withTimeInterval: 5, repeats: true) { [weak self] _ in self?.tick() }
    }

    private func tick() {
        guard !isRecording() else { micHotSince = nil; return }
        if let m = checkCalendar() { onDetect?(m); return }
        if let m = checkAudio() { onDetect?(m) }
    }

    // 1. Calendar: events starting within +/- 2 minutes that look like meetings.
    private func checkCalendar() -> DetectedMeeting? {
        guard calendarOK else { return nil }
        let now = Date()
        let pred = store.predicateForEvents(withStart: now.addingTimeInterval(-120), end: now.addingTimeInterval(120), calendars: nil)
        for e in store.events(matching: pred) where !e.isAllDay && !seenEvents.contains(e.eventIdentifier) {
            let haystack = [e.location, e.notes, e.url?.absoluteString].compactMap { $0 }.joined(separator: " ").lowercased()
            let looksOnline = meetingHosts.contains { haystack.contains($0) }
            let hasOthers = (e.attendees?.count ?? 0) > 1
            guard looksOnline || hasOthers else { continue }
            seenEvents.insert(e.eventIdentifier)
            return DetectedMeeting(title: e.title ?? "Meeting", source: "calendar",
                                   attendees: (e.attendees ?? []).compactMap { $0.name })
        }
        return nil
    }

    // 2 + 3. Mic in use for 10+ s while not recording; Teams running upgrades the label.
    private func checkAudio() -> DetectedMeeting? {
        guard micInUse() else { micHotSince = nil; return nil }
        if micHotSince == nil { micHotSince = Date() }
        guard Date().timeIntervalSince(micHotSince!) > 10,
              Date().timeIntervalSince(lastAudioPrompt) > 600 else { return nil }
        lastAudioPrompt = Date()
        let teams = NSWorkspace.shared.runningApplications.contains { teamsBundles.contains($0.bundleIdentifier ?? "") }
        return DetectedMeeting(title: teams ? "Teams call" : "Call", source: teams ? "teams" : "audio")
    }

    private func micInUse() -> Bool {
        var dev = AudioObjectID(0)
        var size = UInt32(MemoryLayout<AudioObjectID>.size)
        var addr = AudioObjectPropertyAddress(mSelector: kAudioHardwarePropertyDefaultInputDevice,
                                              mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
        guard AudioObjectGetPropertyData(AudioObjectID(kAudioObjectSystemObject), &addr, 0, nil, &size, &dev) == noErr else { return false }
        var running: UInt32 = 0
        size = UInt32(MemoryLayout<UInt32>.size)
        addr = AudioObjectPropertyAddress(mSelector: kAudioDevicePropertyDeviceIsRunningSomewhere,
                                          mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
        guard AudioObjectGetPropertyData(dev, &addr, 0, nil, &size, &running) == noErr else { return false }
        return running != 0
    }
}
