import Foundation

/// Writes each meeting as a pair of files in the transcripts folder:
///   2026-11-10_1430_Weekly-sync.md    readable transcript
///   2026-11-10_1430_Weekly-sync.json  same data, machine-readable
/// Nothing else is produced; other tools can pick the files up from the folder.
enum Store {
    @discardableResult
    static func save(_ m: Meeting) -> URL {
        let base = Paths.transcripts.appendingPathComponent(fileStem(m))
        let md = base.appendingPathExtension("md")
        try? markdown(m).write(to: md, atomically: true, encoding: .utf8)

        let e = JSONEncoder()
        e.dateEncodingStrategy = .iso8601
        e.outputFormatting = [.prettyPrinted, .sortedKeys]
        try? e.encode(m).write(to: base.appendingPathExtension("json"))
        return md
    }

    static func recent(limit: Int = 5) -> [URL] {
        let files = (try? FileManager.default.contentsOfDirectory(
            at: Paths.transcripts, includingPropertiesForKeys: [.contentModificationDateKey])) ?? []
        return files.filter { $0.pathExtension == "md" }
            .sorted { (mod($0)) > (mod($1)) }
            .prefix(limit).map { $0 }
    }

    private static func mod(_ u: URL) -> Date {
        (try? u.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate) ?? .distantPast
    }

    private static func fileStem(_ m: Meeting) -> String {
        let f = DateFormatter()
        f.dateFormat = "yyyy-MM-dd_HHmm"
        let slug = m.title.unicodeScalars
            .map { CharacterSet.alphanumerics.contains($0) ? Character($0) : "-" }
            .reduce(into: "") { $0.append($1) }
            .split(separator: "-").joined(separator: "-")
        return "\(f.string(from: m.startedAt))_\(slug.isEmpty ? "Meeting" : String(slug.prefix(60)))"
    }

    private static func markdown(_ m: Meeting) -> String {
        let iso = ISO8601DateFormatter()
        var out = "# \(m.title)\n\n"
        out += "- Started: \(iso.string(from: m.startedAt))\n"
        if let end = m.endedAt {
            out += "- Ended: \(iso.string(from: end))\n"
            out += "- Duration: \(Int(end.timeIntervalSince(m.startedAt) / 60)) min\n"
        }
        out += "- Source: \(m.source)\n"
        if !m.attendees.isEmpty { out += "- Attendees: \(m.attendees.joined(separator: ", "))\n" }
        out += "\n## Transcript\n\n"
        for s in m.transcript {
            let t = Int(s.start)
            out += String(format: "[%02d:%02d] **%@**: %@\n\n", t / 60, t % 60, s.speaker, s.text)
        }
        return out
    }
}
