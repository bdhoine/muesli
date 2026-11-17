import XCTest
@testable import MuesliCore

/// Reading, watching and cleaning up audio files is the Rust core; these only check that the
/// bindings are wired. The real-model test runs on request:
///   MUESLI_TEST_MODEL=... MUESLI_TEST_VAD=... MUESLI_TEST_AUDIO=/path/to/speech.m4a swift test --filter AudioFileTests
final class AudioFileTests: IsolatedTestCase {
    func testWhichFilesCountAsAudio() {
        XCTAssertTrue(audioExtensions().contains("m4a"))
        XCTAssertTrue(isAudioFile(path: "/x/call.MP3"))
        XCTAssertFalse(isAudioFile(path: "/x/notes.txt"))
    }

    func testDropFolderOffersAFileOnceItStoppedChanging() throws {
        let drop = dir.appendingPathComponent("drop")
        try FileManager.default.createDirectory(at: drop, withIntermediateDirectories: true)
        let file = drop.appendingPathComponent("call.wav")
        try Data(repeating: 1, count: 100).write(to: file)
        let watch = DropFolder(paths: Paths.current)
        XCTAssertEqual(watch.poll(folder: drop.path), [])          // it may still be copying
        XCTAssertEqual(watch.poll(folder: drop.path), [file.path])
        XCTAssertEqual(watch.poll(folder: drop.path), [])          // once
    }

    func testCleanupModes() throws {
        let file = dir.appendingPathComponent("gone.mp3")
        try Data(repeating: 1, count: 10).write(to: file)
        XCTAssertNil(audioCleanUp(path: file.path, mode: .keep))
        XCTAssertTrue(FileManager.default.fileExists(atPath: file.path))
        XCTAssertNil(audioCleanUp(path: file.path, mode: .delete))
        XCTAssertFalse(FileManager.default.fileExists(atPath: file.path))
        XCTAssertNotNil(audioCleanUp(path: file.path, mode: .delete))   // already gone
    }

    func testDropFolderPreferencesRoundTrip() {
        var prefs = Preferences.load()
        XCTAssertNil(prefs.dropFolder)
        XCTAssertEqual(prefs.dropFolderCleanup, .keep)
        prefs.dropFolder = "/Users/me/Audio"
        prefs.dropFolderCleanup = .trash
        prefs.save()
        XCTAssertEqual(Preferences.load().dropFolder, "/Users/me/Audio")
        XCTAssertEqual(Preferences.load().dropFolderCleanup, .trash)
    }

    private final class Seen: ImportObserver, @unchecked Sendable {
        var fractions: [Double] = []
        func progress(fraction: Double) { fractions.append(fraction) }
    }

    func testTranscribesARealAudioFile() throws {
        let env = ProcessInfo.processInfo.environment
        guard let model = env["MUESLI_TEST_MODEL"], let vad = env["MUESLI_TEST_VAD"], let audio = env["MUESLI_TEST_AUDIO"] else {
            throw XCTSkip("set MUESLI_TEST_MODEL, MUESLI_TEST_VAD and MUESLI_TEST_AUDIO to run this")
        }
        let expand = { (p: String) in (p as NSString).expandingTildeInPath }
        let transcriber = try XCTUnwrap(transcriberLoad(modelPath: expand(model), vadPath: expand(vad), languages: ["en"]))
        let seen = Seen()
        let meeting = try FileImporter().import(transcriber: transcriber, path: expand(audio), paths: Paths.current, observer: seen)
        XCTAssertEqual(meeting.source, "file")
        XCTAssertEqual(meeting.state, MeetingState.ready)
        XCTAssertFalse(meeting.transcript.isEmpty)
        XCTAssertEqual(seen.fractions.last, 1)
    }
}
