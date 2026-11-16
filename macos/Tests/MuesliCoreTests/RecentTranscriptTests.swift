import XCTest
@testable import MuesliCore

final class RecentTranscriptTests: IsolatedTestCase {
    let utc: Calendar = {
        var c = Calendar(identifier: .gregorian)
        c.timeZone = TimeZone(identifier: "UTC")!
        return c
    }()

    func testRecentTranscriptsUseTitleAndTimesFromTheJSON() {
        let t0 = Date(timeIntervalSince1970: 1_800_000_000)
        Store.save(meeting("a", state: .ready, title: "Weekly sync", started: t0))
        let recent = Store.recentTranscripts(limit: 5)
        XCTAssertEqual(recent.map(\.title), ["Weekly sync"])
        XCTAssertEqual(recent.first?.startedAt, t0)
        XCTAssertEqual(recent.first?.durationMinutes, 30)
        XCTAssertEqual(recent.first?.url.pathExtension, "md")
    }

    func testMarkdownWithoutJSONFallsBackToTheFileName() throws {
        try FileManager.default.createDirectory(at: transcriptsDir, withIntermediateDirectories: true)
        try Data("# Notes".utf8).write(to: transcriptsDir.appendingPathComponent("handwritten.md"))
        let recent = Store.recentTranscripts(limit: 5)
        XCTAssertEqual(recent.map(\.title), ["handwritten"])
        XCTAssertNil(recent.first?.durationMinutes)
    }

    func testLimitZeroHidesTheList() {
        Store.save(meeting("a", state: .ready))
        XCTAssertTrue(Store.recentTranscripts(limit: 0).isEmpty)
        XCTAssertEqual(Store.recentTranscripts(limit: 1).count, 1)
    }

    func testSubtitleSaysTodayYesterdayOrTheDate() {
        let now = Date(timeIntervalSince1970: 1_800_000_000)
        let en = Locale(identifier: "en_US")
        func subtitle(_ started: Date, minutes: Double?) -> String {
            RecentTranscript(url: URL(fileURLWithPath: "/x.md"), title: "t", startedAt: started,
                             endedAt: minutes.map { started.addingTimeInterval($0 * 60) })
                .subtitle(now: now, calendar: utc, locale: en)
        }
        XCTAssertTrue(subtitle(now.addingTimeInterval(-3600), minutes: 12).hasPrefix("Today "))
        XCTAssertTrue(subtitle(now.addingTimeInterval(-3600), minutes: 12).hasSuffix(" · 12 min"))
        XCTAssertTrue(subtitle(now.addingTimeInterval(-86_400), minutes: 0.2).hasPrefix("Yesterday "))
        XCTAssertTrue(subtitle(now.addingTimeInterval(-86_400), minutes: 0.2).hasSuffix(" · <1 min"))
        XCTAssertFalse(subtitle(now.addingTimeInterval(-10 * 86_400), minutes: nil).contains("·"))
    }

    func testRecentLimitDefaultsToFiveAndIsCapped() throws {
        XCTAssertEqual(Preferences().recentLimit, 5)
        let old = try preferences(fromJSON: "{}")
        XCTAssertEqual(old.recentLimit, 5)
        var p = Preferences()
        p.recentLimit = 50
        XCTAssertEqual(p.effectiveRecentLimit, Preferences.maxRecentLimit)
        p.recentLimit = -1
        XCTAssertEqual(p.effectiveRecentLimit, 0)
        XCTAssertEqual(Preferences.recentLimitOptions.max(), Preferences.maxRecentLimit)
    }
}
