import XCTest
@testable import MuesliCore

/// New features around the recording lifecycle and the Claude connection.
final class IntegrationTests: IsolatedTestCase {

    // MARK: Meeting store and lifecycle

    func testStoreRoundTripAndRewriteDuringRecording() {
        var m = meeting("live", state: .recording, segments: 1)
        MeetingStore.save(m)
        XCTAssertEqual(MeetingStore.load("live")?.transcript.count, 1)
        m.transcript.append(TranscriptSegment(start: 30, end: 34, speaker: "Others", text: "More"))
        MeetingStore.save(m)                                       // same file, rewritten
        XCTAssertEqual(MeetingStore.load("live")?.transcript.count, 2)
        XCTAssertEqual(MeetingStore.all().count, 1)
    }

    func testStoreIgnoresForeignFilesAndUnsafeIDs() throws {
        try Data("not json".utf8).write(to: Paths.meetings.appendingPathComponent("broken.json"))
        try Data("x".utf8).write(to: Paths.meetings.appendingPathComponent("notes.txt"))
        MeetingStore.save(meeting("ok", state: .ready))
        XCTAssertEqual(MeetingStore.all().map(\.id), ["ok"])

        MeetingStore.save(meeting("../escape", state: .ready))
        XCTAssertFalse(FileManager.default.fileExists(atPath: Paths.support.appendingPathComponent("escape.json").path))
        XCTAssertNil(MeetingStore.load("../config"))
        XCTAssertNil(MeetingStore.load(""))
        XCTAssertNotNil(MeetingStore.load("ok"))
    }

    func testNewMeetingIDsAreAcceptedByTheStore() {
        let m = Meeting(title: "t", source: "manual")
        XCTAssertEqual(m.state, .recording)
        MeetingStore.save(m)
        XCTAssertNotNil(MeetingStore.load(m.id))
    }

    func testLatestReadyPicksTheLastEnded() {
        let t0 = Date(timeIntervalSince1970: 1_800_000_000)
        MeetingStore.save(meeting("a", state: .ready, started: t0))
        MeetingStore.save(meeting("b", state: .ready, started: t0.addingTimeInterval(7200)))
        MeetingStore.save(meeting("c", state: .recording, started: t0.addingTimeInterval(9000)))
        MeetingStore.save(meeting("d", state: .failed, started: t0.addingTimeInterval(9500)))
        XCTAssertEqual(MeetingStore.latestReady()?.id, "b")
    }

    func testTranscriptsWrittenBeforeTheStoreExistedAreImported() throws {
        try FileManager.default.createDirectory(at: transcriptsDir, withIntermediateDirectories: true)
        let legacy = """
        {"id":"FBD41FE0-CBBD-47C9-A89B-69F86F42FB55","title":"Meeting","source":"manual",
         "startedAt":"2026-11-13T15:32:30Z","endedAt":"2026-11-13T15:32:56Z","attendees":[],
         "transcript":[{"start":0,"end":2,"speaker":"You","text":"Hello"}]}
        """
        try Data(legacy.utf8).write(to: transcriptsDir.appendingPathComponent("2026-11-13_1532_Meeting.json"))
        try Data("not json".utf8).write(to: transcriptsDir.appendingPathComponent("broken.json"))
        try Data("# Meeting".utf8).write(to: transcriptsDir.appendingPathComponent("2026-11-13_1532_Meeting.md"))

        XCTAssertEqual(MeetingStore.importTranscripts().map(\.id), ["FBD41FE0-CBBD-47C9-A89B-69F86F42FB55"])
        let m = try XCTUnwrap(MeetingStore.load("FBD41FE0-CBBD-47C9-A89B-69F86F42FB55"))
        XCTAssertEqual(m.state, .ready)
        XCTAssertEqual(m.transcript.count, 1)
        XCTAssertEqual(MeetingStore.all().count, 1)
    }

    func testImportNeverOverwritesTheStore() throws {
        var stored = meeting("same", state: .ready, segments: 3)
        MeetingStore.save(stored)
        stored.transcript = []
        Store.save(stored)
        XCTAssertTrue(MeetingStore.importTranscripts().isEmpty)
        XCTAssertEqual(MeetingStore.load("same")?.transcript.count, 3)
    }

    // MARK: Crash recovery

    func testInterruptedRecordingIsSavedAsPartial() throws {
        var m = meeting("crashed", state: .recording, segments: 0, title: "Crashed call")
        m.transcript = [TranscriptSegment(start: 30, end: 58, speaker: "Others", text: "Later"),
                        TranscriptSegment(start: 0, end: 20, speaker: "You", text: "Earlier")]
        MeetingStore.save(m)
        MeetingStore.save(meeting("fine", state: .ready))

        let changed = MeetingStore.recoverInterrupted()
        XCTAssertEqual(changed.map(\.id), ["crashed"])
        let back = try XCTUnwrap(MeetingStore.load("crashed"))
        XCTAssertEqual(back.state, .ready)
        XCTAssertEqual(back.partial, true)
        XCTAssertEqual(back.transcript.map(\.text), ["Earlier", "Later"])
        XCTAssertEqual(back.endedAt, m.startedAt.addingTimeInterval(58))          // end of the last transcribed segment

        let exported = Store.recent().filter { $0.lastPathComponent.contains("Crashed-call") }
        XCTAssertEqual(exported.count, 1)                                          // also lands in the transcripts folder
        XCTAssertTrue(try String(contentsOf: exported[0], encoding: .utf8).contains("- Partial: recording was interrupted"))
        XCTAssertNil(MeetingStore.load("fine")?.partial)
    }

    func testInterruptedRecordingWithNothingTranscribedFails() {
        MeetingStore.save(meeting("empty", state: .finalizing, segments: 0))
        MeetingStore.recoverInterrupted()
        XCTAssertEqual(MeetingStore.load("empty")?.state, .failed)
        XCTAssertTrue(Store.recent().isEmpty)
    }

    func testRecoveryIsIdempotent() {
        MeetingStore.save(meeting("crashed", state: .recording))
        XCTAssertEqual(MeetingStore.recoverInterrupted().count, 1)
        XCTAssertEqual(MeetingStore.recoverInterrupted().count, 0)
        XCTAssertEqual(Store.recent().count, 1)
    }

    // MARK: state.json heartbeat

    func testStatusFileRoundTrip() throws {
        XCTAssertNil(AppStatus.read())
        AppStatus(appState: "error", lastReady: .init(id: "m", title: "t", endedAt: nil), error: "Whisper model missing").write()
        let s = try XCTUnwrap(AppStatus.read())
        XCTAssertEqual(s.schemaVersion, 1)
        XCTAssertEqual(s.appState, "error")
        XCTAssertEqual(s.error, "Whisper model missing")
        XCTAssertEqual(s.lastReady?.id, "m")
        XCTAssertEqual(s.pid, ProcessInfo.processInfo.processIdentifier)
        XCTAssertNil(s.current)
    }

    func testHeartbeatFreshnessWindow() {
        var s = AppStatus(appState: "idle")
        let now = Date()
        s.updatedAt = now.addingTimeInterval(-29)
        XCTAssertTrue(s.isFresh(now: now))
        s.updatedAt = now.addingTimeInterval(-30)
        XCTAssertFalse(s.isFresh(now: now))
    }

    func testCorruptStatusFileReadsAsNotRunning() throws {
        try Data("{".utf8).write(to: Paths.state)
        XCTAssertNil(AppStatus.read())
        XCTAssertFalse(Tools.status().appRunning)
    }

    // MARK: Preferences

    func testPreferencesDefaultsAreSafe() {
        let p = Preferences()
        XCTAssertFalse(p.mcpEnabled)                               // nothing is exposed until switched on
        XCTAssertFalse(p.autoStop)
        XCTAssertEqual(p.endDelaySeconds, 60)
    }

    func testCorruptPreferencesFallBackToDefaults() throws {
        try Data("garbage".utf8).write(to: Paths.config)
        XCTAssertEqual(Preferences.load(), Preferences())
    }

    // MARK: Claude Desktop registration

    private var claudeDir: URL { dir.appendingPathComponent("Claude") }
    private var claudeFile: URL { claudeDir.appendingPathComponent("claude_desktop_config.json") }
    private let helper = URL(fileURLWithPath: "/Applications/Muesli.app/Contents/MacOS/muesli-mcp")

    private func claudeConfig() throws -> [String: Any] {
        try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: claudeFile)) as? [String: Any])
    }

    func testRegisterNeedsClaudeDesktop() {
        XCTAssertTrue(ClaudeDesktopConfig.register(helper, directory: claudeDir).contains("not found"))
        XCTAssertFalse(FileManager.default.fileExists(atPath: claudeDir.path))     // does not create Claude's folder
    }

    func testRegisterCreatesTheFileWhenMissing() throws {
        try FileManager.default.createDirectory(at: claudeDir, withIntermediateDirectories: true)
        XCTAssertTrue(ClaudeDesktopConfig.register(helper, directory: claudeDir).hasPrefix("Registered"))
        let entry = try XCTUnwrap((claudeConfig()["mcpServers"] as? [String: Any])?["Muesli"] as? [String: Any])
        XCTAssertEqual(entry["command"] as? String, helper.path)
        XCTAssertEqual((entry["args"] as? [Any])?.count, 0)
    }

    func testRegisterAndUnregisterLeaveEverythingElseAlone() throws {
        try FileManager.default.createDirectory(at: claudeDir, withIntermediateDirectories: true)
        let existing = #"{"globalShortcut":"Alt+Space","mcpServers":{"filesystem":{"command":"npx","args":["-y","server-filesystem","/Users/me"],"env":{"A":"1"}}}}"#
        try Data(existing.utf8).write(to: claudeFile)
        let before = try claudeConfig()

        _ = ClaudeDesktopConfig.register(helper, directory: claudeDir)
        var servers = try XCTUnwrap(claudeConfig()["mcpServers"] as? [String: Any])
        XCTAssertEqual(Set(servers.keys), ["filesystem", "Muesli"])
        XCTAssertEqual(try claudeConfig()["globalShortcut"] as? String, "Alt+Space")
        XCTAssertEqual(ClaudeDesktopConfig.register(helper, directory: claudeDir), "Registered with Claude Desktop.")   // already there

        XCTAssertTrue(ClaudeDesktopConfig.unregister(directory: claudeDir).hasPrefix("Removed"))
        servers = try XCTUnwrap(claudeConfig()["mcpServers"] as? [String: Any])
        XCTAssertEqual(Set(servers.keys), ["filesystem"])
        XCTAssertEqual(try claudeConfig() as NSDictionary, before as NSDictionary)                                      // byte-for-byte the same data
        XCTAssertEqual(ClaudeDesktopConfig.unregister(directory: claudeDir), "Off")
    }

    func testRegisterUpdatesAMovedApp() throws {
        try FileManager.default.createDirectory(at: claudeDir, withIntermediateDirectories: true)
        _ = ClaudeDesktopConfig.register(URL(fileURLWithPath: "/old/muesli-mcp"), directory: claudeDir)
        XCTAssertTrue(ClaudeDesktopConfig.register(helper, directory: claudeDir).contains("Quit and reopen"))
        let entry = try XCTUnwrap((claudeConfig()["mcpServers"] as? [String: Any])?["Muesli"] as? [String: Any])
        XCTAssertEqual(entry["command"] as? String, helper.path)
    }

    func testTheOldLowercaseEntryIsReplacedAndRemoved() throws {
        try FileManager.default.createDirectory(at: claudeDir, withIntermediateDirectories: true)
        try Data(#"{"mcpServers":{"muesli":{"command":"/old/muesli-mcp","args":[]}}}"#.utf8).write(to: claudeFile)
        XCTAssertTrue(ClaudeDesktopConfig.register(helper, directory: claudeDir).hasPrefix("Registered"))
        XCTAssertEqual(Set(try XCTUnwrap(claudeConfig()["mcpServers"] as? [String: Any]).keys), ["Muesli"])

        try Data(#"{"mcpServers":{"muesli":{"command":"/old/muesli-mcp","args":[]}}}"#.utf8).write(to: claudeFile)
        XCTAssertTrue(ClaudeDesktopConfig.unregister(directory: claudeDir).hasPrefix("Removed"))
        XCTAssertEqual(try XCTUnwrap(claudeConfig()["mcpServers"] as? [String: Any]).count, 0)
    }

    func testUnparseableClaudeConfigIsNeverOverwritten() throws {
        try FileManager.default.createDirectory(at: claudeDir, withIntermediateDirectories: true)
        let broken = Data(#"{"mcpServers": { oops"#.utf8)
        try broken.write(to: claudeFile)
        XCTAssertTrue(ClaudeDesktopConfig.register(helper, directory: claudeDir).contains("left it untouched"))
        XCTAssertEqual(ClaudeDesktopConfig.unregister(directory: claudeDir), "Off")
        XCTAssertEqual(try Data(contentsOf: claudeFile), broken)
    }

    func testUnregisterWithoutClaudeDesktopDoesNothing() {
        XCTAssertEqual(ClaudeDesktopConfig.unregister(directory: claudeDir), "Off")
        XCTAssertFalse(FileManager.default.fileExists(atPath: claudeDir.path))
    }

    // MARK: MCP batches and audit log

    func testBatchRequests() throws {
        let body = Data(#"[{"jsonrpc":"2.0","id":1,"method":"ping"},{"jsonrpc":"2.0","method":"notifications/initialized"},{"jsonrpc":"2.0","id":2,"method":"tools/list"}]"#.utf8)
        let replies = try XCTUnwrap(JSONSerialization.jsonObject(with: XCTUnwrap(MCPServer().handle(body))) as? [[String: Any]])
        XCTAssertEqual(replies.compactMap { $0["id"] as? Int }.sorted(), [1, 2])
        XCTAssertNil(MCPServer().handle(Data(#"[{"jsonrpc":"2.0","method":"notifications/initialized"}]"#.utf8)))
    }

    func testAuditLogAppends() throws {
        Audit.record(tool: "rename_transcript", meetingID: nil)
        Audit.record(tool: "delete_transcript", meetingID: "m1")
        let lines = try String(contentsOf: Paths.auditLog, encoding: .utf8).split(separator: "\n")
        XCTAssertEqual(lines.count, 2)
        XCTAssertFalse(lines[0].contains("meetingId"))
        let second = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(lines[1].utf8)) as? [String: Any])
        XCTAssertEqual(second["meetingId"] as? String, "m1")
        XCTAssertNotNil(second["time"])
    }
}
