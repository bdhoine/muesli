import XCTest
@testable import MuesliCore

final class PathsTests: IsolatedTestCase {
    private func freshDefaults() -> UserDefaults {
        let suite = "muesli-tests-\(UUID().uuidString)"
        UserDefaults().removePersistentDomain(forName: suite)
        return UserDefaults(suiteName: suite)!
    }

    func testTranscriptsFolderIsRememberedInConfigJSON() throws {
        unsetenv("TRANSCRIPT_DIR")
        XCTAssertFalse(Paths.transcriptsIsCustom)
        let chosen = dir.appendingPathComponent("chosen", isDirectory: true)
        Paths.setTranscripts(chosen)
        XCTAssertTrue(Paths.transcriptsIsCustom)
        XCTAssertEqual(Paths.transcripts.standardizedFileURL.path, chosen.standardizedFileURL.path)
        XCTAssertTrue(try String(contentsOf: Paths.config, encoding: .utf8).contains("transcriptsFolder"))
        Paths.setTranscripts(nil)
        XCTAssertFalse(Paths.transcriptsIsCustom)
    }

    func testEnvironmentOverridesTheChosenFolder() {
        Paths.setTranscripts(dir.appendingPathComponent("chosen"))
        XCTAssertTrue(Paths.transcriptsOverriddenByEnvironment)         // the test case sets TRANSCRIPT_DIR
        XCTAssertEqual(Paths.transcripts.standardizedFileURL.path, transcriptsDir.standardizedFileURL.path)
    }

    func testFolderFromOlderVersionsMovesFromUserDefaultsToConfigJSON() {
        unsetenv("TRANSCRIPT_DIR")
        let defaults = freshDefaults()
        defaults.set("/Users/me/Notes", forKey: Paths.legacyTranscriptDirKey)
        Paths.migrateLegacyTranscriptsFolder(defaults: defaults)
        XCTAssertEqual(Preferences.load().transcriptsFolder, "/Users/me/Notes")
        XCTAssertNil(defaults.string(forKey: Paths.legacyTranscriptDirKey))
    }

    func testMigrationNeverOverwritesAFolderAlreadyInConfigJSON() {
        unsetenv("TRANSCRIPT_DIR")
        Paths.setTranscripts(URL(fileURLWithPath: "/new"))
        let defaults = freshDefaults()
        defaults.set("/old", forKey: Paths.legacyTranscriptDirKey)
        Paths.migrateLegacyTranscriptsFolder(defaults: defaults)
        XCTAssertEqual(Preferences.load().transcriptsFolder, "/new")
    }
}
