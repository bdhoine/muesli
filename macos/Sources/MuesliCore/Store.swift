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
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        let base = folder.appendingPathComponent(fileStem(m))
        let md = base.appendingPathExtension("md")
        try? markdown(m).write(to: md, atomically: true, encoding: .utf8)
        try? JSONCoding.encoder().encode(m).write(to: base.appendingPathExtension("json"), options: .atomic)
        return md
    }

    public static func recent(limit: Int = 5) -> [URL] {
        let files = (try? FileManager.default.contentsOfDirectory(
            at: Paths.transcripts, includingPropertiesForKeys: [.contentModificationDateKey])) ?? []
        return files.filter { $0.pathExtension == "md" }
            .sorted { (mod($0)) > (mod($1)) }
            .prefix(limit).map { $0 }
    }

    public static func recentTranscripts(limit: Int) -> [RecentTranscript] {
        guard limit > 0 else { return [] }
        return recent(limit: limit).map { md in
            let json = md.deletingPathExtension().appendingPathExtension("json")
            let meeting = (try? Data(contentsOf: json)).flatMap { try? JSONCoding.decoder().decode(Meeting.self, from: $0) }
            return RecentTranscript(url: md, title: meeting?.title ?? md.deletingPathExtension().lastPathComponent,
                                    startedAt: meeting?.startedAt ?? mod(md), endedAt: meeting?.endedAt)
        }
    }

    private static func mod(_ u: URL) -> Date {
        (try? u.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate) ?? .distantPast
    }

    static func fileStem(_ m: Meeting) -> String {
        let f = DateFormatter()
        f.dateFormat = "yyyy-MM-dd_HHmm"
        let slug = m.title.unicodeScalars
            .map { CharacterSet.alphanumerics.contains($0) ? Character($0) : "-" }
            .reduce(into: "") { $0.append($1) }
            .split(separator: "-").joined(separator: "-")
        return "\(f.string(from: m.startedAt))_\(slug.isEmpty ? "Meeting" : String(slug.prefix(60)))"
    }

    /// Cuts a long title in the middle (`Weekly planning…Q3 review`). Menus grow with their text.
    public static func shortenTitle(_ title: String, limit: Int = 44) -> String {
        guard limit >= 3, title.count > limit else { return title }
        let head = (limit - 1) * 2 / 3, tail = limit - 1 - head
        return String(title.prefix(head)) + "…" + String(title.suffix(tail))
    }

    public static func markdown(_ m: Meeting) -> String {
        header(m) + "\n## Transcript\n\n" + lines(m.transcript)
    }

    static func header(_ m: Meeting) -> String {
        let iso = ISO8601DateFormatter()
        var out = "# \(m.title)\n\n"
        out += "- Started: \(iso.string(from: m.startedAt))\n"
        if let end = m.endedAt {
            out += "- Ended: \(iso.string(from: end))\n"
            out += "- Duration: \(Int(end.timeIntervalSince(m.startedAt) / 60)) min\n"
        }
        out += "- Source: \(m.source)\n"
        if !m.attendees.isEmpty { out += "- Attendees: \(m.attendees.joined(separator: ", "))\n" }
        if m.partial == true { out += "- Partial: recording was interrupted\n" }
        return out
    }

    static func line(_ s: TranscriptSegment) -> String {
        let t = Int(s.start)
        let mm = t / 60, ss = t % 60
        return "[\(mm < 10 ? "0" : "")\(mm):\(ss < 10 ? "0" : "")\(ss)] **\(s.speaker)**: \(s.text)\n\n"
    }

    static func lines(_ segments: [TranscriptSegment]) -> String {
        segments.map(line).joined()
    }
}

/// Canonical copy of every meeting, kept in Application Support so the MCP server
/// never depends on where the user's transcripts folder is. Rewritten during recording.
public enum MeetingStore {
    static func url(_ id: String) -> URL? {
        let ok = !id.isEmpty && id.count <= 64 && id.unicodeScalars.allSatisfy {
            CharacterSet.alphanumerics.contains($0) || $0 == "-"
        }
        return ok ? Paths.meetings.appendingPathComponent(id).appendingPathExtension("json") : nil
    }

    public static func save(_ m: Meeting) {
        guard let u = url(m.id) else { return }
        try? JSONCoding.encoder().encode(m).write(to: u, options: .atomic)
    }

    public static func load(_ id: String) -> Meeting? {
        guard let u = url(id), let data = try? Data(contentsOf: u) else { return nil }
        return try? JSONCoding.decoder().decode(Meeting.self, from: data)
    }

    /// All meetings, newest first.
    public static func all() -> [Meeting] {
        let files = (try? FileManager.default.contentsOfDirectory(at: Paths.meetings, includingPropertiesForKeys: nil)) ?? []
        return files.filter { $0.pathExtension == "json" }
            .compactMap { try? JSONCoding.decoder().decode(Meeting.self, from: Data(contentsOf: $0)) }
            .sorted { $0.startedAt > $1.startedAt }
    }

    /// A crash or forced quit mid-recording leaves a meeting in `recording` or `finalizing`.
    /// Keeps what was transcribed (marked partial, exported to the transcripts folder) and
    /// marks empty ones failed. Returns the meetings it changed.
    @discardableResult
    public static func recoverInterrupted() -> [Meeting] {
        var changed: [Meeting] = []
        for var m in all() where m.state == .recording || m.state == .finalizing {
            if m.transcript.isEmpty {
                m.state = .failed
            } else {
                m.transcript.sort { $0.start < $1.start }
                m.endedAt = m.endedAt ?? m.startedAt.addingTimeInterval(m.transcript.map(\.end).max() ?? 0)
                m.state = .ready
                m.partial = true
                Store.save(m)
            }
            save(m)
            changed.append(m)
        }
        return changed
    }

    @discardableResult
    public static func importTranscripts(from folder: URL = Paths.transcripts) -> [Meeting] {
        let files = (try? FileManager.default.contentsOfDirectory(at: folder, includingPropertiesForKeys: nil)) ?? []
        var imported: [Meeting] = []
        for file in files where file.pathExtension == "json" {
            guard let data = try? Data(contentsOf: file),
                  let m = try? JSONCoding.decoder().decode(Meeting.self, from: data),
                  let target = url(m.id),
                  !FileManager.default.fileExists(atPath: target.path) else { continue }
            save(m)
            imported.append(m)
        }
        return imported
    }

    /// Adds freshly transcribed segments to a meeting that is being recorded and saves it, so the
    /// transcript survives a crash and can be read while the meeting is still going on.
    public static func append(_ segments: [TranscriptSegment], to m: inout Meeting) {
        guard !segments.isEmpty else { return }
        m.transcript.append(contentsOf: segments)
        m.transcript.sort { $0.start < $1.start }
        save(m)
    }

    /// Ends a recording: marks the meeting ready, saves it and exports the transcript files.
    /// Returns the Markdown file.
    @discardableResult
    public static func finish(_ m: inout Meeting, now: Date = Date()) -> URL {
        m.endedAt = now
        m.transcript.sort { $0.start < $1.start }
        m.state = .ready
        save(m)
        return Store.save(m)
    }

    public static func latestReady() -> Meeting? {
        all().filter { $0.state == .ready }.max { ($0.endedAt ?? $0.startedAt) < ($1.endedAt ?? $1.startedAt) }
    }
}
