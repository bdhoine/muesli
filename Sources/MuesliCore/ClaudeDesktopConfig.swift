import Foundation

/// Adds or removes the "muesli" entry in Claude Desktop's MCP server list.
/// Every other key in that file is left exactly as it was.
public enum ClaudeDesktopConfig {
    public static let key = "Muesli"
    public static let legacyKeys = ["muesli"]

    public static var defaultDirectory: URL {
        FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0].appendingPathComponent("Claude", isDirectory: true)
    }

    static func file(in directory: URL) -> URL { directory.appendingPathComponent("claude_desktop_config.json") }

    /// Returns a status line for the Preferences window.
    public static func register(_ helper: URL, directory: URL = defaultDirectory) -> String {
        let file = file(in: directory)
        guard FileManager.default.fileExists(atPath: directory.path) else {
            return "Claude Desktop was not found on this Mac. Install it, then switch this off and on again."
        }
        guard var root = read(file) else { return "Could not read \(file.lastPathComponent); left it untouched. Add the server by hand." }
        var servers = root["mcpServers"] as? [String: Any] ?? [:]
        let hasLegacy = legacyKeys.contains { servers[$0] != nil }
        if !hasLegacy, let existing = servers[key] as? [String: Any], existing["command"] as? String == helper.path {
            return "Registered with Claude Desktop."
        }
        for legacy in legacyKeys { servers[legacy] = nil }
        servers[key] = ["command": helper.path, "args": [String]()] as [String: Any]
        root["mcpServers"] = servers
        return write(root, to: file) ? "Registered with Claude Desktop. Quit and reopen Claude Desktop to load it." : "Could not write \(file.lastPathComponent)."
    }

    public static func unregister(directory: URL = defaultDirectory) -> String {
        let file = file(in: directory)
        guard FileManager.default.fileExists(atPath: file.path), var root = read(file),
              var servers = root["mcpServers"] as? [String: Any],
              ([key] + legacyKeys).contains(where: { servers[$0] != nil }) else { return "Off" }
        for name in [key] + legacyKeys { servers[name] = nil }
        root["mcpServers"] = servers
        return write(root, to: file) ? "Removed from Claude Desktop (takes effect when Claude Desktop restarts)." : "Could not write \(file.lastPathComponent)."
    }

    /// The file's JSON object; an empty object when the file does not exist yet; nil when it cannot be parsed.
    private static func read(_ file: URL) -> [String: Any]? {
        guard FileManager.default.fileExists(atPath: file.path) else { return [:] }
        guard let data = try? Data(contentsOf: file) else { return nil }
        if data.isEmpty { return [:] }
        return (try? JSONSerialization.jsonObject(with: data)) as? [String: Any]
    }

    private static func write(_ root: [String: Any], to file: URL) -> Bool {
        guard let data = try? JSONSerialization.data(withJSONObject: root, options: [.prettyPrinted, .sortedKeys, .withoutEscapingSlashes]) else { return false }
        return (try? data.write(to: file, options: .atomic)) != nil
    }
}
