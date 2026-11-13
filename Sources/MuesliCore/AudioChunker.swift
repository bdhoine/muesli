import Foundation

/// Collects 16 kHz mono samples per speaker and emits fixed-length chunks for Whisper.
/// Thread-safe: the mic tap and the system-audio queue append concurrently.
public final class AudioChunker {
    private let emit: (AudioChunk) -> Void
    private let chunkSeconds: Double
    private let rate: Double
    private let lock = NSLock()
    private var buffers: [String: [Float]]
    private var emitted: [String: Double]

    public init(chunkSeconds: Double = 30, rate: Double = 16_000, speakers: [String] = ["You", "Others"],
                emit: @escaping (AudioChunk) -> Void) {
        self.chunkSeconds = chunkSeconds
        self.rate = rate
        self.emit = emit
        buffers = Dictionary(uniqueKeysWithValues: speakers.map { ($0, []) })
        emitted = Dictionary(uniqueKeysWithValues: speakers.map { ($0, 0) })
    }

    public func append(_ speaker: String, _ samples: [Float]) {
        lock.lock(); defer { lock.unlock() }
        buffers[speaker, default: []].append(contentsOf: samples)
        flush(speaker, force: false)
    }

    /// Emits what is left of each stream. Tails of one second or less are dropped.
    public func finish() {
        lock.lock(); defer { lock.unlock() }
        for speaker in buffers.keys.sorted() { flush(speaker, force: true) }
    }

    /// Caller holds lock.
    private func flush(_ speaker: String, force: Bool) {
        let need = Int(chunkSeconds * rate)
        while let buf = buffers[speaker], buf.count >= need || (force && buf.count > Int(rate)) {
            let n = min(need, buf.count)
            let chunk = Array(buf.prefix(n))
            buffers[speaker] = Array(buf.dropFirst(n))
            let offset = emitted[speaker, default: 0]
            emitted[speaker] = offset + Double(n) / rate
            emit(AudioChunk(speaker: speaker, offsetSeconds: offset, samples: chunk))
            if buffers[speaker]?.isEmpty ?? true { break }
        }
    }
}

extension TranscriptSegment {
    /// Maps one Whisper segment (times in milliseconds, relative to its chunk) onto the meeting timeline.
    /// Returns nil for silence markers and empty text.
    public static func fromWhisper(speaker: String, chunkOffsetSeconds: Double, startMs: Double, endMs: Double, text: String) -> TranscriptSegment? {
        let text = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty, text != "[BLANK_AUDIO]" else { return nil }
        return TranscriptSegment(start: chunkOffsetSeconds + startMs / 1000, end: chunkOffsetSeconds + endMs / 1000,
                                 speaker: speaker, text: text)
    }
}
