import Foundation
@_exported import MuesliFFI

// Meeting, TranscriptSegment, MeetingState, Preferences, AppStatus and PathSet are defined once,
// in the Rust core (windows/crates/core), and generated into MuesliFFI. This file adds the
// Swift conveniences the app uses.

public enum MuesliInfo {
    public static var version: String { muesliVersion() }
    public static let repository = URL(string: "https://github.com/bdhoine/muesli")!
}

extension MeetingState: Sendable {
    /// The word used in files and messages.
    public var rawValue: String {
        switch self {
        case .detected: "detected"
        case .recording: "recording"
        case .finalizing: "finalizing"
        case .ready: "ready"
        case .failed: "failed"
        }
    }
}

extension Meeting: Identifiable {
    /// A meeting that starts now, with a new id (the usual way to begin a recording).
    public init(title: String, source: String, attendees: [String] = [], state: MeetingState = .recording) {
        self = meetingNew(title: title, source: source)
        self.attendees = attendees
        self.state = state
    }
}

extension DetectedMeeting {
    public init(title: String, source: String) { self.init(title: title, source: source, attendees: []) }
}

/// Where Muesli keeps its data. The rules (environment overrides, the folder chosen in
/// config.json) live in the Rust core; each access asks for the paths in effect now.
public enum Paths {
    public static var current: PathSet { pathsCurrent() }

    /// App data: Whisper models, state.json, config.json, meetings/. MUESLI_SUPPORT_DIR overrides (tests).
    public static var support: URL { URL(fileURLWithPath: current.support, isDirectory: true) }
    public static var state: URL { URL(fileURLWithPath: pathsStateFile(paths: current)) }
    public static var config: URL { URL(fileURLWithPath: pathsConfigFile(paths: current)) }
    public static var auditLog: URL { URL(fileURLWithPath: pathsAuditLog(paths: current)) }
    public static var meetings: URL { URL(fileURLWithPath: pathsMeetingsDir(paths: current), isDirectory: true) }

    /// Where transcripts are written. TRANSCRIPT_DIR overrides, then the folder chosen in Settings.
    public static var transcripts: URL { URL(fileURLWithPath: current.transcripts, isDirectory: true) }
    public static var exportFolder: URL { URL(fileURLWithPath: pathsExportFolder(paths: current), isDirectory: true) }

    public static var transcriptsOverriddenByEnvironment: Bool { pathsTranscriptsSetByEnvironment() }
    public static var transcriptsIsCustom: Bool { Preferences.load().transcriptsFolder != nil }

    public static func setTranscripts(_ url: URL?) { pathsSetTranscriptsFolder(folder: url?.path) }

    static let legacyTranscriptDirKey = "transcriptDir"

    /// Before 0.0.2 the macOS app kept the transcripts folder in its own defaults. Moves it to
    /// config.json once, so both apps use the same file.
    public static func migrateLegacyTranscriptsFolder(defaults: UserDefaults = .standard) {
        guard let saved = defaults.string(forKey: legacyTranscriptDirKey) else { return }
        if Preferences.load().transcriptsFolder == nil { pathsSetTranscriptsFolder(folder: saved) }
        defaults.removeObject(forKey: legacyTranscriptDirKey)
    }
}
