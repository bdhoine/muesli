import XCTest
@testable import MuesliCore

final class WhisperModelTests: IsolatedTestCase {
    func testDefaultsToTheOneFixedModelInTheSupportFolder() {
        let p = Preferences()
        XCTAssertEqual(p.modelURL, Paths.support.appendingPathComponent("ggml-large-v3-turbo-q5_0.bin"))
        XCTAssertEqual(p.vadModelURL, Paths.support.appendingPathComponent("ggml-silero-v5.1.2.bin"))
        XCTAssertEqual(SpeechModel.info.fileName, "ggml-large-v3-turbo-q5_0.bin")
    }

    func testCustomPathReplacesTheSpeechModelButNotTheVoiceModel() {
        var p = Preferences()
        p.customModelPath = "/tmp/my-model.bin"
        XCTAssertEqual(p.modelURL.path, "/tmp/my-model.bin")
        XCTAssertEqual(p.vadModelURL, Paths.support.appendingPathComponent("ggml-silero-v5.1.2.bin"))
        p.customModelPath = ""
        XCTAssertEqual(p.modelURL, Paths.support.appendingPathComponent(SpeechModel.info.fileName))
    }

    func testTheOldModelChoiceInConfigIsIgnored() throws {
        let p = try preferences(fromJSON: #"{"model":"medium","autoStop":true}"#)
        XCTAssertTrue(p.autoStop)
        XCTAssertNil(p.customModelPath)
        p.save()
        XCTAssertFalse(try String(contentsOf: Paths.config, encoding: .utf8).contains("\"model\""))
    }

    func testCustomModelSettingRoundTrips() {
        var p = Preferences()
        p.customModelPath = "/x/y.bin"
        p.save()
        XCTAssertEqual(Preferences.load(), p)
    }

    func testOldModelFilesAreListedAndRemoved() throws {
        XCTAssertEqual(SpeechModel.legacy.files, 0)
        try Data(count: 10).write(to: Paths.support.appendingPathComponent("ggml-medium.bin"))
        XCTAssertEqual(SpeechModel.legacy.files, 1)
        XCTAssertEqual(SpeechModel.legacy.bytes, 10)
        XCTAssertEqual(SpeechModel.removeLegacy(), 1)
        XCTAssertEqual(SpeechModel.legacy.files, 0)
    }
}

final class OnboardingPreferencesTests: IsolatedTestCase {
    func testNewInstallNeedsOnboarding() {
        XCTAssertFalse(Preferences().onboardingDone)
        XCTAssertFalse(Preferences.load().onboardingDone)
    }

    func testConfigFromBeforeOnboardingCountsAsDone() throws {
        let p = try preferences(fromJSON: #"{"autoStop":true}"#)
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
