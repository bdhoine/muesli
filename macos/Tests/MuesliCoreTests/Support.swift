import XCTest
@testable import MuesliCore

/// Gives every test its own Application Support and transcripts folders.
class IsolatedTestCase: XCTestCase {
    var dir: URL!
    var transcriptsDir: URL { dir.appendingPathComponent("transcripts") }

    override func setUp() {
        dir = FileManager.default.temporaryDirectory.appendingPathComponent("muesli-tests-\(UUID().uuidString)")
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        setenv("MUESLI_SUPPORT_DIR", dir.appendingPathComponent("support").path, 1)
        setenv("TRANSCRIPT_DIR", transcriptsDir.path, 1)
    }

    override func tearDown() {
        try? FileManager.default.removeItem(at: dir)
        unsetenv("MUESLI_SUPPORT_DIR")
        unsetenv("TRANSCRIPT_DIR")
    }

    func meeting(_ id: String, state: MeetingState, segments: Int = 2, title: String = "Weekly sync",
                 started: Date = Date(timeIntervalSince1970: 1_800_000_000)) -> Meeting {
        Meeting(id: id, title: title, source: "calendar", startedAt: started,
                endedAt: state == .ready ? started.addingTimeInterval(1800) : nil,
                attendees: ["Ann"],
                transcript: (0..<segments).map { TranscriptSegment(start: Double($0) * 5, end: Double($0) * 5 + 4, speaker: $0 % 2 == 0 ? "You" : "Others", text: "Segment \($0)") },
                state: state)
    }
}
