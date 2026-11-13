import Foundation
import SwiftWhisper
import MuesliCore

/// On-device transcription with whisper.cpp. Chunks are consumed one at a time
/// by AppState, so the single Whisper instance is never used concurrently.
final class Transcriber {
    private let whisper: Whisper

    init?(modelURL: URL = Paths.model) {
        guard FileManager.default.fileExists(atPath: modelURL.path) else { return nil }
        whisper = Whisper(fromFileURL: modelURL)
        whisper.params.language = .auto   // Dutch/English/French mixes are common
    }

    func transcribe(_ chunk: AudioChunk) async throws -> [TranscriptSegment] {
        let segs = try await whisper.transcribe(audioFrames: chunk.samples)
        // SwiftWhisper reports start/end in milliseconds.
        return segs.compactMap { s in
            TranscriptSegment.fromWhisper(speaker: chunk.speaker, chunkOffsetSeconds: chunk.offsetSeconds,
                                          startMs: Double(s.startTime), endMs: Double(s.endTime), text: s.text)
        }
    }
}
