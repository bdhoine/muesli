import Foundation
import MuesliFFI

/// The assistants Muesli can set up. Texts and commands come from the shared Rust core.
public enum AssistantClient: String, CaseIterable, Identifiable, Sendable {
    case claudeCode, codex, geminiCLI, vsCode, cursor, other

    public var id: String { rawValue }

    private var kind: ClientKind {
        switch self {
        case .claudeCode: .claudeCode
        case .codex: .codex
        case .geminiCLI: .geminiCli
        case .vsCode: .vsCode
        case .cursor: .cursor
        case .other: .other
        }
    }

    private var info: ClientInfo { clientInfo(client: kind) }

    public var name: String { info.name }
    public var instructions: String { info.instructions }
    public var isTerminalCommand: Bool { info.isTerminalCommand }

    public func snippet(helper: String) -> String { clientSnippet(client: kind, helper: helper) }
}
