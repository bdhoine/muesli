import XCTest
@testable import MuesliCore

final class ClaudeDesktopExtensionTests: IsolatedTestCase {
    var claude: URL { dir.appendingPathComponent("Claude") }

    private func install(_ manifest: [String: Any], folder: String) throws {
        let target = ClaudeDesktopExtension.extensionsDirectory(in: claude).appendingPathComponent(folder)
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

    func testManifestHasWhatClaudeDesktopNeeds() throws {
        let data = ClaudeDesktopExtension.manifestJSON(version: "1.2.3")
        let m = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
        for key in ["manifest_version", "name", "version", "description", "author", "server"] {
            XCTAssertNotNil(m[key], key)
        }
        XCTAssertEqual(m["name"] as? String, "muesli")
        XCTAssertEqual(m["display_name"] as? String, "Muesli")
        XCTAssertEqual(m["version"] as? String, "1.2.3")
        XCTAssertEqual(m["icon"] as? String, "icon.png")
        let server = try XCTUnwrap(m["server"] as? [String: Any])
        XCTAssertEqual(server["type"] as? String, "binary")
        XCTAssertEqual(server["entry_point"] as? String, "server/muesli-mcp")
        let config = try XCTUnwrap(server["mcp_config"] as? [String: Any])
        XCTAssertEqual(config["command"] as? String, "${__dirname}/server/muesli-mcp")
        let tools = try XCTUnwrap(m["tools"] as? [[String: String]]).compactMap { $0["name"] }
        XCTAssertEqual(Set(tools), ["get_status", "list_transcripts", "get_transcript", "rename_transcript", "set_attendees", "rename_speaker", "delete_transcript"])
        XCTAssertFalse(String(decoding: data, as: UTF8.self).contains(#"\/"#))
    }
}
