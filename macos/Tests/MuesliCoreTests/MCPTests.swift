import XCTest
@testable import MuesliCore

final class MCPTests: IsolatedTestCase {
    // MARK: helpers

    private func rpc(_ method: String, _ params: [String: Any] = [:], id: Any? = 1, server: MCPServer = MCPServer()) -> [String: Any]? {
        var msg: [String: Any] = ["jsonrpc": "2.0", "method": method, "params": params]
        if let id { msg["id"] = id }
        let data = try! JSONSerialization.data(withJSONObject: msg)
        return server.handle(data).map { try! JSONSerialization.jsonObject(with: $0) as! [String: Any] }
    }

    private func call(_ tool: String, _ args: [String: Any] = [:], server: MCPServer = MCPServer()) -> (text: String, isError: Bool) {
        let result = rpc("tools/call", ["name": tool, "arguments": args], server: server)!["result"] as! [String: Any]
        let content = result["content"] as! [[String: Any]]
        return (content[0]["text"] as! String, result["isError"] as! Bool)
    }

    private func object(_ text: String) -> [String: Any] {
        try! JSONSerialization.jsonObject(with: Data(text.utf8)) as! [String: Any]
    }

    // MARK: protocol

    func testInitializeNegotiatesVersion() {
        let known = rpc("initialize", ["protocolVersion": "2025-03-26"])!["result"] as! [String: Any]
        XCTAssertEqual(known["protocolVersion"] as? String, "2025-03-26")
        let unknown = rpc("initialize", ["protocolVersion": "2099-01-01"])!["result"] as! [String: Any]
        XCTAssertEqual(unknown["protocolVersion"] as? String, MCPServer.protocolVersions[0])
    }

    func testNotificationsGetNoReply() {
        XCTAssertNil(rpc("notifications/initialized", id: nil))
    }

    func testToolsListMarksWhichToolsChangeData() {
        let tools = (rpc("tools/list")!["result"] as! [String: Any])["tools"] as! [[String: Any]]
        XCTAssertEqual(Set(tools.map { $0["name"] as! String }), Tools.names)
        for t in tools {
            let readOnly = (t["annotations"] as! [String: Any])["readOnlyHint"] as? Bool
            XCTAssertEqual(readOnly, Tools.readNames.contains(t["name"] as! String))
        }
    }

    func testUnknownMethodAndParseError() {
        XCTAssertEqual((rpc("resources/list")!["error"] as! [String: Any])["code"] as? Int, -32601)
        let reply = try! JSONSerialization.jsonObject(with: MCPServer().handle(Data("{not json".utf8))!) as! [String: Any]
        XCTAssertEqual((reply["error"] as! [String: Any])["code"] as? Int, -32700)
    }

    func testRefusalBlocksToolCalls() {
        var server = MCPServer()
        server.refusal = { "switched off" }
        let r = call("get_status", server: server)
        XCTAssertTrue(r.isError)
        XCTAssertEqual(r.text, "switched off")
    }

    // MARK: get_status

    func testStatusWithoutAppNeverClaimsRecording() {
        MeetingStore.save(meeting("ready-1", state: .ready))
        var stale = AppStatus(appState: "recording", current: .init(id: "x", title: "t", source: "manual", startedAt: Date(), segmentCount: 1))
        stale.updatedAt = Date().addingTimeInterval(-120)
        stale.write()

        let s = object(call("get_status").text)
        XCTAssertEqual(s["appRunning"] as? Bool, false)
        XCTAssertEqual(s["appState"] as? String, "notRunning")
        XCTAssertNil(s["current"])
        XCTAssertEqual((s["lastReady"] as? [String: Any])?["id"] as? String, "ready-1")
    }

    func testStatusWhileRecording() {
        AppStatus(appState: "recording",
                  detected: .init(title: "Teams call", source: "teams", since: Date()),
                  current: .init(id: "abc", title: "Teams call", source: "teams", startedAt: Date(), segmentCount: 3)).write()
        let s = object(call("get_status").text)
        XCTAssertEqual(s["appRunning"] as? Bool, true)
        XCTAssertEqual(s["appState"] as? String, "recording")
        XCTAssertEqual((s["current"] as? [String: Any])?["segmentCount"] as? Int, 3)
        XCTAssertEqual((s["detected"] as? [String: Any])?["source"] as? String, "teams")
    }

    // MARK: list_transcripts

    func testListFiltersAndOrders() {
        let t0 = Date(timeIntervalSince1970: 1_800_000_000)
        MeetingStore.save(meeting("old", state: .ready, started: t0))
        MeetingStore.save(meeting("new", state: .ready, started: t0.addingTimeInterval(86_400)))
        MeetingStore.save(meeting("live", state: .recording, started: t0.addingTimeInterval(90_000)))

        let all = object(call("list_transcripts").text)["transcripts"] as! [[String: Any]]
        XCTAssertEqual(all.map { $0["id"] as! String }, ["live", "new", "old"])
        XCTAssertEqual(all[0]["partial"] as? Bool, true)
        XCTAssertEqual(all[1]["durationMinutes"] as? Int, 30)

        let ready = object(call("list_transcripts", ["state": "ready", "limit": 1]).text)["transcripts"] as! [[String: Any]]
        XCTAssertEqual(ready.map { $0["id"] as! String }, ["new"])

        let since = ISO8601DateFormatter().string(from: t0.addingTimeInterval(3600))
        let recent = object(call("list_transcripts", ["since": since]).text)["transcripts"] as! [[String: Any]]
        XCTAssertEqual(recent.count, 2)

        XCTAssertTrue(call("list_transcripts", ["since": "yesterday"]).isError)
    }

    // MARK: get_transcript

    func testTranscriptMarkdownAndJSON() {
        MeetingStore.save(meeting("m1", state: .ready, segments: 3))
        let md = call("get_transcript", ["id": "m1"])
        XCTAssertFalse(md.isError)
        XCTAssertTrue(md.text.contains("# Weekly sync"))
        XCTAssertTrue(md.text.contains("[00:05] **Others**: Segment 1"))
        XCTAssertTrue(md.text.contains("- State: ready"))

        let js = object(call("get_transcript", ["id": "m1", "format": "json", "fromSeconds": 5, "toSeconds": 10]).text)
        XCTAssertEqual((js["segments"] as! [[String: Any]]).count, 1)
        XCTAssertEqual(js["truncated"] as? Bool, false)
        XCTAssertEqual(js["partial"] as? Bool, false)
    }

    func testTranscriptWhileRecordingIsPartial() {
        MeetingStore.save(meeting("live", state: .recording))
        let js = object(call("get_transcript", ["id": "live", "format": "json"]).text)
        XCTAssertEqual(js["partial"] as? Bool, true)
        XCTAssertEqual(js["state"] as? String, "recording")
    }

    func testTranscriptRejectsBadIDs() {
        XCTAssertTrue(call("get_transcript", ["id": "missing"]).isError)
        XCTAssertTrue(call("get_transcript", ["id": "../config"]).isError)
        XCTAssertTrue(call("get_transcript").isError)
    }

    func testLongTranscriptPagesWithoutLoss() {
        var m = meeting("long", state: .ready, segments: 0)
        let filler = String(repeating: "word ", count: 200)                       // ~1000 characters per segment
        m.transcript = (0..<300).map { (i: Int) -> TranscriptSegment in
            let start = Double(i / 2) * 10
            let speaker = i % 2 == 0 ? "You" : "Others"
            return TranscriptSegment(start: start, end: start + 9, speaker: speaker, text: "\(i) \(filler)")
        }
        MeetingStore.save(m)

        var seen: [String] = []
        var from = 0.0
        var pages = 0
        while true {
            let js = object(call("get_transcript", ["id": "long", "format": "json", "fromSeconds": from]).text)
            seen += (js["segments"] as! [[String: Any]]).map { ($0["text"] as! String).components(separatedBy: " ")[0] }
            pages += 1
            guard js["truncated"] as? Bool == true else { break }
            from = js["nextFromSeconds"] as! Double
            XCTAssertLessThan(pages, 50)
        }
        XCTAssertGreaterThan(pages, 1)
        XCTAssertEqual(seen, (0..<300).map(String.init))
    }

    func testPagingKeepsEqualStartsTogether() {
        let segs = [0.0, 1, 1, 1, 2].map { TranscriptSegment(start: $0, end: $0 + 1, speaker: "You", text: "x") }
        let (page, next) = Tools.page(segs, cap: 3) { _ in 1 }
        XCTAssertEqual(page.count, 1)
        XCTAssertEqual(next, 1)
        let (run, after) = Tools.page(Array(segs.dropFirst()), cap: 2) { _ in 1 }   // run of three equal starts over the cap
        XCTAssertEqual(run.count, 3)
        XCTAssertEqual(after, 2)
    }

    // MARK: settings and stores

    func testPreferencesRoundTripAndDefaults() throws {
        XCTAssertEqual(Preferences.load(), Preferences())
        var s = Preferences()
        s.mcpEnabled = true; s.claudeDesktopConfigEntry = true; s.recentLimit = 3
        s.save()
        XCTAssertEqual(Preferences.load(), s)

        try Data(#"{"mcpEnabled":true}"#.utf8).write(to: Paths.config)             // older file with fewer keys
        XCTAssertEqual(Preferences.load().recentLimit, Preferences().recentLimit)
        XCTAssertTrue(Preferences.load().mcpEnabled)

        let remoteEra = #"{"mcpEnabled":true,"mcpMode":"remote","httpPort":8747,"teamDomain":"t","audience":"a","allowedEmail":"e","remoteProvider":"cloudflare"}"#
        try Data(remoteEra.utf8).write(to: Paths.config)                           // files from before remote mode was removed
        XCTAssertTrue(Preferences.load().mcpEnabled)
    }

    func testMeetingDecodesFilesWithoutState() throws {
        let old = #"{"id":"a","title":"t","source":"manual","startedAt":"2026-11-10T14:30:00Z","attendees":[],"transcript":[]}"#
        let m = try JSONCoding.decoder().decode(Meeting.self, from: Data(old.utf8))
        XCTAssertEqual(m.state, .ready)
    }
}
