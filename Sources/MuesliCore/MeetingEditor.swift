import Foundation

public enum MeetingEditor {
    public enum EditError: Error, Equatable {
        case notFound
        case busy(MeetingState)
        case invalid(String)

        public var message: String {
            switch self {
            case .notFound: "No meeting with that id. Use list_transcripts to find ids."
            case .busy(let state): "This meeting is still \(state.rawValue). Try again once it is ready."
            case .invalid(let reason): reason
            }
        }
    }

    public static let maxTitleLength = 200
    public static let maxSpeakerLength = 60
    public static let maxAttendees = 100

    nonisolated(unsafe) public static var discard: (URL) -> Void = { url in
        guard FileManager.default.fileExists(atPath: url.path) else { return }
        #if os(macOS)
        if (try? FileManager.default.trashItem(at: url, resultingItemURL: nil)) != nil { return }
        #endif
        try? FileManager.default.removeItem(at: url)
    }

    public static func rename(_ id: String, to title: String, folder: URL = Paths.exportFolder) -> Result<Meeting, EditError> {
        let clean = singleLine(title)
        guard !clean.isEmpty else { return .failure(.invalid("The title cannot be empty.")) }
        guard clean.count <= maxTitleLength else { return .failure(.invalid("The title is longer than \(maxTitleLength) characters.")) }
        return edit(id, folder: folder) { $0.title = clean; return nil }
    }

    public static func setAttendees(_ id: String, to attendees: [String], folder: URL = Paths.exportFolder) -> Result<Meeting, EditError> {
        var seen = Set<String>()
        let clean = attendees.map(singleLine).filter { !$0.isEmpty && seen.insert($0.lowercased()).inserted }
        guard clean.count <= maxAttendees else { return .failure(.invalid("At most \(maxAttendees) attendees.")) }
        guard clean.allSatisfy({ $0.count <= maxTitleLength }) else { return .failure(.invalid("An attendee name is longer than \(maxTitleLength) characters.")) }
        return edit(id, folder: folder) { $0.attendees = clean; return nil }
    }

    public static func renameSpeaker(_ id: String, from old: String, to new: String, folder: URL = Paths.exportFolder) -> Result<Meeting, EditError> {
        let clean = singleLine(new)
        guard !clean.isEmpty else { return .failure(.invalid("The new speaker name cannot be empty.")) }
        guard clean.count <= maxSpeakerLength else { return .failure(.invalid("The speaker name is longer than \(maxSpeakerLength) characters.")) }
        return edit(id, folder: folder) { m in
            let speakers = Set(m.transcript.map(\.speaker))
            guard speakers.contains(old) else {
                return .invalid("No speaker called \"\(old)\" in this transcript. Speakers: \(speakers.sorted().joined(separator: ", ")).")
            }
            for i in m.transcript.indices where m.transcript[i].speaker == old { m.transcript[i].speaker = clean }
            return nil
        }
    }

    public static func delete(_ id: String, folder: URL = Paths.exportFolder) -> Result<Meeting, EditError> {
        guard let m = MeetingStore.load(id) else { return .failure(.notFound) }
        if let busy = busyState(m) { return .failure(.busy(busy)) }
        let stem = folder.appendingPathComponent(Store.fileStem(m))
        if ownedBy(m.id, stem: stem) {
            discard(stem.appendingPathExtension("md"))
            discard(stem.appendingPathExtension("json"))
        }
        if let stored = MeetingStore.url(m.id) { discard(stored) }
        return .success(m)
    }

    static func edit(_ id: String, folder: URL, _ change: (inout Meeting) -> EditError?) -> Result<Meeting, EditError> {
        guard var m = MeetingStore.load(id) else { return .failure(.notFound) }
        if let busy = busyState(m) { return .failure(.busy(busy)) }
        let old = m
        if let error = change(&m) { return .failure(error) }

        let oldStem = folder.appendingPathComponent(Store.fileStem(old))
        let newStem = folder.appendingPathComponent(Store.fileStem(m))
        if newStem != oldStem && !ownedBy(m.id, stem: newStem) && exists(newStem) {
            return .failure(.invalid("Another transcript that started in the same minute already has that title."))
        }

        MeetingStore.save(m)
        if m.state == .ready {
            if newStem != oldStem && ownedBy(m.id, stem: oldStem) {
                try? FileManager.default.removeItem(at: oldStem.appendingPathExtension("md"))
                try? FileManager.default.removeItem(at: oldStem.appendingPathExtension("json"))
            }
            Store.save(m, in: folder)
        }
        return .success(m)
    }

    private static func busyState(_ m: Meeting) -> MeetingState? {
        m.state == .recording || m.state == .finalizing || m.state == .detected ? m.state : nil
    }

    private static func exists(_ stem: URL) -> Bool {
        FileManager.default.fileExists(atPath: stem.appendingPathExtension("md").path)
            || FileManager.default.fileExists(atPath: stem.appendingPathExtension("json").path)
    }

    private static func ownedBy(_ id: String, stem: URL) -> Bool {
        guard let data = try? Data(contentsOf: stem.appendingPathExtension("json")),
              let other = try? JSONCoding.decoder().decode(Meeting.self, from: data) else { return false }
        return other.id == id
    }

    static func singleLine(_ s: String) -> String {
        s.components(separatedBy: .newlines).joined(separator: " ").trimmingCharacters(in: .whitespaces)
    }
}
