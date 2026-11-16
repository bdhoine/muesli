import Foundation

/// Entry point of the muesli-mcp executable.
public enum MCPMain {
    public static func run(_ args: [String]) -> Never {
        switch args.first {
        case nil, "stdio":
            runStdio()
        case "doctor":
            exit(Doctor.run() ? 0 : 1)
        case "--version":
            print("muesli-mcp \(MuesliInfo.version)")
            exit(0)
        case "--mcpb-manifest":
            FileHandle.standardOutput.write(ClaudeDesktopExtension.manifestJSON())
            exit(0)
        default:
            print("""
            muesli-mcp \(MuesliInfo.version): MCP server for Muesli

              muesli-mcp                  serve over stdio (Claude Desktop, Claude Code and other MCP clients)
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
            if !s.mcpEnabled { return "AI assistant access is switched off in Muesli. Turn it on in Muesli > Settings > AI Assistants." }
            return nil
        }
        server.audit = { tool, meetingID in
            if Tools.manageNames.contains(tool) { Audit.record(tool: tool, meetingID: meetingID) }
        }
        while let line = readLine(strippingNewline: true) {
            guard !line.isEmpty, let reply = server.handle(Data(line.utf8)) else { continue }
            FileHandle.standardOutput.write(reply + Data("\n".utf8))
        }
        exit(0)
    }
}

/// Append-only record of tool calls that change meetings: which tool, which meeting, when.
enum Audit {
    private static let lock = NSLock()

    static func record(tool: String, meetingID: String?) {
        var entry: [String: Any] = ["time": ISO8601DateFormatter().string(from: Date()), "tool": tool]
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
        check(s.mcpEnabled, "AI assistant access enabled", s.mcpEnabled ? "" : "off in Muesli > Settings > AI Assistants")

        if let status = AppStatus.read() {
            check(status.isFresh(), "Muesli app running", status.isFresh() ? "state \(status.appState)" : "last heartbeat \(ISO8601DateFormatter().string(from: status.updatedAt))")
        } else {
            check(false, "Muesli app running", "no state.json; start Muesli")
        }
        print("      \(MeetingStore.all().count) meeting(s) in the store")

        note("The assistant (or the Claude Desktop extension) starts muesli-mcp itself over stdio; nothing else to check.")
        return ok
    }
}
