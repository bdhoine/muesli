import XCTest
@testable import MuesliCore

/// Writes the files the desktop app's golden tests compare against (`fixtures/core`).
/// Does nothing in a normal test run: `scripts/regenerate-fixtures.sh` sets FIXTURE_OUT.
final class FixtureGenerator: IsolatedTestCase {
    func testWriteCoreFixtures() throws {
        guard let out = ProcessInfo.processInfo.environment["FIXTURE_OUT"] else { return }
        let dir = URL(fileURLWithPath: out)
        try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)

        var m = meeting("golden-1", state: .ready, segments: 3, title: "Weekly sync")
        m.partial = true
        try JSONCoding.encoder().encode(m).write(to: dir.appendingPathComponent("meeting.json"))
        try Store.markdown(m).write(to: dir.appendingPathComponent("meeting.md"), atomically: true, encoding: .utf8)
        try Data(Store.fileStem(m).utf8).write(to: dir.appendingPathComponent("stem.txt"))

        var p = Preferences()
        p.mcpEnabled = true
        p.languages = ["nl", "en"]
        p.model = .medium
        p.customModelPath = "/models/x.bin"
        p.onboardingDone = true
        try JSONCoding.encoder().encode(p).write(to: dir.appendingPathComponent("config.json"))

        let t0 = Date(timeIntervalSince1970: 1_800_000_000)
        var s = AppStatus(appState: "recording", lastReady: .init(id: "m", title: "t", endedAt: t0))
        s.updatedAt = t0.addingTimeInterval(100)
        s.pid = 4242
        s.current = .init(id: "golden-1", title: "Weekly sync", source: "calendar", startedAt: t0, segmentCount: 3)
        try JSONCoding.encoder().encode(s).write(to: dir.appendingPathComponent("state.json"))
    }
}
