import AppKit
import MuesliCore

/// Connects Muesli to Claude Desktop according to Preferences. The app itself never opens a
/// socket: assistants start the bundled `muesli-mcp` themselves and talk to it over stdio.
@MainActor @Observable
final class BridgeController {
    private(set) var localStatus = ""
    private(set) var claudeExtension: ClaudeDesktopExtension.State = .notInstalled
    private var settings = Preferences()

    /// The muesli-mcp binary inside the app bundle.
    static var helperURL: URL? {
        let candidates = [Bundle.main.url(forAuxiliaryExecutable: "muesli-mcp"),
                          Bundle.main.executableURL?.deletingLastPathComponent().appendingPathComponent("muesli-mcp")]
        return candidates.compactMap { $0 }.first { FileManager.default.isExecutableFile(atPath: $0.path) }
    }

    static var extensionURL: URL? {
        Bundle.main.url(forResource: "Muesli", withExtension: "mcpb")
    }

    var extensionInstalled: Bool {
        switch claudeExtension {
        case .installed, .outdated: true
        case .notInstalled, .claudeMissing: false
        }
    }

    func apply(_ settings: Preferences) {
        self.settings = settings
        claudeExtension = ClaudeDesktopExtension.state()
        guard BridgeController.helperURL != nil else {
            localStatus = "muesli-mcp is missing from the app bundle; rebuild with build.sh."
            return
        }
        applyConfigEntry()
    }

    func refreshExtension() {
        let wasInstalled = extensionInstalled
        claudeExtension = ClaudeDesktopExtension.state()
        if extensionInstalled && !wasInstalled { applyConfigEntry() }
    }

    func installExtension() {
        guard let url = BridgeController.extensionURL else { return }
        NSWorkspace.shared.open(url)
    }

    private func applyConfigEntry() {
        guard let helper = BridgeController.helperURL else { return }
        localStatus = ClaudeDesktopConfig.sync(settings, helper: helper, extensionInstalled: extensionInstalled)
    }
}
