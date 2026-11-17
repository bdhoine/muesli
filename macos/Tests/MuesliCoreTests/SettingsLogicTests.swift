import XCTest
@testable import MuesliCore

/// Rules behind the Settings window. The Windows app has the same tests in windows/crates/core.
final class SettingsLogicTests: IsolatedTestCase {
    func testSwitchingToSystemLanguagesKeepsTheOwnList() {
        var p = Preferences()
        p.languages = ["fr", "de"]
        p.setUsesSystemLanguages(true)
        XCTAssertNil(p.languages)
        XCTAssertTrue(p.usesSystemLanguages)
        XCTAssertEqual(p.savedLanguages, ["fr", "de"])
        p.setUsesSystemLanguages(false, systemDefaults: ["nl", "en"])
        XCTAssertEqual(p.languages, ["fr", "de"])                     // the own list is back
        p.setUsesSystemLanguages(false, systemDefaults: ["nl", "en"])
        XCTAssertEqual(p.languages, ["fr", "de"])                     // switching off twice changes nothing
    }

    func testFirstOwnListStartsFromTheSystemLanguages() {
        var p = Preferences()
        p.setUsesSystemLanguages(false, systemDefaults: ["nl", "en"])
        XCTAssertEqual(p.languages, ["nl", "en"])
        p.setUsesSystemLanguages(true)
        p.setUsesSystemLanguages(true)                                // on twice must not wipe the saved list
        XCTAssertEqual(p.savedLanguages, ["nl", "en"])
    }

    func testSavedLanguagesRoundTripAndStayOutOfOldFiles() throws {
        Preferences().save()
        XCTAssertFalse(try String(contentsOf: Paths.config, encoding: .utf8).contains("savedLanguages"))
        var p = Preferences()
        p.savedLanguages = ["fr"]
        p.save()
        XCTAssertEqual(Preferences.load().savedLanguages, ["fr"])
    }

    func testClaudeCountsAsMissingWhenTheAppIsNotInstalled() throws {
        let claude = dir.appendingPathComponent("Claude")
        try FileManager.default.createDirectory(at: claude, withIntermediateDirectories: true)   // leftover data folder
        XCTAssertEqual(ClaudeDesktopExtension.state(bundledVersion: "1.0.0", claude: claude, appInstalled: false), .claudeMissing)
        XCTAssertEqual(ClaudeDesktopExtension.state(bundledVersion: "1.0.0", claude: claude, appInstalled: true), .notInstalled)
        XCTAssertEqual(ClaudeDesktopExtension.state(bundledVersion: "1.0.0", claude: claude), .notInstalled)
        XCTAssertEqual(ClaudeDesktopExtension.state(bundledVersion: "1.0.0", claude: dir.appendingPathComponent("none"), appInstalled: true), .notInstalled)
    }
}
