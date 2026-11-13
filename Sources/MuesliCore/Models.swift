import Foundation

public enum MuesliInfo {
    public static let version = "0.2.0"
}

public enum MeetingState: String, Codable, Sendable {
    case detected, recording, finalizing, ready, failed
}

public struct TranscriptSegment: Codable, Sendable, Equatable {
    public var start: Double          // seconds from meeting start
    public var end: Double
    public var speaker: String        // "You" (mic) or "Others" (system audio)
    public var text: String

    public init(start: Double, end: Double, speaker: String, text: String) {
        self.start = start; self.end = end; self.speaker = speaker; self.text = text
    }
}

public struct Meeting: Codable, Identifiable, Sendable {
    public var id: String
    public var title: String
    public var source: String             // calendar | teams | audio | manual
    public var startedAt: Date
    public var endedAt: Date?
    public var attendees: [String]
    public var transcript: [TranscriptSegment]
    public var state: MeetingState
    /// True when the transcript was cut short (recovered after a crash).
    public var partial: Bool?

    public init(id: String = UUID().uuidString, title: String, source: String, startedAt: Date = Date(),
                endedAt: Date? = nil, attendees: [String] = [], transcript: [TranscriptSegment] = [],
                state: MeetingState = .recording, partial: Bool? = nil) {
        self.id = id; self.title = title; self.source = source; self.startedAt = startedAt
        self.endedAt = endedAt; self.attendees = attendees; self.transcript = transcript
        self.state = state; self.partial = partial
    }

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(String.self, forKey: .id)
        title = try c.decode(String.self, forKey: .title)
        source = try c.decode(String.self, forKey: .source)
        startedAt = try c.decode(Date.self, forKey: .startedAt)
        endedAt = try c.decodeIfPresent(Date.self, forKey: .endedAt)
        attendees = try c.decodeIfPresent([String].self, forKey: .attendees) ?? []
        transcript = try c.decodeIfPresent([TranscriptSegment].self, forKey: .transcript) ?? []
        state = try c.decodeIfPresent(MeetingState.self, forKey: .state) ?? .ready   // files written before states existed
        partial = try c.decodeIfPresent(Bool.self, forKey: .partial)
    }
}

public struct DetectedMeeting: Equatable, Sendable {
    public var title: String
    public var source: String
    public var attendees: [String]

    public init(title: String, source: String, attendees: [String] = []) {
        self.title = title; self.source = source; self.attendees = attendees
    }
}

/// Audio chunk (16 kHz mono Float32) from one stream.
public struct AudioChunk: Sendable {
    public var speaker: String
    public var offsetSeconds: Double
    public var samples: [Float]

    public init(speaker: String, offsetSeconds: Double, samples: [Float]) {
        self.speaker = speaker; self.offsetSeconds = offsetSeconds; self.samples = samples
    }
}

public enum Paths {
    /// App data: Whisper model, state.json, config.json, meetings/. MUESLI_SUPPORT_DIR overrides (tests).
    public static var support: URL {
        let u: URL
        if let custom = ProcessInfo.processInfo.environment["MUESLI_SUPPORT_DIR"] {
            u = URL(fileURLWithPath: custom, isDirectory: true)
        } else {
            u = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
                .appendingPathComponent("Muesli", isDirectory: true)
        }
        try? FileManager.default.createDirectory(at: u, withIntermediateDirectories: true)
        return u
    }
    public static var model: URL { support.appendingPathComponent("ggml-small.bin") }
    public static var state: URL { support.appendingPathComponent("state.json") }
    public static var config: URL { support.appendingPathComponent("config.json") }
    public static var auditLog: URL { support.appendingPathComponent("audit.log") }
    public static var meetings: URL {
        let u = support.appendingPathComponent("meetings", isDirectory: true)
        try? FileManager.default.createDirectory(at: u, withIntermediateDirectories: true)
        return u
    }

    public static let transcriptDirKey = "transcriptDir"

    public static var transcriptsOverriddenByEnvironment: Bool {
        ProcessInfo.processInfo.environment["TRANSCRIPT_DIR"] != nil
    }

    public static var transcriptsIsCustom: Bool {
        UserDefaults.standard.string(forKey: transcriptDirKey) != nil
    }

    public static func setTranscripts(_ url: URL?) {
        UserDefaults.standard.set(url?.path, forKey: transcriptDirKey)
    }

    /// Where transcripts are written. Override with TRANSCRIPT_DIR.
    public static var transcripts: URL {
        let u: URL
        if let custom = ProcessInfo.processInfo.environment["TRANSCRIPT_DIR"] {
            u = URL(fileURLWithPath: (custom as NSString).expandingTildeInPath, isDirectory: true)
        } else if let saved = UserDefaults.standard.string(forKey: transcriptDirKey) {
            u = URL(fileURLWithPath: saved, isDirectory: true)
        } else {
            u = FileManager.default.homeDirectoryForCurrentUser
                .appendingPathComponent("Documents/MeetingTranscripts", isDirectory: true)
        }
        try? FileManager.default.createDirectory(at: u, withIntermediateDirectories: true)
        return u
    }
}

enum JSONCoding {
    static func encoder(pretty: Bool = true) -> JSONEncoder {
        let e = JSONEncoder()
        e.dateEncodingStrategy = .iso8601
        e.outputFormatting = pretty ? [.prettyPrinted, .sortedKeys] : [.sortedKeys]
        return e
    }

    static func decoder() -> JSONDecoder {
        let d = JSONDecoder()
        d.dateDecodingStrategy = .iso8601
        return d
    }
}
