import XCTest
@testable import MuesliCore

final class MeetingEditorTests: IsolatedTestCase {
    private var discarded: [String] = []
    private var savedDiscard: ((URL) -> Void)!

    override func setUp() {
        super.setUp()
        discarded = []
        savedDiscard = MeetingEditor.discard
        MeetingEditor.discard = { [unowned self] url in
            discarded.append(url.lastPathComponent)
            try? FileManager.default.removeItem(at: url)
        }
    }

    override func tearDown() {
        MeetingEditor.discard = savedDiscard
        super.tearDown()
    }

    private func ready(_ id: String, title: String = "Weekly sync") -> Meeting {
        let m = meeting(id, state: .ready, title: title)
        MeetingStore.save(m)
        Store.save(m)
        return m
    }

    private func exported() -> [String] {
        ((try? FileManager.default.contentsOfDirectory(atPath: transcriptsDir.path)) ?? []).sorted()
    }

    func testRenameUpdatesStoreAndRenamesTheFiles() throws {
        let m = ready("a")
        let before = exported()
        XCTAssertEqual(before.count, 2)

        let renamed = try MeetingEditor.rename("a", to: "  Planning\nQ3 ", folder: transcriptsDir).get()
        XCTAssertEqual(renamed.title, "Planning Q3")
        XCTAssertEqual(MeetingStore.load("a")?.title, "Planning Q3")
        let after = exported()
        XCTAssertEqual(after.count, 2)
        XCTAssertTrue(after.allSatisfy { $0.contains("Planning-Q3") })
        let md = try String(contentsOf: transcriptsDir.appendingPathComponent(Store.fileStem(renamed) + ".md"), encoding: .utf8)
        XCTAssertTrue(md.hasPrefix("# Planning Q3"))
        XCTAssertNotEqual(Store.fileStem(m), Store.fileStem(renamed))
    }

    func testRenameRejectsEmptyAndTooLongTitles() {
        _ = ready("a")
        XCTAssertEqual(MeetingEditor.rename("a", to: "  ", folder: transcriptsDir).failure, .invalid("The title cannot be empty."))
        XCTAssertNotNil(MeetingEditor.rename("a", to: String(repeating: "x", count: 201), folder: transcriptsDir).failure)
        XCTAssertEqual(MeetingStore.load("a")?.title, "Weekly sync")
    }

    func testRenameNeverOverwritesAnotherMeetingsFiles() {
        _ = ready("a", title: "One")
        _ = ready("b", title: "Two")
        let result = MeetingEditor.rename("a", to: "Two", folder: transcriptsDir)
        XCTAssertEqual(result.failure, .invalid("Another transcript that started in the same minute already has that title."))
        XCTAssertEqual(MeetingStore.load("b")?.title, "Two")
        XCTAssertEqual(exported().count, 4)
    }

    func testMeetingsInProgressCannotBeChanged() {
        MeetingStore.save(meeting("live", state: .recording))
        XCTAssertEqual(MeetingEditor.rename("live", to: "x", folder: transcriptsDir).failure, .busy(.recording))
        XCTAssertEqual(MeetingEditor.delete("live", folder: transcriptsDir).failure, .busy(.recording))
        XCTAssertEqual(MeetingEditor.rename("missing", to: "x", folder: transcriptsDir).failure, .notFound)
    }

    func testSetAttendeesCleansTheList() throws {
        _ = ready("a")
        let m = try MeetingEditor.setAttendees("a", to: [" Ann ", "ann", "", "Bob"], folder: transcriptsDir).get()
        XCTAssertEqual(m.attendees, ["Ann", "Bob"])
        XCTAssertEqual(try MeetingEditor.setAttendees("a", to: [], folder: transcriptsDir).get().attendees, [])
    }

    func testRenameSpeakerChangesOnlyThatLabel() throws {
        _ = ready("a")
        let m = try MeetingEditor.renameSpeaker("a", from: "Others", to: "Ann", folder: transcriptsDir).get()
        XCTAssertEqual(Set(m.transcript.map(\.speaker)), ["You", "Ann"])
        let md = try String(contentsOf: transcriptsDir.appendingPathComponent(Store.fileStem(m) + ".md"), encoding: .utf8)
        XCTAssertTrue(md.contains("**Ann**"))
        XCTAssertNotNil(MeetingEditor.renameSpeaker("a", from: "Nobody", to: "X", folder: transcriptsDir).failure)
    }

    func testDeleteDiscardsFilesAndIsNotImportedAgain() throws {
        let m = ready("a")
        _ = try MeetingEditor.delete("a", folder: transcriptsDir).get()
        XCTAssertNil(MeetingStore.load("a"))
        XCTAssertEqual(exported(), [])
        XCTAssertEqual(Set(discarded), [Store.fileStem(m) + ".md", Store.fileStem(m) + ".json", "a.json"])
        XCTAssertTrue(MeetingStore.importTranscripts(from: transcriptsDir).isEmpty)
    }

    func testDeleteLeavesForeignFilesWithTheSameNameAlone() throws {
        let m = meeting("a", state: .ready)
        MeetingStore.save(m)
        var other = m
        other.id = "someone-else"
        Store.save(other, in: transcriptsDir)
        _ = try MeetingEditor.delete("a", folder: transcriptsDir).get()
        XCTAssertEqual(exported().count, 2)
    }

    func testToolsReachTheEditor() throws {
        _ = ready("a")
        setenv("TRANSCRIPT_DIR", transcriptsDir.path, 1)
        guard case .success(let renamed) = Tools.call("rename_transcript", ["id": "a", "title": "Retro"]) else { return XCTFail() }
        XCTAssertTrue(renamed.contains("\"title\" : \"Retro\""))
        guard case .success = Tools.call("rename_speaker", ["id": "a", "from": "Others", "to": "Ann"]) else { return XCTFail() }
        guard case .success = Tools.call("set_attendees", ["id": "a", "attendees": ["Ann"]]) else { return XCTFail() }
        guard case .failure = Tools.call("set_attendees", ["id": "a", "attendees": [1, 2]]) else { return XCTFail() }
        guard case .success(let deleted) = Tools.call("delete_transcript", ["id": "a"]) else { return XCTFail() }
        XCTAssertTrue(deleted.contains("\"movedToTrash\" : true"))
        guard case .failure = Tools.call("delete_transcript", ["id": "a"]) else { return XCTFail() }
    }

    func testToolAnnotationsTellClientsWhatChangesData() {
        let byName = Dictionary(uniqueKeysWithValues: Tools.definitions.map { ($0["name"] as! String, $0["annotations"] as! [String: Any]) })
        XCTAssertEqual(Set(byName.keys), Tools.names)
        for name in Tools.readNames { XCTAssertEqual(byName[name]?["readOnlyHint"] as? Bool, true, name) }
        for name in Tools.manageNames { XCTAssertEqual(byName[name]?["readOnlyHint"] as? Bool, false, name) }
        XCTAssertEqual(byName["delete_transcript"]?["destructiveHint"] as? Bool, true)
    }
}

private extension Result {
    var failure: Failure? {
        if case .failure(let e) = self { return e }
        return nil
    }
}
