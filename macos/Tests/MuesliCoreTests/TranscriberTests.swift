import XCTest
@testable import MuesliCore

/// Transcription runs in the Rust core. The real-model test only runs on request:
///   MUESLI_TEST_MODEL=~/Library/Application\ Support/Muesli/ggml-large-v3-turbo-q5_0.bin \
///   MUESLI_TEST_VAD=~/Library/Application\ Support/Muesli/ggml-silero-v5.1.2.bin \
///   MUESLI_TEST_WAV=/path/to/speech.wav swift test --filter TranscriberTests
final class TranscriberTests: XCTestCase {
    func testMissingModelGivesNil() {
        XCTAssertNil(transcriberLoad(modelPath: "/nonexistent/model.bin", vadPath: "/nonexistent/vad.bin", languages: ["en"]))
    }

    private func load() throws -> (Transcriber, [Float]) {
        let env = ProcessInfo.processInfo.environment
        guard let model = env["MUESLI_TEST_MODEL"], let vad = env["MUESLI_TEST_VAD"], let wav = env["MUESLI_TEST_WAV"] else {
            throw XCTSkip("set MUESLI_TEST_MODEL, MUESLI_TEST_VAD and MUESLI_TEST_WAV to run this")
        }
        let bytes = [UInt8](try Data(contentsOf: URL(fileURLWithPath: wav)))
        let start = try XCTUnwrap(bytes.indices.dropLast(4).first { Array(bytes[$0..<$0 + 4]) == Array("data".utf8) }) + 8
        let samples = stride(from: start, to: bytes.count - 1, by: 2).map {
            Float(Int16(bitPattern: UInt16(bytes[$0]) | UInt16(bytes[$0 + 1]) << 8)) / 32768
        }
        let expand = { (p: String) in (p as NSString).expandingTildeInPath }
        let transcriber = try XCTUnwrap(transcriberLoad(modelPath: expand(model), vadPath: expand(vad), languages: ["en", "nl"]))
        return (transcriber, samples)
    }

    func testTranscribesRealSpeech() throws {
        let (transcriber, samples) = try load()
        let segments = try transcriber.transcribe(chunk: AudioChunk(speaker: "You", offsetSeconds: 30, samples: samples))
        let text = segments.map(\.text).joined(separator: " ").lowercased()
        XCTAssertTrue(segments.allSatisfy { $0.start >= 30 && $0.speaker == "You" })
        XCTAssertTrue(text.contains("meeting"), text)
    }

    /// Without voice detection Whisper turns silence into "Thank you."
    func testSilenceGivesNoTextInsteadOfAHallucination() throws {
        let (transcriber, _) = try load()
        let silence = AudioChunk(speaker: "Others", offsetSeconds: 0, samples: [Float](repeating: 0, count: 30 * 16_000))
        XCTAssertEqual(try transcriber.transcribe(chunk: silence), [])
    }
}
