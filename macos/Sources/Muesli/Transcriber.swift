import Foundation
import MuesliCore

/// On-device transcription with whisper.cpp, through the shared Rust core (the same code the
/// Windows app runs). Chunks are consumed one at a time by AppState, so the single Whisper
/// instance is never used concurrently.
final class Transcriber: @unchecked Sendable {
    private let core: MuesliFFI.Transcriber
    private let queue = DispatchQueue(label: "muesli.whisper")

    /// nil when a file is missing or Whisper cannot load it. The voice activity model keeps Whisper
    /// from inventing text for silence.
    init?(modelURL: URL, vadURL: URL, languages: [String]) {
        guard let core = transcriberLoad(modelPath: modelURL.path, vadPath: vadURL.path, languages: languages) else { return nil }
        self.core = core
    }

    func transcribe(_ chunk: AudioChunk) async throws -> [TranscriptSegment] {
        guard !chunk.samples.isEmpty else { return [] }
        return try await withCheckedThrowingContinuation { cont in
            queue.async { cont.resume(with: Result { try self.core.transcribe(chunk: chunk) }) }
        }
    }
}
