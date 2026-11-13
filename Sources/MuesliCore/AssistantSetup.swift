import Foundation

public enum AssistantClient: String, CaseIterable, Identifiable, Sendable {
    case claudeCode, codex, geminiCLI, vsCode, cursor, other

    public var id: String { rawValue }

    public var name: String {
        switch self {
        case .claudeCode: "Claude Code"
        case .codex: "Codex CLI"
        case .geminiCLI: "Gemini CLI"
        case .vsCode: "VS Code (Copilot)"
        case .cursor: "Cursor"
        case .other: "Other MCP client"
        }
    }

    public var instructions: String {
        switch self {
        case .claudeCode: "Run this in Terminal. It adds Muesli for your user, so it works in every project."
        case .codex: "Run this in Terminal. Codex saves it in ~/.codex/config.toml."
        case .geminiCLI: "Run this in Terminal. It adds Muesli for your user in ~/.gemini/settings.json."
        case .vsCode: "Run this in Terminal, then use Muesli from Copilot Chat in agent mode."
        case .cursor: "Add this to ~/.cursor/mcp.json, next to any servers already there. Or use Cursor Settings → MCP → Add new server."
        case .other: "Muesli speaks MCP over stdio. Most clients accept this in their configuration file."
        }
    }

    public var isTerminalCommand: Bool {
        switch self {
        case .claudeCode, .codex, .geminiCLI, .vsCode: true
        case .cursor, .other: false
        }
    }

    public func snippet(helper: String, name: String = ClaudeDesktopExtension.name) -> String {
        let path = AssistantClient.shellQuote(helper)
        switch self {
        case .claudeCode:
            return "claude mcp add --scope user \(name) -- \(path)"
        case .codex:
            return "codex mcp add \(name) -- \(path)"
        case .geminiCLI:
            return "gemini mcp add --scope user \(name) \(path)"
        case .vsCode:
            let json = AssistantClient.json(["name": name, "command": helper], pretty: false)
            return "code --add-mcp \(AssistantClient.shellQuote(json))"
        case .cursor, .other:
            return AssistantClient.json(["mcpServers": [name: ["command": helper]]], pretty: true)
        }
    }

    static func shellQuote(_ s: String) -> String {
        "'" + s.replacingOccurrences(of: "'", with: "'\\''") + "'"
    }

    static func json(_ object: [String: Any], pretty: Bool) -> String {
        var options: JSONSerialization.WritingOptions = [.sortedKeys, .withoutEscapingSlashes]
        if pretty { options.insert(.prettyPrinted) }
        guard let data = try? JSONSerialization.data(withJSONObject: object, options: options) else { return "" }
        return String(decoding: data, as: UTF8.self)
    }
}
