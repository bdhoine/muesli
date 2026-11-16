import Foundation

/// Collects 16 kHz mono samples per speaker and emits fixed-length chunks for Whisper.
/// The logic lives in the Rust core. Thread-safe: the mic tap and the system-audio queue append
/// concurrently. `emit` runs on the appending thread and must not call back into the chunker.
public final class AudioChunker {
    private final class Sink: ChunkSink, @unchecked Sendable {
        let emit: (AudioChunk) -> Void
        init(_ emit: @escaping (AudioChunk) -> Void) { self.emit = emit }
        func chunkReady(chunk: AudioChunk) { emit(chunk) }
    }

    private let core: Chunker

    public init(chunkSeconds: Double = 30, rate: Double = 16_000, speakers: [String] = ["You", "Others"],
                emit: @escaping (AudioChunk) -> Void) {
        core = Chunker(chunkSeconds: chunkSeconds, rate: rate, speakers: speakers, sink: Sink(emit))
    }

    public func append(_ speaker: String, _ samples: [Float]) { core.append(speaker: speaker, samples: samples) }

    /// Emits what is left of each stream. Tails of one second or less are dropped.
    public func finish() { core.finish() }
}

extension TranscriptSegment {
    /// Maps one Whisper segment (times in milliseconds, relative to its chunk) onto the meeting timeline.
    /// Returns nil for silence markers and empty text.
    public static func fromWhisper(speaker: String, chunkOffsetSeconds: Double, startMs: Double, endMs: Double, text: String) -> TranscriptSegment? {
        transcriptSegmentFromWhisper(speaker: speaker, chunkOffsetSeconds: chunkOffsetSeconds, startMs: startMs, endMs: endMs, text: text)
    }
}
