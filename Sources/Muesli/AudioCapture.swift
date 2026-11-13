import AVFoundation
import ScreenCaptureKit
import MuesliCore

/// Captures microphone ("You") and system audio ("Others") as 16 kHz mono Float32
/// and emits ~30 s chunks, so no meeting-bot has to join the call.
final class AudioCapture: NSObject, SCStreamOutput, SCStreamDelegate {
    private let emit: (AudioChunk) -> Void
    private let chunkSeconds: Double = 30
    private let rate: Double = 16_000

    private let engine = AVAudioEngine()
    private var stream: SCStream?
    private let lock = NSLock()
    private var buffers: [String: [Float]] = ["You": [], "Others": []]
    private var emitted: [String: Double] = ["You": 0, "Others": 0]

    init(emit: @escaping (AudioChunk) -> Void) { self.emit = emit }

    func start() async throws {
        try startMic()
        try await startSystemAudio()
    }

    func stop() {
        engine.inputNode.removeTap(onBus: 0)
        engine.stop()
        let s = stream
        stream = nil
        Task { try? await s?.stopCapture() }
        lock.lock(); defer { lock.unlock() }
        for speaker in buffers.keys { flush(speaker, force: true) }   // emit the tail
    }

    // MARK: Microphone
    private func startMic() throws {
        let input = engine.inputNode
        let inFmt = input.outputFormat(forBus: 0)
        let outFmt = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: rate, channels: 1, interleaved: false)!
        guard let conv = AVAudioConverter(from: inFmt, to: outFmt) else { throw NSError(domain: "audio", code: 1) }
        input.installTap(onBus: 0, bufferSize: 4096, format: inFmt) { [weak self] buf, _ in
            guard let self else { return }
            let cap = AVAudioFrameCount(Double(buf.frameLength) * self.rate / inFmt.sampleRate) + 16
            guard let out = AVAudioPCMBuffer(pcmFormat: outFmt, frameCapacity: cap) else { return }
            var fed = false
            conv.convert(to: out, error: nil) { _, status in
                if fed { status.pointee = .noDataNow; return nil }
                fed = true; status.pointee = .haveData; return buf
            }
            if let p = out.floatChannelData?[0] {
                self.append("You", Array(UnsafeBufferPointer(start: p, count: Int(out.frameLength))))
            }
        }
        try engine.start()
    }

    // MARK: System audio (ScreenCaptureKit, needs Screen Recording permission)
    private func startSystemAudio() async throws {
        let content = try await SCShareableContent.current
        guard let display = content.displays.first else { throw NSError(domain: "audio", code: 2) }
        let cfg = SCStreamConfiguration()
        cfg.capturesAudio = true
        cfg.excludesCurrentProcessAudio = true
        cfg.sampleRate = Int(rate)
        cfg.channelCount = 1
        cfg.width = 2; cfg.height = 2                       // we don't want video
        cfg.minimumFrameInterval = CMTime(value: 1, timescale: 1)
        let filter = SCContentFilter(display: display, excludingWindows: [])
        let s = SCStream(filter: filter, configuration: cfg, delegate: self)
        let q = DispatchQueue(label: "muesli.sysaudio")
        try s.addStreamOutput(self, type: .audio, sampleHandlerQueue: q)
        try s.addStreamOutput(self, type: .screen, sampleHandlerQueue: q)
        try await s.startCapture()
        stream = s
    }

    func stream(_ stream: SCStream, didOutputSampleBuffer sb: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .audio, sb.isValid else { return }
        var samples: [Float] = []
        try? sb.withAudioBufferList { abl, _ in
            guard let buf = abl.first, let p = buf.mData?.assumingMemoryBound(to: Float.self) else { return }
            samples = Array(UnsafeBufferPointer(start: p, count: Int(buf.mDataByteSize) / MemoryLayout<Float>.size))
        }
        if !samples.isEmpty { append("Others", samples) }
    }

    // MARK: Chunking
    private func append(_ speaker: String, _ samples: [Float]) {
        lock.lock(); defer { lock.unlock() }
        buffers[speaker, default: []].append(contentsOf: samples)
        flush(speaker, force: false)
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
