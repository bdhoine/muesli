import XCTest
@testable import MuesliCore

final class AssistantSetupTests: XCTestCase {
    let helper = "/Applications/Muesli.app/Contents/MacOS/muesli-mcp"

    func testTerminalCommandsNameTheServerAndHelper() {
        XCTAssertEqual(AssistantClient.claudeCode.snippet(helper: helper),
                       "claude mcp add --scope user muesli -- '\(helper)'")
        XCTAssertEqual(AssistantClient.codex.snippet(helper: helper), "codex mcp add muesli -- '\(helper)'")
        XCTAssertEqual(AssistantClient.geminiCLI.snippet(helper: helper), "gemini mcp add --scope user muesli '\(helper)'")
    }

    func testPathsWithSpacesAndQuotesStayOneShellWord() {
        let odd = "/Users/me/My Apps/Bob's/muesli-mcp"
        XCTAssertEqual(AssistantClient.codex.snippet(helper: odd), #"codex mcp add muesli -- '/Users/me/My Apps/Bob'\''s/muesli-mcp'"#)
    }

    func testVSCodeCommandCarriesValidJSON() throws {
        let command = AssistantClient.vsCode.snippet(helper: helper)
        XCTAssertTrue(command.hasPrefix("code --add-mcp '"))
        let json = String(command.dropFirst("code --add-mcp '".count).dropLast())
        let object = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(json.utf8)) as? [String: String])
        XCTAssertEqual(object, ["name": "muesli", "command": helper])
    }

    func testConfigSnippetIsAnMCPServersObject() throws {
        for client in [AssistantClient.cursor, .other] {
            let text = client.snippet(helper: helper)
            let root = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any])
            let servers = try XCTUnwrap(root["mcpServers"] as? [String: [String: String]])
            XCTAssertEqual(servers["muesli"]?["command"], helper)
            XCTAssertFalse(text.contains(#"\/"#))
            XCTAssertFalse(client.isTerminalCommand)
        }
    }

    func testEveryClientHasCopy() {
        for client in AssistantClient.allCases {
            XCTAssertFalse(client.name.isEmpty)
            XCTAssertFalse(client.instructions.isEmpty)
            XCTAssertFalse(client.snippet(helper: helper).isEmpty)
        }
    }
}
