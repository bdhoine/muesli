import Foundation

/// Writes each finished meeting as a pair of files in the transcripts folder:
///   2026-11-10_1430_Weekly-sync.md    readable transcript
///   2026-11-10_1430_Weekly-sync.json  same data, machine-readable
/// Other tools can pick the files up from the folder.
public struct RecentTranscript: Identifiable, Equatable, Sendable {
    public var url: URL
    public var title: String
    public var startedAt: Date
    public var endedAt: Date?
    public var id: URL { url }

    public var durationMinutes: Int? {
        endedAt.map { max(Int($0.timeIntervalSince(startedAt) / 60), 0) }
    }

    init(_ record: RecentRecord) {
        self.init(url: URL(fileURLWithPath: record.path), title: record.title, startedAt: record.startedAt, endedAt: record.endedAt)
    }

    public init(url: URL, title: String, startedAt: Date, endedAt: Date?) {
        self.url = url; self.title = title; self.startedAt = startedAt; self.endedAt = endedAt
    }

    public func subtitle(now: Date = Date(), calendar: Calendar = .current, locale: Locale = .current) -> String {
        let time = startedAt.formatted(Date.FormatStyle(date: .omitted, time: .shortened).locale(locale))
        let day: String
        if calendar.isDate(startedAt, inSameDayAs: now) {
            day = "Today"
        } else if let yesterday = calendar.date(byAdding: .day, value: -1, to: now), calendar.isDate(startedAt, inSameDayAs: yesterday) {
            day = "Yesterday"
        } else {
            day = startedAt.formatted(Date.FormatStyle(date: .abbreviated, time: .omitted).locale(locale))
        }
        var parts = ["\(day) \(time)"]
        if let minutes = durationMinutes { parts.append(minutes < 1 ? "<1 min" : "\(minutes) min") }
        return parts.joined(separator: " · ")
    }
}

public enum Store {
    @discardableResult
    public static func save(_ m: Meeting, in folder: URL = Paths.transcripts) -> URL {
        URL(fileURLWithPath: storeSave(meeting: m, folder: folder.path))
    }

    public static func recent(limit: Int = 5) -> [URL] {
        storeRecent(folder: Paths.transcripts.path, limit: UInt32(max(limit, 0))).map { URL(fileURLWithPath: $0) }
    }

    public static func recentTranscripts(limit: Int) -> [RecentTranscript] {
        storeRecentTranscripts(folder: Paths.transcripts.path, limit: UInt32(max(limit, 0))).map(RecentTranscript.init)
    }

    static func fileStem(_ m: Meeting) -> String { storeFileStem(meeting: m) }

    /// Cuts a long title in the middle (`Weekly planning…Q3 review`). Menus grow with their text.
    public static func shortenTitle(_ title: String, limit: Int = 44) -> String {
        storeShortenTitle(title: title, limit: UInt32(max(limit, 0)))
    }

    public static func markdown(_ m: Meeting) -> String { meetingMarkdown(meeting: m) }
}

/// Canonical copy of every meeting, kept in Application Support so the MCP server
/// never depends on where the user's transcripts folder is. Rewritten during recording.
public enum MeetingStore {
    public static func save(_ m: Meeting) { meetingStoreSave(paths: Paths.current, meeting: m) }

    public static func load(_ id: String) -> Meeting? { meetingStoreLoad(paths: Paths.current, id: id) }

    /// All meetings, newest first.
    public static func all() -> [Meeting] { meetingStoreAll(paths: Paths.current) }

    /// A crash or forced quit mid-recording leaves a meeting in `recording` or `finalizing`.
    /// Keeps what was transcribed (marked partial, exported to the transcripts folder) and
    /// marks empty ones failed. Returns the meetings it changed.
    @discardableResult
    public static func recoverInterrupted() -> [Meeting] { meetingStoreRecoverInterrupted(paths: Paths.current) }

    @discardableResult
    public static func importTranscripts(from folder: URL = Paths.transcripts) -> [Meeting] {
        meetingStoreImportTranscripts(paths: Paths.current, folder: folder.path)
    }

    /// Adds freshly transcribed segments to a meeting that is being recorded and saves it, so the
    /// transcript survives a crash and can be read while the meeting is still going on.
    public static func append(_ segments: [TranscriptSegment], to m: inout Meeting) {
        m = meetingStoreAppend(paths: Paths.current, meeting: m, segments: segments)
    }

    /// Ends a recording: marks the meeting ready, saves it and exports the transcript files.
    /// Returns the Markdown file.
    @discardableResult
    public static func finish(_ m: inout Meeting, now: Date = Date()) -> URL {
        let done = meetingStoreFinish(paths: Paths.current, meeting: m, now: now)
        m = done.meeting
        return URL(fileURLWithPath: done.markdown)
    }

    public static func latestReady() -> Meeting? { meetingStoreLatestReady(paths: Paths.current) }
}
