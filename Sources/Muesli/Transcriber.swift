import Foundation
import whisper_cpp
import MuesliCore

/// On-device transcription with whisper.cpp. Chunks are consumed one at a time
/// by AppState, so the single Whisper instance is never used concurrently.
final class Transcriber: @unchecked Sendable {
    struct Failure: Error { let code: Int32 }

    private let context: OpaquePointer
    private let languages: [String]
    private let queue = DispatchQueue(label: "muesli.whisper")

    init?(modelURL: URL = Paths.model, languages: [String]) {
        guard FileManager.default.fileExists(atPath: modelURL.path),
              let ctx = modelURL.path.withCString({ whisper_init_from_file($0) }) else { return nil }
        context = ctx
        self.languages = languages
    }

    deinit { whisper_free(context) }

    func transcribe(_ chunk: AudioChunk) async throws -> [TranscriptSegment] {
        guard !chunk.samples.isEmpty else { return [] }
        return try await withCheckedThrowingContinuation { cont in
            queue.async { cont.resume(with: Result { try self.run(chunk) }) }
        }
    }

    private func run(_ chunk: AudioChunk) throws -> [TranscriptSegment] {
        var params = whisper_full_default_params(WHISPER_SAMPLING_GREEDY)
        let language = detectLanguage(chunk.samples, threads: params.n_threads)
        let status = language.withCString { lang in
            params.language = lang
            return chunk.samples.withUnsafeBufferPointer {
                whisper_full(context, params, $0.baseAddress, Int32($0.count))
            }
        }
        guard status == 0 else { throw Failure(code: status) }
        return (0..<whisper_full_n_segments(context)).compactMap { i in
            guard let text = whisper_full_get_segment_text(context, i) else { return nil }
            return TranscriptSegment.fromWhisper(speaker: chunk.speaker, chunkOffsetSeconds: chunk.offsetSeconds,
                                                 startMs: Double(whisper_full_get_segment_t0(context, i)) * 10,
                                                 endMs: Double(whisper_full_get_segment_t1(context, i)) * 10,
                                                 text: String(cString: text))
        }
    }

    private func detectLanguage(_ samples: [Float], threads: Int32) -> String {
        guard languages.count > 1 else { return languages.first ?? "en" }
        let fallback = languages[0]
        let mel = samples.withUnsafeBufferPointer { whisper_pcm_to_mel(context, $0.baseAddress, Int32($0.count), threads) }
        guard mel == 0 else { return fallback }
        var probs = [Float](repeating: 0, count: Int(whisper_lang_max_id()) + 1)
        guard whisper_lang_auto_detect(context, 0, threads, &probs) >= 0 else { return fallback }
        var byCode: [String: Float] = [:]
        for code in languages {
            let id = whisper_lang_id(code)
            if id >= 0 && Int(id) < probs.count { byCode[code] = probs[Int(id)] }
        }
        return Languages.best(probabilities: byCode, allowed: languages) ?? fallback
    }
}
