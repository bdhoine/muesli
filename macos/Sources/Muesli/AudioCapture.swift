import AVFoundation
import CoreAudio
import AudioToolbox
import MuesliCore

/// Captures microphone ("You") and system audio ("Others") as 16 kHz mono Float32
/// and emits ~30 s chunks, so no meeting-bot has to join the call.
///
/// System audio uses a Core Audio process tap, so macOS asks for "System Audio Recording Only"
/// instead of Screen Recording.
///
/// Devices change during a call (headphones plugged in, AirPods connecting). Each stream is then
/// restarted on the new device; only if that fails is `onError` called, so the app can save what
/// it has.
final class AudioCapture {
    struct Failure: LocalizedError {
        let step: String
        let status: OSStatus
        var errorDescription: String? { "System audio \(step) failed (error \(status))." }
    }

    private let chunker: AudioChunker
    private let rate: Double = 16_000
    private let outFmt = AVAudioFormat(commonFormat: .pcmFormatFloat32, sampleRate: 16_000, channels: 1, interleaved: false)!

    private let engine = AVAudioEngine()
    private let tapQueue = DispatchQueue(label: "muesli.sysaudio", qos: .userInitiated)
    private var tapID = AudioObjectID(kAudioObjectUnknown)
    private var aggregateID = AudioObjectID(kAudioObjectUnknown)
    private var ioProc: AudioDeviceIOProcID?

    private let onError: (String) -> Void
    private var running = false
    private var engineObserver: NSObjectProtocol?
    private var outputListener: AudioObjectPropertyListenerBlock?
    private var defaultOutputAddress = AudioObjectPropertyAddress(mSelector: kAudioHardwarePropertyDefaultOutputDevice,
                                                                  mScope: kAudioObjectPropertyScopeGlobal,
                                                                  mElement: kAudioObjectPropertyElementMain)

    /// `onError` is called once, from an audio queue, when a stream breaks and cannot be restarted.
    init(emit: @escaping (AudioChunk) -> Void, onError: @escaping (String) -> Void = { _ in }) {
        chunker = AudioChunker(rate: 16_000, emit: emit)
        self.onError = onError
    }

    func start() async throws {
        try startMic()
        try startSystemAudio()
        running = true
        watchDevices()
    }

    func stop() {
        tapQueue.sync { running = false }                             // no restart may begin after this
        if let engineObserver { NotificationCenter.default.removeObserver(engineObserver) }
        if let outputListener {
            AudioObjectRemovePropertyListenerBlock(AudioObjectID(kAudioObjectSystemObject), &defaultOutputAddress, tapQueue, outputListener)
        }
        engineObserver = nil; outputListener = nil
        engine.inputNode.removeTap(onBus: 0)
        engine.stop()
        stopSystemAudio()
        chunker.finish()                                              // emit the tail
    }

    // MARK: Device changes

    /// The engine stops itself when the input device or its format changes, and the tap's
    /// aggregate device follows the default output; restart whichever stream was affected.
    private func watchDevices() {
        engineObserver = NotificationCenter.default.addObserver(forName: .AVAudioEngineConfigurationChange, object: engine, queue: nil) { [weak self] _ in
            self?.tapQueue.async { self?.restart("The microphone changed and could not be reopened.") { cap in
                cap.engine.inputNode.removeTap(onBus: 0)
                cap.engine.stop()
                try cap.startMic()
            } }
        }
        let listener: AudioObjectPropertyListenerBlock = { [weak self] _, _ in
            self?.restart("The audio output changed and system audio could not be reopened.") { cap in
                cap.stopSystemAudio()
                try cap.startSystemAudio()
            }
        }
        outputListener = listener
        AudioObjectAddPropertyListenerBlock(AudioObjectID(kAudioObjectSystemObject), &defaultOutputAddress, tapQueue, listener)
    }

    /// Runs on `tapQueue`. A failed restart ends the capture with one error.
    private func restart(_ failure: String, _ work: (AudioCapture) throws -> Void) {
        guard running else { return }
        do {
            try work(self)
        } catch {
            running = false
            onError("\(failure) (\(error.localizedDescription))")
        }
    }

    // MARK: Microphone
    private func startMic() throws {
        let input = engine.inputNode
        let inFmt = input.outputFormat(forBus: 0)
        guard let conv = AVAudioConverter(from: inFmt, to: outFmt) else { throw NSError(domain: "audio", code: 1) }
        input.installTap(onBus: 0, bufferSize: 4096, format: inFmt) { [weak self] buf, _ in
            self?.append(buf, through: conv, speaker: "You")
        }
        try engine.start()
    }

    /// Resamples to 16 kHz mono and hands the samples to the chunker.
    private func append(_ buf: AVAudioPCMBuffer, through conv: AVAudioConverter, speaker: String) {
        let cap = AVAudioFrameCount(Double(buf.frameLength) * rate / buf.format.sampleRate) + 16
        guard let out = AVAudioPCMBuffer(pcmFormat: outFmt, frameCapacity: cap) else { return }
        var fed = false
        conv.convert(to: out, error: nil) { _, status in
            if fed { status.pointee = .noDataNow; return nil }
            fed = true; status.pointee = .haveData; return buf
        }
        if let p = out.floatChannelData?[0], out.frameLength > 0 {
            chunker.append(speaker, Array(UnsafeBufferPointer(start: p, count: Int(out.frameLength))))
        }
    }

    // MARK: System audio (Core Audio process tap, needs System Audio Recording permission)
    private func startSystemAudio() throws {
        let description = CATapDescription(stereoGlobalTapButExcludeProcesses: [])
        description.uuid = UUID()
        description.name = "Muesli system audio"
        description.isPrivate = true
        description.muteBehavior = .unmuted

        var status = AudioHardwareCreateProcessTap(description, &tapID)
        guard status == noErr else { throw Failure(step: "tap", status: status) }

        do {
            var format = try tapFormat()
            guard let inFmt = AVAudioFormat(streamDescription: &format),
                  let conv = AVAudioConverter(from: inFmt, to: outFmt) else { throw Failure(step: "format", status: -1) }

            var aggregate: [String: Any] = [
                kAudioAggregateDeviceNameKey: "Muesli system audio",
                kAudioAggregateDeviceUIDKey: UUID().uuidString,
                kAudioAggregateDeviceIsPrivateKey: true,
                kAudioAggregateDeviceIsStackedKey: false,
                kAudioAggregateDeviceTapAutoStartKey: true,
                kAudioAggregateDeviceTapListKey: [[
                    kAudioSubTapUIDKey: description.uuid.uuidString,
                    kAudioSubTapDriftCompensationKey: true,
                ]],
            ]
            if let output = try? defaultOutputUID() {                  // gives the aggregate a clock
                aggregate[kAudioAggregateDeviceMainSubDeviceKey] = output
                aggregate[kAudioAggregateDeviceSubDeviceListKey] = [[kAudioSubDeviceUIDKey: output]]
            }
            status = AudioHardwareCreateAggregateDevice(aggregate as CFDictionary, &aggregateID)
            guard status == noErr else { throw Failure(step: "device", status: status) }

            status = AudioDeviceCreateIOProcIDWithBlock(&ioProc, aggregateID, tapQueue) { [weak self] _, input, _, _, _ in
                guard let self,
                      let buf = AVAudioPCMBuffer(pcmFormat: inFmt, bufferListNoCopy: input, deallocator: nil),
                      buf.frameLength > 0 else { return }
                self.append(buf, through: conv, speaker: "Others")
            }
            guard status == noErr, let ioProc else { throw Failure(step: "callback", status: status) }

            status = AudioDeviceStart(aggregateID, ioProc)
            guard status == noErr else { throw Failure(step: "start", status: status) }
        } catch {
            stopSystemAudio()
            throw error
        }
    }

    private func stopSystemAudio() {
        if aggregateID != kAudioObjectUnknown {
            if let ioProc {
                AudioDeviceStop(aggregateID, ioProc)
                AudioDeviceDestroyIOProcID(aggregateID, ioProc)
            }
            AudioHardwareDestroyAggregateDevice(aggregateID)
        }
        if tapID != kAudioObjectUnknown { AudioHardwareDestroyProcessTap(tapID) }
        ioProc = nil
        aggregateID = AudioObjectID(kAudioObjectUnknown)
        tapID = AudioObjectID(kAudioObjectUnknown)
    }

    private func tapFormat() throws -> AudioStreamBasicDescription {
        var address = AudioObjectPropertyAddress(mSelector: kAudioTapPropertyFormat,
                                                 mScope: kAudioObjectPropertyScopeGlobal,
                                                 mElement: kAudioObjectPropertyElementMain)
        var format = AudioStreamBasicDescription()
        var size = UInt32(MemoryLayout<AudioStreamBasicDescription>.size)
        let status = AudioObjectGetPropertyData(tapID, &address, 0, nil, &size, &format)
        guard status == noErr else { throw Failure(step: "format", status: status) }
        return format
    }

    private func defaultOutputUID() throws -> String {
        var address = AudioObjectPropertyAddress(mSelector: kAudioHardwarePropertyDefaultSystemOutputDevice,
                                                 mScope: kAudioObjectPropertyScopeGlobal,
                                                 mElement: kAudioObjectPropertyElementMain)
        var device = AudioObjectID(kAudioObjectUnknown)
        var size = UInt32(MemoryLayout<AudioObjectID>.size)
        var status = AudioObjectGetPropertyData(AudioObjectID(kAudioObjectSystemObject), &address, 0, nil, &size, &device)
        guard status == noErr else { throw Failure(step: "output device", status: status) }

        address.mSelector = kAudioDevicePropertyDeviceUID
        var uid: Unmanaged<CFString>?
        size = UInt32(MemoryLayout<Unmanaged<CFString>?>.size)
        status = AudioObjectGetPropertyData(device, &address, 0, nil, &size, &uid)
        guard status == noErr, let uid else { throw Failure(step: "output device", status: status) }
        return uid.takeRetainedValue() as String
    }
}
