import Foundation

/// Preferences shared by the app (writes) and muesli-mcp (reads): config.json.
/// The transcripts folder stays in the app's UserDefaults; muesli-mcp does not need it.
public struct Preferences: Codable, Equatable, Sendable {
    public enum MCPMode: String, Codable, Sendable { case local, remote }

    public var mcpEnabled = false
    public var mcpMode: MCPMode = .local
    /// Remote mode: local port cloudflared forwards to.
    public var httpPort = 8747
    /// Remote mode: public hostname of the tunnel, e.g. muesli.example.com (shown in the UI, used by doctor).
    public var publicHostname = ""
    /// Remote mode: Cloudflare Zero Trust team domain, e.g. myteam.cloudflareaccess.com.
    public var teamDomain = ""
    /// Remote mode: Application Audience (AUD) tag of the Access application.
    public var audience = ""
    /// Remote mode: the only identity allowed to call the server.
    public var allowedEmail = ""
    public var autoStop = false
    public var endDelaySeconds = 60
    public var languages: [String]? = nil

    public init() {}

    public init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        let d = Preferences()
        mcpEnabled = try c.decodeIfPresent(Bool.self, forKey: .mcpEnabled) ?? d.mcpEnabled
        mcpMode = try c.decodeIfPresent(MCPMode.self, forKey: .mcpMode) ?? d.mcpMode
        httpPort = try c.decodeIfPresent(Int.self, forKey: .httpPort) ?? d.httpPort
        publicHostname = try c.decodeIfPresent(String.self, forKey: .publicHostname) ?? d.publicHostname
        teamDomain = try c.decodeIfPresent(String.self, forKey: .teamDomain) ?? d.teamDomain
        audience = try c.decodeIfPresent(String.self, forKey: .audience) ?? d.audience
        allowedEmail = try c.decodeIfPresent(String.self, forKey: .allowedEmail) ?? d.allowedEmail
        autoStop = try c.decodeIfPresent(Bool.self, forKey: .autoStop) ?? d.autoStop
        endDelaySeconds = try c.decodeIfPresent(Int.self, forKey: .endDelaySeconds) ?? d.endDelaySeconds
        languages = try c.decodeIfPresent([String].self, forKey: .languages)
    }

    public static func load() -> Preferences {
        guard let data = try? Data(contentsOf: Paths.config),
              let s = try? JSONDecoder().decode(Preferences.self, from: data) else { return Preferences() }
        return s
    }

    public func save() {
        try? JSONCoding.encoder().encode(self).write(to: Paths.config, options: .atomic)
    }

    /// "myteam", "https://myteam.cloudflareaccess.com/" and "myteam.cloudflareaccess.com" all mean the same.
    public var normalizedTeamDomain: String {
        var t = Preferences.bareHost(teamDomain)
        if !t.isEmpty && !t.contains(".") { t += ".cloudflareaccess.com" }
        return t
    }

    public var effectiveLanguages: [String] {
        let chosen = Languages.unique((languages ?? []).map(Languages.baseCode).filter(Languages.isSupported))
        return chosen.isEmpty ? Languages.defaults() : chosen
    }

    public var normalizedHostname: String { Preferences.bareHost(publicHostname) }

    /// What is still missing before remote mode can start; nil when complete.
    public var remoteProblem: String? {
        if normalizedTeamDomain.isEmpty { return "Team domain is missing" }
        if audience.trimmingCharacters(in: .whitespaces).isEmpty { return "Audience (AUD) tag is missing" }
        if allowedEmail.trimmingCharacters(in: .whitespaces).isEmpty { return "Allowed email is missing" }
        if !(1024...65535).contains(httpPort) { return "Port must be between 1024 and 65535" }
        return nil
    }

    static func bareHost(_ s: String) -> String {
        var t = s.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        for p in ["https://", "http://"] where t.hasPrefix(p) { t.removeFirst(p.count) }
        if let slash = t.firstIndex(of: "/") { t = String(t[..<slash]) }
        return t
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
