import XCTest
@testable import MuesliCore

/// The original app: chunking audio, mapping Whisper output, writing transcript files.
final class TranscriptionTests: IsolatedTestCase {

    // MARK: Audio chunking

    private func collect(chunkSeconds: Double = 2, rate: Double = 10, _ body: (AudioChunker) -> Void) -> [AudioChunk] {
        var out: [AudioChunk] = []
        let chunker = AudioChunker(chunkSeconds: chunkSeconds, rate: rate) { out.append($0) }
        body(chunker)
        return out
    }

    func testNothingIsEmittedBeforeAFullChunk() {
        let chunks = collect { $0.append("You", [Float](repeating: 0, count: 19)) }
        XCTAssertTrue(chunks.isEmpty)
    }

    func testEmitsFullChunksWithRunningOffsets() {
        let chunks = collect { c in
            c.append("You", (0..<15).map(Float.init))
            c.append("You", (15..<45).map(Float.init))            // 45 samples = 2 chunks of 20 + 5 left
        }
        XCTAssertEqual(chunks.map(\.samples.count), [20, 20])
        XCTAssertEqual(chunks.map(\.offsetSeconds), [0, 2])
        XCTAssertEqual(chunks[1].samples.first, 20)                // no sample lost or repeated at the boundary
        XCTAssertEqual(chunks.map(\.speaker), ["You", "You"])
    }

    func testSpeakersAreChunkedIndependently() {
        let chunks = collect { c in
            c.append("You", [Float](repeating: 1, count: 20))
            c.append("Others", [Float](repeating: 2, count: 10))
            c.append("Others", [Float](repeating: 2, count: 10))
            c.append("You", [Float](repeating: 1, count: 20))
        }
        XCTAssertEqual(chunks.map(\.speaker), ["You", "Others", "You"])
        XCTAssertEqual(chunks.map(\.offsetSeconds), [0, 0, 2])
    }

    func testFinishEmitsTheTailButDropsUnderOneSecond() {
        let chunks = collect { c in
            c.append("You", [Float](repeating: 1, count: 35))      // one chunk + 1.5 s tail
            c.append("Others", [Float](repeating: 2, count: 10))   // exactly 1 s: dropped
            c.finish()
        }
        XCTAssertEqual(chunks.map(\.speaker), ["You", "You"])
        XCTAssertEqual(chunks.map(\.samples.count), [20, 15])
        XCTAssertEqual(chunks[1].offsetSeconds, 2)
    }

    func testDefaultsAreThirtySecondsAtSixteenKilohertz() {
        var out: [AudioChunk] = []
        let chunker = AudioChunker { out.append($0) }
        chunker.append("You", [Float](repeating: 0, count: 479_999))
        XCTAssertTrue(out.isEmpty)
        chunker.append("You", [0])
        XCTAssertEqual(out.first?.samples.count, 480_000)
    }

    func testConcurrentAppendsLoseNothing() {
        let lock = NSLock()
        var total = 0
        let chunker = AudioChunker(chunkSeconds: 1, rate: 100) { chunk in lock.lock(); total += chunk.samples.count; lock.unlock() }
        DispatchQueue.concurrentPerform(iterations: 8) { i in
            for _ in 0..<50 { chunker.append(i % 2 == 0 ? "You" : "Others", [Float](repeating: 0, count: 25)) }
        }
        XCTAssertEqual(total, 8 * 50 * 25)
    }

    // MARK: Whisper segments

    func testWhisperTimesAreMillisecondsRelativeToTheChunk() {
        let s = TranscriptSegment.fromWhisper(speaker: "Others", chunkOffsetSeconds: 60, startMs: 1500, endMs: 4250, text: "  Hello there \n")
        XCTAssertEqual(s, TranscriptSegment(start: 61.5, end: 64.25, speaker: "Others", text: "Hello there"))
    }

    func testBlankAndEmptySegmentsAreDropped() {
        XCTAssertNil(TranscriptSegment.fromWhisper(speaker: "You", chunkOffsetSeconds: 0, startMs: 0, endMs: 1000, text: " [BLANK_AUDIO] "))
        XCTAssertNil(TranscriptSegment.fromWhisper(speaker: "You", chunkOffsetSeconds: 0, startMs: 0, endMs: 1000, text: "  \n"))
    }

    // MARK: Transcript files

    func testSaveWritesMarkdownAndJSONPair() throws {
        let url = Store.save(meeting("m1", state: .ready, title: "Weekly sync: Q4 / plans!"))
        XCTAssertEqual(url.pathExtension, "md")
        XCTAssertEqual(url.deletingLastPathComponent().standardizedFileURL.path, transcriptsDir.standardizedFileURL.path)
        let name = url.deletingPathExtension().lastPathComponent
        XCTAssertNotNil(name.range(of: #"^\d{4}-\d{2}-\d{2}_\d{4}_Weekly-sync-Q4-plans$"#, options: .regularExpression), name)

        let json = url.deletingPathExtension().appendingPathExtension("json")
        let back = try XCTUnwrap(MeetingStore.importTranscripts(from: transcriptsDir).first)
        XCTAssertEqual(back.id, "m1")
        XCTAssertEqual(back.transcript.count, 2)
        XCTAssertEqual(back.state, .ready)
        let raw = try XCTUnwrap(JSONSerialization.jsonObject(with: Data(contentsOf: json)) as? [String: Any])
        for key in ["id", "title", "source", "startedAt", "endedAt", "attendees", "transcript"] { XCTAssertNotNil(raw[key], key) }
    }

    func testFileNameFallsBackAndIsBounded() {
        XCTAssertTrue(Store.fileStem(meeting("a", state: .ready, title: "???")).hasSuffix("_Meeting"))
        let long = Store.fileStem(meeting("a", state: .ready, title: String(repeating: "a", count: 200)))
        XCTAssertEqual(long.split(separator: "_").last?.count, 60)
    }

    func testMarkdownLayout() {
        var m = meeting("m1", state: .ready, segments: 0)
        m.transcript = [TranscriptSegment(start: 0, end: 4, speaker: "You", text: "Hi"),
                        TranscriptSegment(start: 754, end: 760, speaker: "Others", text: "Bye")]
        let md = Store.markdown(m)
        XCTAssertTrue(md.hasPrefix("# Weekly sync\n\n- Started: 2027-01-15T08:00:00Z\n- Ended: 2027-01-15T08:30:00Z\n- Duration: 30 min\n- Source: calendar\n- Attendees: Ann\n"), md)
        XCTAssertTrue(md.contains("\n## Transcript\n\n[00:00] **You**: Hi\n\n[12:34] **Others**: Bye\n\n"), md)
    }

    func testMarkdownOmitsWhatIsUnknown() {
        var m = meeting("m1", state: .recording)
        m.attendees = []
        let md = Store.markdown(m)
        XCTAssertFalse(md.contains("Ended"))
        XCTAssertFalse(md.contains("Attendees"))
        XCTAssertFalse(md.contains("Partial"))
    }

    func testTimestampsPastAnHourKeepCountingMinutes() {
        var m = meeting("long", state: .ready, segments: 0)
        m.transcript = [TranscriptSegment(start: 3725, end: 3730, speaker: "You", text: "x")]
        XCTAssertTrue(Store.markdown(m).contains("[62:05] **You**: x"))
    }

    func testRecentListsNewestMarkdownFirstAndHonoursLimit() throws {
        for (i, title) in ["one", "two", "three"].enumerated() {
            let url = Store.save(meeting("m\(i)", state: .ready, title: title, started: Date(timeIntervalSince1970: 1_800_000_000 + Double(i) * 3600)))
            try FileManager.default.setAttributes([.modificationDate: Date(timeIntervalSince1970: 1_800_000_000 + Double(i))], ofItemAtPath: url.path)
        }
        let recent = Store.recent(limit: 2)
        XCTAssertEqual(recent.map { $0.lastPathComponent.hasSuffix("three.md") || $0.lastPathComponent.hasSuffix("two.md") }, [true, true])
        XCTAssertTrue(recent[0].lastPathComponent.hasSuffix("three.md"))
        XCTAssertTrue(Store.recent().allSatisfy { $0.pathExtension == "md" })
        XCTAssertEqual(Store.recent().count, 3)
    }

    func testTranscriptDirEnvironmentOverride() {
        XCTAssertTrue(Paths.transcriptsOverriddenByEnvironment)
        XCTAssertEqual(Paths.transcripts.standardizedFileURL.path, transcriptsDir.standardizedFileURL.path)
        var isDir: ObjCBool = false
        XCTAssertTrue(FileManager.default.fileExists(atPath: transcriptsDir.path, isDirectory: &isDir) && isDir.boolValue)   // created on demand
    }

    // MARK: Languages

    func testDefaultLanguagesAreOSLanguagePlusEnglish() {
        XCTAssertEqual(Languages.defaults(preferred: ["en-US"]), ["en"])
        XCTAssertEqual(Languages.defaults(preferred: ["nl-BE", "en-US"]), ["nl", "en"])
        XCTAssertEqual(Languages.defaults(preferred: ["fr"]), ["fr", "en"])
        XCTAssertEqual(Languages.defaults(preferred: ["xx-YY"]), ["en"])
        XCTAssertEqual(Languages.defaults(preferred: []), ["en"])
    }

    func testBestLanguageIgnoresLanguagesOutsideTheSet() {
        let probs: [String: Float] = ["fi": 0.6, "nl": 0.3, "en": 0.1]
        XCTAssertEqual(Languages.best(probabilities: probs, allowed: ["en", "nl"]), "nl")
        XCTAssertNil(Languages.best(probabilities: probs, allowed: []))
    }

    func testLanguagesMissingFromOldConfigUseTheDefault() throws {
        let p = try preferences(fromJSON: #"{"mcpEnabled": true}"#)
        XCTAssertNil(p.languages)
        XCTAssertEqual(p.effectiveLanguages, Languages.defaults())
    }

    func testChosenLanguagesRoundTripAndAreValidated() throws {
        var p = Preferences()
        p.languages = ["en", "nl"]
        p.save()
        XCTAssertEqual(Preferences.load().languages, ["en", "nl"])
        XCTAssertEqual(Preferences.load().effectiveLanguages, ["en", "nl"])

        p.languages = ["nl-BE", "xx", "nl"]
        XCTAssertEqual(p.effectiveLanguages, ["nl"])
        p.languages = ["xx"]
        XCTAssertEqual(p.effectiveLanguages, Languages.defaults())
    }
}
