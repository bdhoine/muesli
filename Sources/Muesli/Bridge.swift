import AppKit
import MuesliCore

/// Connects Muesli to Claude according to Preferences. The app itself never opens a
/// socket: local mode only registers the bundled `muesli-mcp` with Claude Desktop, and
/// remote mode runs `muesli-mcp --http` as a separate process for cloudflared to reach.
@MainActor @Observable
final class BridgeController {
    private(set) var localStatus = ""
    private(set) var remoteStatus = ""
    private(set) var remoteRunning = false
    private var process: Process?

    /// The muesli-mcp binary inside the app bundle.
    static var helperURL: URL? {
        let candidates = [Bundle.main.url(forAuxiliaryExecutable: "muesli-mcp"),
                          Bundle.main.executableURL?.deletingLastPathComponent().appendingPathComponent("muesli-mcp")]
        return candidates.compactMap { $0 }.first { FileManager.default.isExecutableFile(atPath: $0.path) }
    }

    func apply(_ settings: Preferences) {
        stopRemote()
        guard let helper = BridgeController.helperURL else {
            localStatus = "muesli-mcp is missing from the app bundle; rebuild with build.sh."
            remoteStatus = localStatus
            return
        }
        if settings.mcpEnabled && settings.mcpMode == .local {
            localStatus = ClaudeDesktopConfig.register(helper)
        } else {
            localStatus = ClaudeDesktopConfig.unregister()
        }
        if settings.mcpEnabled && settings.mcpMode == .remote {
            startRemote(helper, settings)
        } else {
            remoteStatus = "Off"
        }
    }

    func shutdown() { stopRemote() }

    private func startRemote(_ helper: URL, _ settings: Preferences) {
        if let problem = settings.remoteProblem {
            remoteStatus = "Not started: \(problem)."
            return
        }
        let p = Process()
        p.executableURL = helper
        p.arguments = ["--http", String(settings.httpPort), "--watch-parent"]
        p.terminationHandler = { [weak self] finished in
            let code = finished.terminationStatus
            Task { @MainActor in
                guard let self, self.process === finished else { return }
                self.process = nil
                self.remoteRunning = false
                self.remoteStatus = "Stopped unexpectedly (exit \(code)). Is port \(settings.httpPort) already in use?"
            }
        }
        do {
            try p.run()
            process = p
            remoteRunning = true
            remoteStatus = "Running on 127.0.0.1:\(settings.httpPort)"
        } catch {
            remoteStatus = "Could not start: \(error.localizedDescription)"
        }
    }

    private func stopRemote() {
        guard let p = process else { return }
        process = nil
        remoteRunning = false
        p.terminationHandler = nil
        if p.isRunning {
            p.terminate()
            p.waitUntilExit()                 // the port must be free before a restart
        }
    }
}

/// Adds or removes the "muesli" entry in Claude Desktop's MCP server list.
/// Every other key in that file is left exactly as it was.
enum ClaudeDesktopConfig {
    static let key = "muesli"

    static var directory: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0].appendingPathComponent("Claude", isDirectory: true)
    }
    static var file: URL { directory.appendingPathComponent("claude_desktop_config.json") }

    static func register(_ helper: URL) -> String {
        guard FileManager.default.fileExists(atPath: directory.path) else {
            return "Claude Desktop was not found on this Mac. Install it, then switch this off and on again."
        }
        guard var root = read() else { return "Could not read \(file.lastPathComponent); left it untouched. Add the server by hand." }
        var servers = root["mcpServers"] as? [String: Any] ?? [:]
        let entry: [String: Any] = ["command": helper.path, "args": [String]()]
        if let existing = servers[key] as? [String: Any], existing["command"] as? String == helper.path {
            return "Registered with Claude Desktop."
        }
        servers[key] = entry
        root["mcpServers"] = servers
        return write(root) ? "Registered with Claude Desktop. Quit and reopen Claude Desktop to load it." : "Could not write \(file.lastPathComponent)."
    }

    static func unregister() -> String {
        guard FileManager.default.fileExists(atPath: file.path), var root = read(),
              var servers = root["mcpServers"] as? [String: Any], servers[key] != nil else { return "Off" }
        servers[key] = nil
        root["mcpServers"] = servers
        return write(root) ? "Removed from Claude Desktop (takes effect when Claude Desktop restarts)." : "Could not write \(file.lastPathComponent)."
    }

    /// The file's JSON object; an empty object when the file does not exist yet; nil when it cannot be parsed.
    private static func read() -> [String: Any]? {
        guard FileManager.default.fileExists(atPath: file.path) else { return [:] }
        guard let data = try? Data(contentsOf: file) else { return nil }
        if data.isEmpty { return [:] }
        return (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
    }

    private static func write(_ root: [String: Any]) -> Bool {
        guard let data = try? JSONSerialization.data(withJSONObject: root, options: [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]) else { return false }
        return (try? data.write(to: file, options: .atomic)) != nil
    }
}
