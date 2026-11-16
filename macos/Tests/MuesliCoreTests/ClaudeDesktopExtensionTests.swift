import XCTest
@testable import MuesliCore

final class ClaudeDesktopExtensionTests: IsolatedTestCase {
    var claude: URL { dir.appendingPathComponent("Claude") }

    private func install(_ manifest: [String: Any], folder: String) throws {
        let target = claude.appendingPathComponent("Claude Extensions").appendingPathComponent(folder)
        try FileManager.default.createDirectory(at: target, withIntermediateDirectories: true)
        try JSONSerialization.data(withJSONObject: manifest).write(to: target.appendingPathComponent("manifest.json"))
    }

    func testStateWithoutClaudeDesktop() {
        XCTAssertEqual(ClaudeDesktopExtension.state(bundledVersion: "1.0.0", claude: claude), .claudeMissing)
    }

    func testStateBeforeInstall() throws {
        try FileManager.default.createDirectory(at: claude, withIntermediateDirectories: true)
        try install(["name": "someone-else", "version": "9.9.9"], folder: "other")
        XCTAssertEqual(ClaudeDesktopExtension.state(bundledVersion: "1.0.0", claude: claude), .notInstalled)
    }

    func testInstalledAndOutdated() throws {
        try install(["name": "muesli", "version": "0.2.0"], folder: "local.mcpb.muesli.muesli")
        XCTAssertEqual(ClaudeDesktopExtension.state(bundledVersion: "0.2.0", claude: claude), .installed(version: "0.2.0"))
        XCTAssertEqual(ClaudeDesktopExtension.state(bundledVersion: "0.10.0", claude: claude), .outdated(installed: "0.2.0", bundled: "0.10.0"))
        XCTAssertEqual(ClaudeDesktopExtension.state(bundledVersion: "0.1.9", claude: claude), .installed(version: "0.2.0"))
    }

    func testVersionOrdering() {
        XCTAssertTrue(ClaudeDesktopExtension.isOlder("0.2", than: "0.2.1"))
        XCTAssertFalse(ClaudeDesktopExtension.isOlder("0.2.0", than: "0.2"))
        XCTAssertTrue(ClaudeDesktopExtension.isOlder("0.9.0", than: "0.10.0"))
    }
}
