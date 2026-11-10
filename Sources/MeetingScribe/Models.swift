import Foundation

struct TranscriptSegment: Codable {
    var start: Double          // seconds from meeting start
    var end: Double
    var speaker: String        // "You" (mic) or "Others" (system audio)
    var text: String
}

struct Meeting: Codable, Identifiable {
    var id: String = UUID().uuidString
    var title: String
    var source: String             // calendar | teams | audio | manual
    var startedAt: Date = Date()
    var endedAt: Date?
    var attendees: [String] = []
    var transcript: [TranscriptSegment] = []
}

struct DetectedMeeting: Equatable {
    var title: String
    var source: String
    var attendees: [String] = []
}

/// Audio chunk (16 kHz mono Float32) from one stream.
struct AudioChunk {
    var speaker: String
    var offsetSeconds: Double
    var samples: [Float]
}

enum Paths {
    /// Where the Whisper model lives.
    static let support: URL = {
        let u = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("MeetingScribe", isDirectory: true)
        try? FileManager.default.createDirectory(at: u, withIntermediateDirectories: true)
        return u
    }()
    static var model: URL { support.appendingPathComponent("ggml-small.bin") }

    /// Where transcripts are written. Override with TRANSCRIPT_DIR.
    static var transcripts: URL {
        let u: URL
        if let custom = ProcessInfo.processInfo.environment["TRANSCRIPT_DIR"] {
            u = URL(fileURLWithPath: (custom as NSString).expandingTildeInPath, isDirectory: true)
        } else {
            u = FileManager.default.homeDirectoryForCurrentUser
                .appendingPathComponent("Documents/MeetingTranscripts", isDirectory: true)
        }
        try? FileManager.default.createDirectory(at: u, withIntermediateDirectories: true)
        return u
    }
}
