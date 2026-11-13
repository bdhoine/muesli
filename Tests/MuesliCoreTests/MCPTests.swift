import XCTest
@testable import MuesliCore

final class MCPTests: XCTestCase {
    private var dir: URL!

    override func setUp() {
        dir = FileManager.default.temporaryDirectory.appendingPathComponent("muesli-tests-\(UUID().uuidString)")
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        setenv("MUESLI_SUPPORT_DIR", dir.path, 1)
    }

    override func tearDown() {
        try? FileManager.default.removeItem(at: dir)
        unsetenv("MUESLI_SUPPORT_DIR")
    }

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

    private func meeting(_ id: String, state: MeetingState, segments: Int = 2, started: Date = Date(timeIntervalSince1970: 1_800_000_000)) -> Meeting {
        Meeting(id: id, title: "Weekly sync", source: "calendar", startedAt: started,
                endedAt: state == .ready ? started.addingTimeInterval(1800) : nil,
                attendees: ["Ann"],
                transcript: (0..<segments).map { TranscriptSegment(start: Double($0) * 5, end: Double($0) * 5 + 4, speaker: $0 % 2 == 0 ? "You" : "Others", text: "Segment \($0)") },
                state: state)
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

    func testToolsListIsReadOnly() {
        let tools = (rpc("tools/list")!["result"] as! [String: Any])["tools"] as! [[String: Any]]
        XCTAssertEqual(Set(tools.map { $0["name"] as! String }), Tools.names)
        for t in tools { XCTAssertEqual((t["annotations"] as! [String: Any])["readOnlyHint"] as? Bool, true) }
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
        m.transcript = (0..<300).map { TranscriptSegment(start: Double($0 / 2) * 10, end: Double($0 / 2) * 10 + 9, speaker: $0 % 2 == 0 ? "You" : "Others", text: "\($0) \(filler)") }
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
        s.mcpEnabled = true; s.mcpMode = .remote; s.teamDomain = "https://MyTeam.cloudflareaccess.com/"
        s.save()
        XCTAssertEqual(Preferences.load(), s)
        XCTAssertEqual(s.normalizedTeamDomain, "myteam.cloudflareaccess.com")
        s.teamDomain = "myteam"
        XCTAssertEqual(s.normalizedTeamDomain, "myteam.cloudflareaccess.com")
        XCTAssertNotNil(s.remoteProblem)
        s.audience = "aud"; s.allowedEmail = "me@example.com"
        XCTAssertNil(s.remoteProblem)

        try Data(#"{"mcpEnabled":true}"#.utf8).write(to: Paths.config)             // older file with fewer keys
        XCTAssertEqual(Preferences.load().httpPort, Preferences().httpPort)
        XCTAssertTrue(Preferences.load().mcpEnabled)
    }

    func testMeetingDecodesFilesWithoutState() throws {
        let old = #"{"id":"a","title":"t","source":"manual","startedAt":"2026-11-10T14:30:00Z","attendees":[],"transcript":[]}"#
        let m = try JSONCoding.decoder().decode(Meeting.self, from: Data(old.utf8))
        XCTAssertEqual(m.state, .ready)
    }

    // MARK: HTTP parsing

    private func parse(_ raw: String, chunkSize: Int = 7) -> Result<HTTPRequest, HTTPServer.ParseError> {
        var bytes = Array(raw.utf8)
        return HTTPServer.parse(HTTPServer.Reader {
            let n = min(chunkSize, bytes.count)
            defer { bytes.removeFirst(n) }
            return Array(bytes.prefix(n))
        })
    }

    func testParsesContentLengthAndChunkedBodies() throws {
        let plain = try parse("POST /mcp?x=1 HTTP/1.1\r\nHost: a\r\nContent-Length: 5\r\nCf-Access-Jwt-Assertion: tok\r\n\r\nhello").get()
        XCTAssertEqual(plain.method, "POST")
        XCTAssertEqual(plain.path, "/mcp")
        XCTAssertEqual(plain.headers["cf-access-jwt-assertion"], "tok")
        XCTAssertEqual(String(decoding: plain.body, as: UTF8.self), "hello")

        let chunked = try parse("POST /mcp HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n3\r\nhel\r\n2;ext=1\r\nlo\r\n0\r\n\r\n").get()
        XCTAssertEqual(String(decoding: chunked.body, as: UTF8.self), "hello")
    }

    func testRejectsOversizedAndBrokenRequests() {
        guard case .failure(let big) = parse("POST /mcp HTTP/1.1\r\nContent-Length: 99999999\r\n\r\n") else { return XCTFail() }
        XCTAssertEqual(big.status, 413)
        guard case .failure(let short) = parse("POST /mcp HTTP/1.1\r\nContent-Length: 10\r\n\r\nabc") else { return XCTFail() }
        XCTAssertEqual(short.status, 400)
        guard case .failure = parse("garbage\r\n\r\n") else { return XCTFail() }
    }

    // MARK: Cloudflare Access token checks

    private func token(_ claims: [String: Any], alg: String = "RS256", kid: String = "k1") -> String {
        func part(_ o: [String: Any]) -> String {
            try! JSONSerialization.data(withJSONObject: o).base64EncodedString()
                .replacingOccurrences(of: "+", with: "-").replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "=", with: "")
        }
        return "\(part(["alg": alg, "kid": kid])).\(part(claims)).c2ln"
    }

    private func verifier(signatureValid: Bool = true) -> AccessVerifier {
        AccessVerifier(teamDomain: "team.cloudflareaccess.com", audience: "aud-tag", allowedEmail: "Me@Example.com",
                       fetchKeys: { _ in ["k1": .init(n: Data([1]), e: Data([1]))] },
                       checkSignature: { _, _, _ in signatureValid })
    }

    private func claims(_ overrides: [String: Any] = [:]) -> [String: Any] {
        var c: [String: Any] = ["iss": "https://team.cloudflareaccess.com", "aud": ["aud-tag"],
                                "exp": Date().timeIntervalSince1970 + 600, "email": "me@example.com"]
        for (k, v) in overrides { c[k] = v }
        return c
    }

    private func reason(_ r: Result<String, AccessVerifier.Failure>) -> String? {
        if case .failure(let f) = r { return f.reason }
        return nil
    }

    func testAccessTokenAccepted() throws {
        XCTAssertEqual(try verifier().verify(token(claims())).get(), "me@example.com")
    }

    func testAccessTokenRejections() {
        XCTAssertEqual(reason(verifier(signatureValid: false).verify(token(claims()))), "bad signature")
        XCTAssertEqual(reason(verifier().verify(token(claims(["email": "other@example.com"])))), "identity not allowed")
        XCTAssertEqual(reason(verifier().verify(token(claims(["aud": ["someone-else"]])))), "wrong audience")
        XCTAssertEqual(reason(verifier().verify(token(claims(["iss": "https://evil.example"])))), "wrong issuer")
        XCTAssertEqual(reason(verifier().verify(token(claims(["exp": Date().timeIntervalSince1970 - 5])))), "expired")
        XCTAssertEqual(reason(verifier().verify(token(claims(), alg: "none"))), "unsupported algorithm")
        XCTAssertEqual(reason(verifier().verify(token(claims(), alg: "HS256"))), "unsupported algorithm")
        XCTAssertEqual(reason(verifier().verify(token(claims(), kid: "unknown"))), "unknown signing key")
        XCTAssertEqual(reason(verifier().verify("not-a-token")), "malformed token")
    }

    func testRouteRequiresTokenBeforeAnythingElse() {
        let v = verifier()
        let body = Data(#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#.utf8)
        XCTAssertEqual(MCPMain.route(HTTPRequest(method: "POST", path: "/mcp", headers: [:], body: body), verifier: v).status, 401)
        XCTAssertEqual(MCPMain.route(HTTPRequest(method: "GET", path: "/nope", headers: [:], body: Data()), verifier: v).status, 401)
        let bad = ["cf-access-jwt-assertion": token(claims(["email": "x@y.z"]))]
        XCTAssertEqual(MCPMain.route(HTTPRequest(method: "POST", path: "/mcp", headers: bad, body: body), verifier: v).status, 401)

        let good = ["cf-access-jwt-assertion": token(claims())]
        XCTAssertEqual(MCPMain.route(HTTPRequest(method: "POST", path: "/mcp", headers: good, body: body), verifier: v).status, 200)
        XCTAssertEqual(MCPMain.route(HTTPRequest(method: "GET", path: "/mcp", headers: good, body: Data()), verifier: v).status, 405)
        XCTAssertEqual(MCPMain.route(HTTPRequest(method: "POST", path: "/other", headers: good, body: body), verifier: v).status, 404)
        let note = Data(#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#.utf8)
        XCTAssertEqual(MCPMain.route(HTTPRequest(method: "POST", path: "/mcp", headers: good, body: note), verifier: v).status, 202)
    }

    func testRemoteToolCallsAreAudited() throws {
        MeetingStore.save(meeting("m1", state: .ready))
        let body = Data(#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"get_transcript","arguments":{"id":"m1"}}}"#.utf8)
        let headers = ["cf-access-jwt-assertion": token(claims())]
        XCTAssertEqual(MCPMain.route(HTTPRequest(method: "POST", path: "/mcp", headers: headers, body: body), verifier: verifier()).status, 200)
        let log = try String(contentsOf: Paths.auditLog, encoding: .utf8)
        XCTAssertTrue(log.contains(#""tool":"get_transcript""#))
        XCTAssertTrue(log.contains(#""meetingId":"m1""#))
        XCTAssertTrue(log.contains(#""email":"me@example.com""#))
    }

    func testPKCS1Encoding() {
        let der = AccessVerifier.pkcs1(.init(n: Data([0x00, 0xC1, 0x02]), e: Data([0x01, 0x00, 0x01])))
        XCTAssertEqual(Array(der), [0x30, 0x0A, 0x02, 0x03, 0x00, 0xC1, 0x02, 0x02, 0x03, 0x01, 0x00, 0x01])
    }
}
