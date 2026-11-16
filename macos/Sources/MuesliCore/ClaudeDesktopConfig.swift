import Foundation

/// Adds or removes the "Muesli" entry in Claude Desktop's MCP server list. Every other key in that
/// file is left exactly as it was. The logic lives in the Rust core.
public enum ClaudeDesktopConfig {
    public static var defaultDirectory: URL { URL(fileURLWithPath: claudeDefaultDirectory(), isDirectory: true) }

    /// Returns a status line for the Preferences window.
    public static func register(_ helper: URL, directory: URL = defaultDirectory) -> String {
        claudeConfigRegister(helper: helper.path, directory: directory.path)
    }

    /// Whether Claude Desktop's config file should carry the Muesli entry: only when assistants
    /// are allowed, the user asked for the config entry, and the extension is not installed
    /// (the extension replaces it).
    public static func wanted(mcpEnabled: Bool, configEntry: Bool, extensionInstalled: Bool) -> Bool {
        claudeConfigWanted(mcpEnabled: mcpEnabled, configEntry: configEntry, extensionInstalled: extensionInstalled)
    }

    /// Adds or removes the entry to match the preferences. Returns the status line.
    public static func sync(_ settings: Preferences, helper: URL, extensionInstalled: Bool, directory: URL = defaultDirectory) -> String {
        claudeConfigSync(prefs: settings, helper: helper.path, extensionInstalled: extensionInstalled, directory: directory.path)
    }

    public static func unregister(directory: URL = defaultDirectory) -> String {
        claudeConfigUnregister(directory: directory.path)
    }
}
