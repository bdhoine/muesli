import XCTest
@testable import MuesliCore

final class WhisperModelTests: IsolatedTestCase {
    func testDefaultsToSmallInSupportFolder() {
        let p = Preferences()
        XCTAssertEqual(p.model, .small)
        XCTAssertEqual(p.modelURL, Paths.support.appendingPathComponent("ggml-small.bin"))
    }

    func testCustomPathTakesPrecedence() {
        var p = Preferences()
        p.model = .large
        p.customModelPath = "/tmp/my-model.bin"
        XCTAssertEqual(p.modelURL.path, "/tmp/my-model.bin")
        p.customModelPath = ""
        XCTAssertEqual(p.modelURL, WhisperModel.large.url)
    }

    func testModelFilesAndURLs() {
        XCTAssertEqual(WhisperModel.medium.fileName, "ggml-medium.bin")
        XCTAssertEqual(WhisperModel.large.fileName, "ggml-large-v3.bin")
        for m in WhisperModel.allCases {
            XCTAssertTrue(m.downloadURL.absoluteString.hasSuffix(m.fileName))
            XCTAssertLessThan(m.minimumValidBytes, m.approximateBytes)
        }
    }

    func testOldConfigWithoutModelDecodes() throws {
        let p = try JSONDecoder().decode(Preferences.self, from: Data(#"{"autoStop":true}"#.utf8))
        XCTAssertTrue(p.autoStop)
        XCTAssertEqual(p.model, .small)
        XCTAssertNil(p.customModelPath)
    }

    func testUnknownModelFallsBackToSmall() throws {
        let p = try JSONDecoder().decode(Preferences.self, from: Data(#"{"model":"huge"}"#.utf8))
        XCTAssertEqual(p.model, .small)
    }

    func testModelSettingsRoundTrip() {
        var p = Preferences()
        p.model = .medium
        p.customModelPath = "/x/y.bin"
        p.save()
        XCTAssertEqual(Preferences.load(), p)
    }
}

final class OnboardingPreferencesTests: IsolatedTestCase {
    func testNewInstallNeedsOnboarding() {
        XCTAssertFalse(Preferences().onboardingDone)
        XCTAssertFalse(Preferences.load().onboardingDone)
    }

    func testConfigFromBeforeOnboardingCountsAsDone() throws {
        let p = try JSONDecoder().decode(Preferences.self, from: Data(#"{"autoStop":true}"#.utf8))
        XCTAssertTrue(p.onboardingDone)
    }

    func testOnboardingFlagRoundTrips() {
        var p = Preferences()
        p.onboardingDone = true
        p.save()
        XCTAssertTrue(Preferences.load().onboardingDone)
    }

    func testMacLanguagesAreReflectedInDefaults() {
        XCTAssertEqual(Languages.defaults(preferred: ["nl-BE", "en"]), ["nl", "en"])
        XCTAssertEqual(Languages.defaults(preferred: ["en-BE"]), ["en"])
        var p = Preferences()
        XCTAssertNil(p.languages)
        XCTAssertEqual(p.effectiveLanguages, Languages.defaults())
        p.languages = ["fr"]
        XCTAssertEqual(p.effectiveLanguages, ["fr"])
    }
}
