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
        XCTAssertEqual(p.mcpMode, .local)
        XCTAssertFalse(p.autoStop)
        XCTAssertEqual(p.endDelaySeconds, 60)
    }

    func testCorruptPreferencesFallBackToDefaults() throws {
        try Data("garbage".utf8).write(to: Paths.config)
        XCTAssertEqual(Preferences.load(), Preferences())
    }

    func testRemoteProblemNamesTheFirstMissingField() {
        var p = Preferences()
        XCTAssertEqual(p.remoteProblem, "Team domain is missing")
        p.teamDomain = "team"
        XCTAssertEqual(p.remoteProblem, "Audience (AUD) tag is missing")
        p.audience = "aud"
        XCTAssertEqual(p.remoteProblem, "Allowed email is missing")
        p.allowedEmail = "  "
        XCTAssertEqual(p.remoteProblem, "Allowed email is missing")
        p.allowedEmail = "me@example.com"
        XCTAssertNil(p.remoteProblem)
        p.httpPort = 80
        XCTAssertEqual(p.remoteProblem, "Port must be between 1024 and 65535")
    }

    func testHostnameNormalization() {
        var p = Preferences()
        p.publicHostname = "  HTTPS://Muesli.Example.com/mcp "
        XCTAssertEqual(p.normalizedHostname, "muesli.example.com")
        XCTAssertEqual(Preferences().normalizedTeamDomain, "")
    }

    // MARK: Claude Desktop registration (local mode)

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
        let entry = try XCTUnwrap((claudeConfig()["mcpServers"] as? [String: Any])?["muesli"] as? [String: Any])
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
        XCTAssertEqual(Set(servers.keys), ["filesystem", "muesli"])
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
        let entry = try XCTUnwrap((claudeConfig()["mcpServers"] as? [String: Any])?["muesli"] as? [String: Any])
        XCTAssertEqual(entry["command"] as? String, helper.path)
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

    // MARK: Remote mode over a real socket

    func testHTTPServerEndToEnd() throws {
        MeetingStore.save(meeting("m1", state: .ready))
        let verifier = AccessVerifier(teamDomain: "team.cloudflareaccess.com", audience: "aud", allowedEmail: "me@example.com",
                                      fetchKeys: { _ in ["k1": .init(n: Data([1]), e: Data([1]))] },
                                      checkSignature: { _, _, signature in signature == Data("good".utf8) })
        let port = UInt16.random(in: 20_000..<60_000)
        let server = HTTPServer(port: port) { MCPMain.route($0, verifier: verifier) }
        Thread.detachNewThread { _ = try? server.run() }
        let url = try XCTUnwrap(URL(string: "http://127.0.0.1:\(port)/mcp"))
        let call = #"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"get_transcript","arguments":{"id":"m1"}}}"#

        var first: HTTPClient.Response?
        for _ in 0..<50 where first == nil {                       // wait for the listener
            first = HTTPClient.postJSON(url, body: call, timeout: 2)
            if first == nil { usleep(100_000) }
        }
        XCTAssertEqual(first?.status, 401)                         // no token

        func token(_ signature: String) -> String {
            func b64(_ d: Data) -> String {
                d.base64EncodedString().replacingOccurrences(of: "+", with: "-").replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "=", with: "")
            }
            let header = try! JSONSerialization.data(withJSONObject: ["alg": "RS256", "kid": "k1"])
            let claims = try! JSONSerialization.data(withJSONObject: ["iss": "https://team.cloudflareaccess.com", "aud": ["aud"],
                                                                      "exp": Date().timeIntervalSince1970 + 600, "email": "me@example.com"] as [String: Any])
            return "\(b64(header)).\(b64(claims)).\(b64(Data(signature.utf8)))"
        }

        let forged = HTTPClient.postJSON(url, body: call, headers: ["Cf-Access-Jwt-Assertion": token("forged")], timeout: 5)
        XCTAssertEqual(forged?.status, 401)

        let ok = try XCTUnwrap(HTTPClient.postJSON(url, body: call, headers: ["Cf-Access-Jwt-Assertion": token("good")], timeout: 5))
        XCTAssertEqual(ok.status, 200)
        let reply = try XCTUnwrap(JSONSerialization.jsonObject(with: ok.body) as? [String: Any])
        XCTAssertEqual(reply["id"] as? Int, 7)
        let text = try XCTUnwrap((((reply["result"] as? [String: Any])?["content"] as? [[String: Any]])?.first?["text"]) as? String)
        XCTAssertTrue(text.contains("**You**: Segment 0"))

        let big = String(repeating: "x", count: HTTPServer.maxBodyBytes + 1)
        XCTAssertEqual(HTTPClient.postJSON(url, body: big, headers: ["Cf-Access-Jwt-Assertion": token("good")], timeout: 5)?.status, 413)
    }

    func testBatchRequests() throws {
        let body = Data(#"[{"jsonrpc":"2.0","id":1,"method":"ping"},{"jsonrpc":"2.0","method":"notifications/initialized"},{"jsonrpc":"2.0","id":2,"method":"tools/list"}]"#.utf8)
        let replies = try XCTUnwrap(JSONSerialization.jsonObject(with: XCTUnwrap(MCPServer().handle(body))) as? [[String: Any]])
        XCTAssertEqual(replies.compactMap { $0["id"] as? Int }.sorted(), [1, 2])
        XCTAssertNil(MCPServer().handle(Data(#"[{"jsonrpc":"2.0","method":"notifications/initialized"}]"#.utf8)))
    }

    func testAuditLogAppends() throws {
        Audit.record(tool: "get_status", meetingID: nil, email: "me@example.com")
        Audit.record(tool: "get_transcript", meetingID: "m1", email: "me@example.com")
        let lines = try String(contentsOf: Paths.auditLog, encoding: .utf8).split(separator: "\n")
        XCTAssertEqual(lines.count, 2)
        XCTAssertFalse(lines[0].contains("meetingId"))
        let second = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(lines[1].utf8)) as? [String: Any])
        XCTAssertEqual(second["meetingId"] as? String, "m1")
        XCTAssertNotNil(second["time"])
    }
}
