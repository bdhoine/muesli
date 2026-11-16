import Foundation

/// The Claude Desktop extension (.mcpb). Its manifest is written by `muesli-mcp --mcpb-manifest`
/// (Rust); this reads what Claude Desktop has installed.
public enum ClaudeDesktopExtension {
    public typealias State = ExtensionState

    public static var claudeDirectory: URL { ClaudeDesktopConfig.defaultDirectory }

    /// `appInstalled`: whether the Claude app itself is installed, when the caller can tell.
    /// Without it, Claude counts as present when its data folder exists.
    public static func state(bundledVersion: String = MuesliInfo.version, claude: URL = claudeDirectory,
                             appInstalled: Bool? = nil) -> State {
        claudeExtensionState(bundledVersion: bundledVersion, claude: claude.path, appInstalled: appInstalled)
    }

    public static func installedVersion(claude: URL = claudeDirectory) -> String? {
        claudeExtensionInstalledVersion(claude: claude.path)
    }

    static func isOlder(_ a: String, than b: String) -> Bool { claudeExtensionIsOlder(a: a, b: b) }
}

extension ExtensionState: Sendable {}
