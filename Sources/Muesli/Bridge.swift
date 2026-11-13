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
