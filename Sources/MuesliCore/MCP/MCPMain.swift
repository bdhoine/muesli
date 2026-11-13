import Foundation
#if canImport(Glibc)
import Glibc
#elseif canImport(Darwin)
import Darwin
#endif

/// Entry point of the muesli-mcp executable.
public enum MCPMain {
    public static func run(_ args: [String]) -> Never {
        switch args.first {
        case nil, "stdio":
            runStdio()
        case "--http":
            guard args.count >= 2, let port = UInt16(args[1]), port >= 1024 else { fail("usage: muesli-mcp --http <port 1024-65535> [--watch-parent]") }
            runHTTP(port: port, watchParent: args.contains("--watch-parent"))
        case "doctor":
            exit(Doctor.run() ? 0 : 1)
        case "--version":
            print("muesli-mcp \(MuesliInfo.version)")
            exit(0)
        default:
            print("""
            muesli-mcp \(MuesliInfo.version): read-only MCP server for Muesli

              muesli-mcp                  serve over stdio (Claude Desktop, Claude Code)
              muesli-mcp --http <port>    serve over HTTP on 127.0.0.1 behind Cloudflare Access
              muesli-mcp doctor           check the setup
            """)
            exit(args.first == "--help" || args.first == "-h" ? 0 : 2)
        }
    }

    // MARK: stdio (local)

    private static func runStdio() -> Never {
        var server = MCPServer()
        server.refusal = {
            let s = Preferences.load()
            if !s.mcpEnabled { return "The Claude integration is switched off in Muesli. Turn it on in Muesli > Preferences." }
            if s.mcpMode != .local { return "Muesli is set to remote mode. Choose Local in Muesli > Preferences to use it from this app." }
            return nil
        }
        while let line = readLine(strippingNewline: true) {
            guard !line.isEmpty, let reply = server.handle(Data(line.utf8)) else { continue }
            FileHandle.standardOutput.write(reply + Data("\n".utf8))
        }
        exit(0)
    }

    // MARK: HTTP (remote, behind cloudflared + Cloudflare Access)

    private static func runHTTP(port: UInt16, watchParent: Bool) -> Never {
        let settings = Preferences.load()
        if let problem = settings.remoteProblem { fail("remote mode is not configured: \(problem)") }
        let verifier = AccessVerifier(teamDomain: settings.normalizedTeamDomain, audience: settings.audience,
                                      allowedEmail: settings.allowedEmail)

        if watchParent {                                   // started by the app: never outlive it
            let parent = getppid()
            Thread.detachNewThread {
                while true {
                    sleep(2)
                    if getppid() != parent { exit(0) }
                }
            }
        }

        let http = HTTPServer(port: port) { request in route(request, verifier: verifier) }
        log("listening on 127.0.0.1:\(port)")
        do { try http.run() } catch let e as HTTPServer.StartError { fail(e.message) } catch { fail("\(error)") }
    }

    static func route(_ request: HTTPRequest, verifier: AccessVerifier) -> HTTPResponse {
        // Authenticate before anything else, so an unauthenticated caller learns nothing.
        guard let token = request.headers["cf-access-jwt-assertion"], !token.isEmpty else {
            return .text(401, "Cloudflare Access token required")
        }
        let email: String
        switch verifier.verify(token) {
        case .success(let e): email = e
        case .failure(let f):
            log("rejected request: \(f.reason)")
            return .text(401, "Invalid Cloudflare Access token")
        }
        guard request.path == "/mcp" || request.path == "/" else { return .text(404, "Not found") }
        guard request.method == "POST" else {
            var r = HTTPResponse.text(405, "Use POST")
            r.headers["Allow"] = "POST"
            return r
        }
        var server = MCPServer()
        server.audit = { tool, meetingID in Audit.record(tool: tool, meetingID: meetingID, email: email) }
        guard let reply = server.handle(request.body) else { return HTTPResponse(status: 202) }
        return HTTPResponse(status: 200, headers: ["Content-Type": "application/json"], body: reply)
    }

    static func log(_ message: String) {
        FileHandle.standardError.write(Data("muesli-mcp: \(message)\n".utf8))
    }

    private static func fail(_ message: String) -> Never {
        log(message)
        exit(2)
    }
}

/// Append-only record of remote tool calls: who read what, when.
enum Audit {
    private static let lock = NSLock()

    static func record(tool: String, meetingID: String?, email: String) {
        var entry: [String: Any] = ["time": ISO8601DateFormatter().string(from: Date()), "tool": tool, "email": email]
        if let meetingID { entry["meetingId"] = meetingID }
        guard var line = try? JSONSerialization.data(withJSONObject: entry, options: [.sortedKeys]) else { return }
        line.append(0x0A)
        lock.lock(); defer { lock.unlock() }
        let url = Paths.auditLog
        if !FileManager.default.fileExists(atPath: url.path) { _ = FileManager.default.createFile(atPath: url.path, contents: nil) }
        guard let handle = try? FileHandle(forWritingTo: url) else { return }
        defer { try? handle.close() }
        _ = try? handle.seekToEnd()
        try? handle.write(contentsOf: line)
    }
}

/// `muesli-mcp doctor`: checks each link of the chain and says which one is broken.
enum Doctor {
    static func run() -> Bool {
        var ok = true
        func check(_ pass: Bool, _ label: String, _ detail: String = "") {
            print("\(pass ? "ok  " : "FAIL") \(label)\(detail.isEmpty ? "" : ": \(detail)")")
            if !pass { ok = false }
        }
        func note(_ text: String) { print("      \(text)") }

        let s = Preferences.load()
        check(FileManager.default.fileExists(atPath: Paths.config.path), "Preferences file", Paths.config.path)
        check(s.mcpEnabled, "Claude integration enabled", s.mcpEnabled ? "mode \(s.mcpMode.rawValue)" : "off in Muesli > Preferences")

        if let status = AppStatus.read() {
            check(status.isFresh(), "Muesli app running", status.isFresh() ? "state \(status.appState)" : "last heartbeat \(ISO8601DateFormatter().string(from: status.updatedAt))")
        } else {
            check(false, "Muesli app running", "no state.json; start Muesli")
        }
        print("      \(MeetingStore.all().count) meeting(s) in the store")

        guard s.mcpEnabled, s.mcpMode == .remote else {
            if s.mcpMode == .local { note("Local mode: Claude Desktop starts muesli-mcp itself; nothing else to check.") }
            return ok
        }

        check(s.remoteProblem == nil, "Remote settings complete", s.remoteProblem ?? "")
        guard s.remoteProblem == nil else { return ok }

        let initialize = #"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"muesli-doctor","version":"1"}}}"#

        let local = HTTPClient.postJSON(URL(string: "http://127.0.0.1:\(s.httpPort)/mcp")!, body: initialize, timeout: 5)
        check(local?.status == 401, "Local server on port \(s.httpPort) rejects unauthenticated calls",
              local.map { "HTTP \($0.status)" } ?? "not reachable; is the bridge running?")

        let keys = AccessVerifier.fetchJWKS(teamDomain: s.normalizedTeamDomain)
        check(keys != nil, "Cloudflare Access signing keys", keys.map { "\($0.count) key(s) from \(s.normalizedTeamDomain)" } ?? "cannot fetch from \(s.normalizedTeamDomain); check the team domain")

        let cloudflared = ["/opt/homebrew/bin/cloudflared", "/usr/local/bin/cloudflared", "/usr/bin/cloudflared"]
            .first { FileManager.default.isExecutableFile(atPath: $0) }
        check(cloudflared != nil, "cloudflared installed", cloudflared ?? "not found; brew install cloudflared")

        let host = s.normalizedHostname
        guard !host.isEmpty, let url = URL(string: "https://\(host)/mcp") else {
            note("No public hostname set; skipping the tunnel checks.")
            return ok
        }
        if let remote = HTTPClient.postJSON(url, body: initialize) {
            let body = String(decoding: remote.body.prefix(4096), as: UTF8.self)
            if remote.status == 200 && body.contains("serverInfo") {
                check(false, "Public endpoint protected", "https://\(host)/mcp answered without sign-in. Turn the bridge off and fix the Access application.")
            } else if remote.status >= 500 {
                check(false, "Tunnel reaches this Mac", "HTTP \(remote.status) from https://\(host)/mcp; is cloudflared running and pointed at port \(s.httpPort)?")
            } else {
                check(true, "Public endpoint up and asks for sign-in", "HTTP \(remote.status)")
            }
        } else {
            check(false, "Public endpoint reachable", "no answer from https://\(host)/mcp; check DNS and the tunnel")
        }
        if let meta = HTTPClient.get(URL(string: "https://\(host)/.well-known/oauth-authorization-server")!) {
            check(meta.status == 200, "OAuth discovery (Managed OAuth)", "HTTP \(meta.status)")
        }
        return ok
    }
}
