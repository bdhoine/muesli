import Foundation

/// The decisions behind meeting detection, free of EventKit and CoreAudio so they can be tested.
/// The rules live in the Rust core; `MeetingDetector` in the app samples the system every few
/// seconds and feeds `tick`. Copies of this value share one state, so keep a single instance.
public struct DetectionLogic {
    public typealias Event = DetectionEvent

    public static var meetingHosts: [String] { detectionMeetingHosts() }

    private let core = Detector()

    /// How long other apps must have used the mic before it counts as a call.
    public var micWarmup: TimeInterval {
        get { core.micWarmup() }
        set { core.setMicWarmup(seconds: newValue) }
    }
    /// Minimum time between two suggestions that come from mic activity alone.
    public var audioPromptCooldown: TimeInterval {
        get { core.audioPromptCooldown() }
        set { core.setAudioPromptCooldown(seconds: newValue) }
    }
    /// How long the mic must stay released before a recorded call counts as ended.
    public var endDelay: TimeInterval {
        get { core.endDelay() }
        set { core.setEndDelay(seconds: newValue) }
    }

    /// The meeting or call going on right now, whether or not it is recorded.
    public var inProgress: (meeting: DetectedMeeting, since: Date)? {
        core.inProgress().map { (meeting: $0.meeting, since: $0.since) }
    }

    public init() {}

    /// A calendar event counts as a meeting when it links to a known meeting service or has other attendees.
    public static func looksLikeMeeting(text: String, attendeeCount: Int) -> Bool {
        detectionLooksLikeMeeting(text: text, attendeeCount: UInt32(max(attendeeCount, 0)))
    }

    /// - Parameters:
    ///   - calendar: the calendar meeting running now, if any.
    ///   - micBusy: whether an app other than Muesli is using a microphone; nil when that cannot be told.
    public mutating func tick(now: Date, recording: Bool, calendar: (meeting: DetectedMeeting, eventID: String)?,
                              micBusy: Bool?, teamsRunning: Bool) -> [Event] {
        core.tick(now: now, recording: recording,
                  calendar: calendar.map { CalendarMeeting(meeting: $0.meeting, eventId: $0.eventID) },
                  micBusy: micBusy, teamsRunning: teamsRunning)
    }
}
