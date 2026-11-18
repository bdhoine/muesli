import XCTest
@testable import MuesliCore

final class DetectionTests: XCTestCase {
    private var logic = DetectionLogic()
    private var t = Date(timeIntervalSince1970: 1_800_000_000)
    private let sync = (meeting: DetectedMeeting(title: "Weekly sync", source: "calendar", attendees: ["Ann", "Bob"]), eventID: "ev1")

    override func setUp() {
        logic = DetectionLogic()
        t = Date(timeIntervalSince1970: 1_800_000_000)
    }

    /// Advances the clock, then ticks.
    private func tick(after seconds: TimeInterval = 5, recording: Bool = false,
                      calendar: (meeting: DetectedMeeting, eventID: String)? = nil, mic: Bool? = false, teams: Bool = false) -> [DetectionLogic.Event] {
        t += seconds
        return logic.tick(now: t, recording: recording, calendar: calendar, micBusy: mic, teamsRunning: teams)
    }

    private func suggestions(_ events: [DetectionLogic.Event]) -> [DetectedMeeting] {
        events.compactMap { if case .suggest(let m) = $0 { return m } else { return nil } }
    }

    // MARK: original behaviour: suggesting

    func testQuietSystemSuggestsNothing() {
        for _ in 0..<10 { XCTAssertEqual(tick(), []) }
        XCTAssertNil(logic.inProgress)
    }

    func testMicMustBeBusyForMoreThanTenSeconds() {
        XCTAssertEqual(tick(mic: true), [])                        // 0 s
        XCTAssertEqual(tick(mic: true), [])                        // 5 s
        XCTAssertEqual(tick(mic: true), [])                        // 10 s: not yet "more than"
        let events = tick(mic: true)                               // 15 s
        XCTAssertEqual(suggestions(events), [DetectedMeeting(title: "Call", source: "audio")])
    }

    func testShortMicUseIsIgnoredAndTheClockRestarts() {
        _ = tick(mic: true); _ = tick(mic: true)
        XCTAssertEqual(tick(mic: false), [])                       // e.g. dictation ended
        _ = tick(mic: true); _ = tick(mic: true)
        XCTAssertEqual(tick(mic: true), [])                        // only 10 s since it restarted
        XCTAssertEqual(suggestions(tick(mic: true)).count, 1)
    }

    func testTeamsRunningChangesTheLabel() {
        for _ in 0..<3 { _ = tick(mic: true, teams: true) }
        XCTAssertEqual(suggestions(tick(mic: true, teams: true)), [DetectedMeeting(title: "Teams call", source: "teams")])
    }

    func testACallIsSuggestedOnceHoweverLongItLasts() {
        for _ in 0..<3 { _ = tick(mic: true) }
        var total = suggestions(tick(mic: true)).count
        for _ in 0..<240 { total += suggestions(tick(mic: true)).count }      // 20 minutes, past the cooldown
        XCTAssertEqual(total, 1)
    }

    func testAMuteDoesNotMakeItANewCall() {
        for _ in 0..<4 { _ = tick(mic: true) }
        for _ in 0..<5 { _ = tick(mic: false) }                    // 25 s, shorter than the end delay
        var again = 0
        for _ in 0..<300 { again += suggestions(tick(mic: true)).count }
        XCTAssertEqual(again, 0)
    }

    func testANewCallAfterTheMicWasFreeIsSuggestedAgain() {
        for _ in 0..<4 { _ = tick(mic: true) }
        for _ in 0..<13 { _ = tick(mic: false) }                   // longer than the 60 s end delay
        var again = 0
        for _ in 0..<140 { again += suggestions(tick(mic: true)).count }      // and past the 600 s cooldown
        XCTAssertEqual(again, 1)
    }

    func testCalendarMeetingIsSuggestedOncePerEvent() {
        XCTAssertEqual(suggestions(tick(calendar: sync)), [sync.meeting])
        for _ in 0..<20 { XCTAssertEqual(suggestions(tick(calendar: sync)), []) }
        let next = (meeting: DetectedMeeting(title: "1:1", source: "calendar"), eventID: "ev2")
        XCTAssertEqual(suggestions(tick(calendar: next)), [next.meeting])
    }

    func testCalendarWinsOverAudioAndSuppressesTheAudioPrompt() {
        _ = tick(calendar: sync, mic: true)
        var extra = 0
        for _ in 0..<10 { extra += suggestions(tick(calendar: sync, mic: true)).count }
        XCTAssertEqual(extra, 0)
        XCTAssertEqual(logic.inProgress?.meeting.source, "calendar")
    }

    func testNothingIsSuggestedWhileRecording() {
        for _ in 0..<10 { XCTAssertEqual(suggestions(tick(recording: true, calendar: sync, mic: true)), []) }
    }

    func testLooksLikeMeeting() {
        XCTAssertTrue(DetectionLogic.looksLikeMeeting(text: "Join: https://Teams.Microsoft.com/l/meetup-join/abc", attendeeCount: 0))
        XCTAssertTrue(DetectionLogic.looksLikeMeeting(text: "https://meet.google.com/abc-defg-hij", attendeeCount: 1))
        XCTAssertTrue(DetectionLogic.looksLikeMeeting(text: "us02web.zoom.us/j/123", attendeeCount: 0))
        XCTAssertTrue(DetectionLogic.looksLikeMeeting(text: "Room 4.12", attendeeCount: 3))
        XCTAssertFalse(DetectionLogic.looksLikeMeeting(text: "Dentist", attendeeCount: 0))
        XCTAssertFalse(DetectionLogic.looksLikeMeeting(text: "Focus time", attendeeCount: 1))      // only yourself
    }

    // MARK: new: knowing a meeting is in progress

    func testInProgressTracksACallFromStartToEnd() {
        for _ in 0..<3 { _ = tick(mic: true) }
        XCTAssertNil(logic.inProgress)
        XCTAssertTrue(tick(mic: true).contains(.inProgressChanged))
        let since = logic.inProgress?.since
        XCTAssertEqual(logic.inProgress?.meeting.source, "audio")

        XCTAssertFalse(tick(mic: true).contains(.inProgressChanged))          // unchanged: no event, same start
        XCTAssertEqual(logic.inProgress?.since, since)

        XCTAssertEqual(tick(mic: false), [.inProgressChanged])
        XCTAssertNil(logic.inProgress)
    }

    func testInProgressIsReportedWhileRecording() {
        XCTAssertTrue(tick(recording: true, calendar: sync).contains(.inProgressChanged))
        XCTAssertEqual(logic.inProgress?.meeting.title, "Weekly sync")
        XCTAssertEqual(tick(recording: true, calendar: nil), [.inProgressChanged])
        XCTAssertNil(logic.inProgress)
    }

    func testCalendarEventEndingFallsBackToTheCallStillRunning() {
        for _ in 0..<4 { _ = tick(calendar: sync, mic: true) }
        let events = tick(calendar: nil, mic: true)
        XCTAssertTrue(events.contains(.inProgressChanged))
        XCTAssertEqual(logic.inProgress?.meeting.source, "audio")
    }

    func testUnknownMicStateKeepsTheLastAnswer() {
        for _ in 0..<4 { _ = tick(mic: true) }
        XCTAssertNotNil(logic.inProgress)
        for _ in 0..<5 { XCTAssertEqual(tick(recording: true, mic: nil), []) }   // older macOS: cannot tell while recording
        XCTAssertNotNil(logic.inProgress)
    }

    // MARK: new: end of call

    func testCallEndIsReportedOnceAfterTheDelay() {
        logic.endDelay = 60
        for _ in 0..<4 { _ = tick(recording: true, mic: true) }
        for _ in 0..<12 {                                          // quiet for 0 s ... 55 s
            XCTAssertFalse(tick(recording: true, mic: false).contains(.callEnded))
        }
        XCTAssertTrue(tick(recording: true, mic: false).contains(.callEnded))
        for _ in 0..<20 { XCTAssertFalse(tick(recording: true, mic: false).contains(.callEnded)) }
    }

    func testBriefSilenceDoesNotEndTheCall() {
        logic.endDelay = 60
        _ = tick(recording: true, mic: true)
        for _ in 0..<10 {
            for _ in 0..<8 { XCTAssertFalse(tick(recording: true, mic: false).contains(.callEnded)) }   // 40 s quiet
            _ = tick(recording: true, mic: true)                                                    // rejoined
        }
    }

    func testCallEndCanFireAgainAfterRejoining() {
        logic.endDelay = 10
        _ = tick(recording: true, mic: true)
        _ = tick(recording: true, mic: false); _ = tick(recording: true, mic: false)
        XCTAssertTrue(tick(recording: true, mic: false).contains(.callEnded))
        _ = tick(recording: true, mic: true)
        _ = tick(recording: true, mic: false); _ = tick(recording: true, mic: false)
        XCTAssertTrue(tick(recording: true, mic: false).contains(.callEnded))
    }

    func testManualRecordingWithoutACallNeverEnds() {
        logic.endDelay = 10
        for _ in 0..<50 { XCTAssertFalse(tick(recording: true, mic: false).contains(.callEnded)) }
    }

    func testEndTrackingResetsBetweenRecordings() {
        logic.endDelay = 10
        _ = tick(recording: true, mic: true)
        _ = tick(recording: false, mic: false)                     // stopped by hand
        for _ in 0..<10 { XCTAssertFalse(tick(recording: true, mic: false).contains(.callEnded)) }   // new, in-person recording
    }

    func testEndDelayIsConfigurable() {
        logic.endDelay = 20
        _ = tick(recording: true, mic: true)
        var ticks = 0
        while !tick(recording: true, mic: false).contains(.callEnded) { ticks += 1; XCTAssertLessThan(ticks, 100) }
        XCTAssertEqual(ticks, 4)                                   // quiet at 5 s; 20 s later is the 5th quiet tick
    }
}
