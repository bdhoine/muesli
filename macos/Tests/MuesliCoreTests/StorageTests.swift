import XCTest
@testable import MuesliCore

/// Preferences and meeting files: older files must keep loading.
final class StorageTests: IsolatedTestCase {
    func testPreferencesRoundTripAndDefaults() throws {
        XCTAssertEqual(Preferences.load(), Preferences())
        var s = Preferences()
        s.mcpEnabled = true; s.claudeDesktopConfigEntry = true; s.recentLimit = 3
        s.save()
        XCTAssertEqual(Preferences.load(), s)

        try Data(#"{"mcpEnabled":true}"#.utf8).write(to: Paths.config)             // older file with fewer keys
        XCTAssertEqual(Preferences.load().recentLimit, Preferences().recentLimit)
        XCTAssertTrue(Preferences.load().mcpEnabled)

        let remoteEra = #"{"mcpEnabled":true,"mcpMode":"remote","httpPort":8747,"teamDomain":"t","audience":"a","allowedEmail":"e","remoteProvider":"cloudflare"}"#
        try Data(remoteEra.utf8).write(to: Paths.config)                           // files from before remote mode was removed
        XCTAssertTrue(Preferences.load().mcpEnabled)
    }

    func testMeetingDecodesFilesWithoutState() throws {
        let old = #"{"id":"a","title":"t","source":"manual","startedAt":"2026-11-10T14:30:00Z","attendees":[],"transcript":[]}"#
        let m = try JSONCoding.decoder().decode(Meeting.self, from: Data(old.utf8))
        XCTAssertEqual(m.state, .ready)
    }
}
