import XCTest
@testable import MuesliCore

/// Logic the app and the Tauri port share: see the matching tests in app/crates/core/tests.
final class RecordingLifecycleTests: IsolatedTestCase {
    func testAppendingAndFinishingARecording() throws {
        var m = meeting("live", state: .recording, segments: 0, title: "Live call")
        MeetingStore.append([TranscriptSegment(start: 30, end: 31, speaker: "Others", text: "b")], to: &m)
        MeetingStore.append([TranscriptSegment(start: 0, end: 1, speaker: "You", text: "a")], to: &m)
        MeetingStore.append([], to: &m)
        XCTAssertEqual(MeetingStore.load("live")?.transcript.map(\.text), ["a", "b"])

        let end = m.startedAt.addingTimeInterval(720)
        let md = MeetingStore.finish(&m, now: end)
        XCTAssertTrue(FileManager.default.fileExists(atPath: md.path))
        let saved = try XCTUnwrap(MeetingStore.load("live"))
        XCTAssertEqual(saved.state, .ready)
        XCTAssertEqual(saved.endedAt, end)
        XCTAssertEqual(MeetingStore.latestReady()?.id, "live")
    }

    func testLongTitlesAreCutInTheMiddle() {
        XCTAssertEqual(Store.shortenTitle("Short"), "Short")
        let long = Store.shortenTitle("2026-11-16_1352_AEM-header-footer-via-content-domain-service.summary")
        XCTAssertEqual(long.count, 44)
        XCTAssertTrue(long.hasPrefix("2026-11-16_1352_AEM-header-f"))
        XCTAssertTrue(long.hasSuffix("ervice.summary"))
        XCTAssertTrue(long.contains("…"))
    }

    func testModelFilesAreInstalledOnlyWhenLargeEnough() throws {
        let file = Paths.support.appendingPathComponent(SpeechModel.info.fileName)
        XCTAssertFalse(SpeechModel.file(file, isAtLeast: 1))
        try Data("tiny".utf8).write(to: file)
        XCTAssertFalse(SpeechModel.file(file, isAtLeast: Int64(SpeechModel.info.approximateBytes / 10 * 9)))
        XCTAssertTrue(SpeechModel.file(file, isAtLeast: 1))
        XCTAssertFalse(SpeechModel.file(dir.appendingPathComponent("nope"), isAtLeast: 0))
    }

    func testTheClaudeConfigEntryFollowsThePreferences() throws {
        XCTAssertTrue(ClaudeDesktopConfig.wanted(mcpEnabled: true, configEntry: true, extensionInstalled: false))
        XCTAssertFalse(ClaudeDesktopConfig.wanted(mcpEnabled: true, configEntry: true, extensionInstalled: true))
        XCTAssertFalse(ClaudeDesktopConfig.wanted(mcpEnabled: false, configEntry: true, extensionInstalled: false))
        XCTAssertFalse(ClaudeDesktopConfig.wanted(mcpEnabled: true, configEntry: false, extensionInstalled: false))

        let claude = dir.appendingPathComponent("Claude")
        try FileManager.default.createDirectory(at: claude, withIntermediateDirectories: true)
        let helper = URL(fileURLWithPath: "/Applications/Muesli.app/Contents/MacOS/muesli-mcp")
        var p = Preferences()
        p.mcpEnabled = true
        p.claudeDesktopConfigEntry = true
        XCTAssertTrue(ClaudeDesktopConfig.sync(p, helper: helper, extensionInstalled: false, directory: claude).hasPrefix("Registered"))
        XCTAssertTrue(ClaudeDesktopConfig.sync(p, helper: helper, extensionInstalled: true, directory: claude).hasPrefix("Removed"))
        XCTAssertEqual(ClaudeDesktopConfig.sync(p, helper: helper, extensionInstalled: true, directory: claude), "Off")
    }
}
