import Foundation

/// Preferences shared by the app (writes) and muesli-mcp (reads): config.json.
/// The transcripts folder stays in the app's UserDefaults; muesli-mcp does not need it.
public struct Preferences: Codable, Equatable, Sendable {
    public var mcpEnabled = false
    public var claudeDesktopConfigEntry = false
    public var autoStop = false
    public var endDelaySeconds = 60
    public var languages: [String]? = nil
    public var recentLimit = 5

    public static let recentLimitOptions = [0, 3, 5, 10]
    public static let maxRecentLimit = 10
    public static let endDelayOptions = [30, 60, 120, 300]

    public var effectiveRecentLimit: Int { min(max(recentLimit, 0), Preferences.maxRecentLimit) }

    public init() {}

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let d = Preferences()
        mcpEnabled = try c.decodeIfPresent(Bool.self, forKey: .mcpEnabled) ?? d.mcpEnabled
        claudeDesktopConfigEntry = try c.decodeIfPresent(Bool.self, forKey: .claudeDesktopConfigEntry) ?? d.claudeDesktopConfigEntry
        autoStop = try c.decodeIfPresent(Bool.self, forKey: .autoStop) ?? d.autoStop
        endDelaySeconds = try c.decodeIfPresent(Int.self, forKey: .endDelaySeconds) ?? d.endDelaySeconds
        languages = try c.decodeIfPresent([String].self, forKey: .languages)
        recentLimit = try c.decodeIfPresent(Int.self, forKey: .recentLimit) ?? d.recentLimit
    }

    public static func load() -> Preferences {
        guard let data = try? Data(contentsOf: Paths.config),
              let s = try? JSONDecoder().decode(Preferences.self, from: data) else { return Preferences() }
        return s
    }

    public func save() {
        try? JSONCoding.encoder().encode(self).write(to: Paths.config, options: .atomic)
    }

    public var effectiveLanguages: [String] {
        let chosen = Languages.unique((languages ?? []).map(Languages.baseCode).filter(Languages.isSupported))
        return chosen.isEmpty ? Languages.defaults() : chosen
    }
}

/// state.json: what the app is doing right now. Rewritten on every change and every 10 s.
public struct AppStatus: Codable, Sendable {
    public struct Detected: Codable, Sendable, Equatable {
        public var title: String
        public var source: String
        public var since: Date
        public init(title: String, source: String, since: Date) { self.title = title; self.source = source; self.since = since }
    }
    public struct Current: Codable, Sendable {
        public var id: String
        public var title: String
        public var source: String
        public var startedAt: Date
        public var segmentCount: Int
        public init(id: String, title: String, source: String, startedAt: Date, segmentCount: Int) {
            self.id = id; self.title = title; self.source = source; self.startedAt = startedAt; self.segmentCount = segmentCount
        }
    }
    public struct LastReady: Codable, Sendable {
        public var id: String
        public var title: String
        public var endedAt: Date?
        public init(id: String, title: String, endedAt: Date?) { self.id = id; self.title = title; self.endedAt = endedAt }
    }

    public static let freshSeconds: TimeInterval = 30

    public var schemaVersion = 1
    public var updatedAt = Date()
    public var pid: Int32 = ProcessInfo.processInfo.processIdentifier
    /// idle | recording | finalizing | error | permissionMissing
    public var appState: String
    public var detected: Detected?
    public var current: Current?
    public var lastReady: LastReady?
    public var error: String?
    public var transcriptsFolder: String?

    public init(appState: String, detected: Detected? = nil, current: Current? = nil, lastReady: LastReady? = nil, error: String? = nil) {
        self.appState = appState; self.detected = detected; self.current = current; self.lastReady = lastReady; self.error = error
    }

    public func write() {
        try? JSONCoding.encoder().encode(self).write(to: Paths.state, options: .atomic)
    }

    public static func read() -> AppStatus? {
        guard let data = try? Data(contentsOf: Paths.state) else { return nil }
        return try? JSONCoding.decoder().decode(AppStatus.self, from: data)
    }

    public func isFresh(now: Date = Date()) -> Bool {
        now.timeIntervalSince(updatedAt) < AppStatus.freshSeconds
    }
}
