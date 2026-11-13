import Foundation

/// The decisions behind meeting detection, free of EventKit and CoreAudio so they can be tested.
/// `MeetingDetector` in the app samples the system every few seconds and feeds `tick`.
public struct DetectionLogic {
    public enum Event: Equatable {
        /// A meeting just started and is not being recorded: suggest transcribing.
        case suggest(DetectedMeeting)
        /// `inProgress` changed.
        case inProgressChanged
        /// The call being recorded seems to be over.
        case callEnded
    }

    public static let meetingHosts = ["teams.microsoft.com", "teams.live.com", "zoom.us", "meet.google.com", "webex.com"]

    /// How long other apps must have used the mic before it counts as a call.
    public var micWarmup: TimeInterval = 10
    /// Minimum time between two suggestions that come from mic activity alone.
    public var audioPromptCooldown: TimeInterval = 600
    /// How long the mic must stay released before a recorded call counts as ended.
    public var endDelay: TimeInterval = 60

    /// The meeting or call going on right now, whether or not it is recorded.
    public private(set) var inProgress: (meeting: DetectedMeeting, since: Date)?

    private var seenEvents = Set<String>()
    private var micHotSince: Date?
    private var lastAudioPrompt = Date.distantPast
    private var callHeardWhileRecording = false
    private var micQuietSince: Date?
    private var endReported = false

    public init() {}

    /// A calendar event counts as a meeting when it links to a known meeting service or has other attendees.
    public static func looksLikeMeeting(text: String, attendeeCount: Int) -> Bool {
        let haystack = text.lowercased()
        return meetingHosts.contains { haystack.contains($0) } || attendeeCount > 1
    }

    /// - Parameters:
    ///   - calendar: the calendar meeting running now, if any.
    ///   - micBusy: whether an app other than Muesli is using a microphone; nil when that cannot be told.
    public mutating func tick(now: Date, recording: Bool, calendar: (meeting: DetectedMeeting, eventID: String)?,
                              micBusy: Bool?, teamsRunning: Bool) -> [Event] {
        var events: [Event] = []

        if micBusy == true {
            if micHotSince == nil { micHotSince = now }
        } else if micBusy == false {
            micHotSince = nil
        }
        let callActive = micHotSince.map { now.timeIntervalSince($0) > micWarmup } ?? false
        let audio = DetectedMeeting(title: teamsRunning ? "Teams call" : "Call", source: teamsRunning ? "teams" : "audio")

        // What is going on right now. The calendar gives the better title.
        let current: DetectedMeeting? = calendar?.meeting ?? (callActive ? audio : nil)
        if current != inProgress?.meeting {
            inProgress = current.map { (meeting: $0, since: now) }
            events.append(.inProgressChanged)
        }

        if recording {
            // Only recordings in which another app used the mic can "end"; an in-person
            // meeting recorded manually never triggers this.
            if let micBusy {
                if micBusy {
                    callHeardWhileRecording = true
                    micQuietSince = nil
                    endReported = false
                } else if callHeardWhileRecording {
                    if micQuietSince == nil { micQuietSince = now }
                    if !endReported, now.timeIntervalSince(micQuietSince!) >= endDelay {
                        endReported = true
                        events.append(.callEnded)
                    }
                }
            }
            return events
        }
        callHeardWhileRecording = false; micQuietSince = nil; endReported = false

        if let calendar {
            if !seenEvents.contains(calendar.eventID) {
                seenEvents.insert(calendar.eventID)
                events.append(.suggest(calendar.meeting))
            }
        } else if callActive, now.timeIntervalSince(lastAudioPrompt) > audioPromptCooldown {
            lastAudioPrompt = now
            events.append(.suggest(audio))
        }
        return events
    }
}
