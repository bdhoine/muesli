import Foundation

// Preferences and AppStatus are generated from the Rust core (see Models.swift); this file adds
// the Swift conveniences.

extension Preferences {
    public init() { self = preferencesDefault() }

    public static var recentLimitOptions: [Int32] { preferencesRecentLimitOptions() }
    public static var maxRecentLimit: Int32 { preferencesMaxRecentLimit() }
    public static var endDelayOptions: [Int32] { preferencesEndDelayOptions() }

    public var effectiveRecentLimit: Int32 { preferencesEffectiveRecentLimit(prefs: self) }

    public static func load() -> Preferences { preferencesLoad(paths: Paths.current) }

    public func save() { preferencesSave(prefs: self, paths: Paths.current) }

    /// Whether the languages follow the system (`languages == nil`).
    public var usesSystemLanguages: Bool { languages == nil }

    /// Switches between the system's languages and the user's own list without losing that list.
    public mutating func setUsesSystemLanguages(_ on: Bool, systemDefaults: [String] = Languages.defaults()) {
        self = preferencesSetUsesSystemLanguages(prefs: self, on: on, systemDefaults: systemDefaults)
    }

    public var effectiveLanguages: [String] {
        preferencesEffectiveLanguages(prefs: self, systemDefaults: Languages.defaults())
    }
}

extension AppStatus {
    public typealias Detected = StatusDetected
    public typealias Current = StatusCurrent
    public typealias LastReady = StatusLastReady
    public typealias FileJob = StatusFileJob

    public static var freshSeconds: TimeInterval { TimeInterval(appStatusFreshSeconds()) }

    public init(appState: String, detected: Detected? = nil, current: Current? = nil, lastReady: LastReady? = nil, fileJob: FileJob? = nil, error: String? = nil) {
        self = appStatusNew(appState: appState)
        self.detected = detected; self.current = current; self.lastReady = lastReady; self.fileJob = fileJob; self.error = error
    }

    public func write() { appStatusWrite(status: self, paths: Paths.current) }

    public static func read() -> AppStatus? { appStatusRead(paths: Paths.current) }

    public func isFresh(now: Date = Date()) -> Bool { appStatusIsFresh(status: self, now: now) }
}
