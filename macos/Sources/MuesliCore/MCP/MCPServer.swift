import Foundation

/// Minimal Model Context Protocol server: JSON-RPC 2.0 in, JSON-RPC 2.0 out.
/// Transport-agnostic; stdio and HTTP both feed `handle`.
struct MCPServer {
    static let protocolVersions = ["2025-06-18", "2025-03-26", "2024-11-05"]

    /// Returns a message when tool calls must be refused (integration switched off).
    var refusal: () -> String? = { nil }
    /// Called for every tool call: tool name, meeting id if any.
    var audit: (String, String?) -> Void = { _, _ in }

    /// One request body or line. Returns nil when nothing has to be sent back (notifications).
    func handle(_ data: Data) -> Data? {
        guard let json = try? JSONSerialization.jsonObject(with: data) else {
            return encode(error(id: NSNull(), code: -32700, message: "Parse error"))
        }
        if let batch = json as? [Any] {
            let replies = batch.compactMap { ($0 as? [String: Any]).flatMap(handleOne) }
            return replies.isEmpty ? nil : encode(replies)
        }
        guard let message = json as? [String: Any] else {
            return encode(error(id: NSNull(), code: -32600, message: "Invalid request"))
        }
        return handleOne(message).map(encode)
    }

    private func handleOne(_ msg: [String: Any]) -> [String: Any]? {
        guard let id = msg["id"], !(id is NSNull) else { return nil }          // notification
        guard let method = msg["method"] as? String else {
            return msg["result"] != nil || msg["error"] != nil ? nil : error(id: id, code: -32600, message: "Invalid request")
        }
        let params = msg["params"] as? [String: Any] ?? [:]
        switch method {
        case "initialize":
            let asked = params["protocolVersion"] as? String ?? ""
            let version = MCPServer.protocolVersions.contains(asked) ? asked : MCPServer.protocolVersions[0]
            return result(id: id, [
                "protocolVersion": version,
                "capabilities": ["tools": [String: Any]()],
                "serverInfo": ["name": "muesli", "title": "Muesli", "version": MuesliInfo.version],
                "instructions": "Access to Muesli, a local meeting transcriber. Use get_status to see whether a meeting is detected or being recorded, list_transcripts to find meetings, and get_transcript to read one. A transcript is complete when its state is \"ready\". Finished meetings can be managed: rename_transcript, set_attendees, rename_speaker (for example \"Others\" to a person's name) and delete_transcript, which moves the files to the macOS Trash. Confirm with the user before deleting. Recordings cannot be started or stopped.",
            ])
        case "ping":
            return result(id: id, [:])
        case "tools/list":
            return result(id: id, ["tools": Tools.definitions])
        case "tools/call":
            guard let name = params["name"] as? String, Tools.names.contains(name) else {
                return error(id: id, code: -32602, message: "Unknown tool")
            }
            let args = params["arguments"] as? [String: Any] ?? [:]
            if let refused = refusal() { return result(id: id, toolResult(refused, isError: true)) }
            audit(name, args["id"] as? String)
            switch Tools.call(name, args) {
            case .success(let text): return result(id: id, toolResult(text, isError: false))
            case .failure(let e): return result(id: id, toolResult(e.message, isError: true))
            }
        default:
            return error(id: id, code: -32601, message: "Method not found")
        }
    }

    private func toolResult(_ text: String, isError: Bool) -> [String: Any] {
        ["content": [["type": "text", "text": text]], "isError": isError]
    }

    private func result(id: Any, _ value: [String: Any]) -> [String: Any] {
        ["jsonrpc": "2.0", "id": id, "result": value]
    }

    private func error(id: Any, code: Int, message: String) -> [String: Any] {
        ["jsonrpc": "2.0", "id": id, "error": ["code": code, "message": message] as [String: Any]]
    }

    private func encode(_ value: Any) -> Data {
        (try? JSONSerialization.data(withJSONObject: value, options: [.sortedKeys, .withoutEscapingSlashes])) ?? Data("{}".utf8)
    }
}

struct ToolError: Error {
    var message: String
}

/// Three read tools, plus four tools that manage finished meetings in the store and the transcripts folder.
enum Tools {
    static let readNames: Set<String> = ["get_status", "list_transcripts", "get_transcript"]
    static let manageNames: Set<String> = ["rename_transcript", "set_attendees", "rename_speaker", "delete_transcript"]
    static let names: Set<String> = readNames.union(manageNames)
    static let responseCap = 60_000

    private static let readOnly: [String: Any] = ["readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false]
    private static let update: [String: Any] = ["readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false]
    private static let destructive: [String: Any] = ["readOnlyHint": false, "destructiveHint": true, "idempotentHint": false, "openWorldHint": false]
    private static let idProperty: [String: Any] = ["type": "string", "description": "Meeting id from list_transcripts or get_status."]

    private static func schema(_ properties: [String: Any], required: [String]) -> [String: Any] {
        ["type": "object", "properties": properties, "required": required, "additionalProperties": false]
    }

    static let definitions: [[String: Any]] = [
        [
            "name": "get_status",
            "title": "Muesli status",
            "description": "Whether Muesli is running, whether a call or meeting is detected, whether it is recording, and the most recent finished transcript.",
            "inputSchema": ["type": "object", "properties": [String: Any](), "additionalProperties": false] as [String: Any],
            "annotations": readOnly,
        ],
        [
            "name": "list_transcripts",
            "title": "List transcripts",
            "description": "List meetings known to Muesli, newest first. state is recording, finalizing, ready or failed; only ready transcripts are complete.",
            "inputSchema": [
                "type": "object",
                "properties": [
                    "since": ["type": "string", "description": "ISO 8601 date-time; only meetings started at or after it."],
                    "limit": ["type": "integer", "minimum": 1, "maximum": 100, "description": "Default 20."],
                    "state": ["type": "string", "enum": ["recording", "finalizing", "ready", "failed"]],
                ] as [String: Any],
                "additionalProperties": false,
            ] as [String: Any],
            "annotations": readOnly,
        ],
        [
            "name": "get_transcript",
            "title": "Get transcript",
            "description": "Read one meeting transcript by id. Works while the meeting is still recording (partial is then true). Long transcripts are paged: when truncated is true, call again with fromSeconds set to nextFromSeconds.",
            "inputSchema": [
                "type": "object",
                "properties": [
                    "id": ["type": "string", "description": "Meeting id from list_transcripts or get_status."],
                    "format": ["type": "string", "enum": ["markdown", "json"], "description": "Default markdown."],
                    "fromSeconds": ["type": "number", "minimum": 0, "description": "Only segments starting at or after this offset."],
                    "toSeconds": ["type": "number", "minimum": 0, "description": "Only segments starting before this offset."],
                ] as [String: Any],
                "required": ["id"],
                "additionalProperties": false,
            ] as [String: Any],
            "annotations": readOnly,
        ],
        [
            "name": "rename_transcript",
            "title": "Rename transcript",
            "description": "Change a finished meeting's title. The transcript files in the user's folder are renamed to match.",
            "inputSchema": schema(["id": idProperty, "title": ["type": "string", "minLength": 1, "maxLength": MeetingEditor.maxTitleLength]], required: ["id", "title"]),
            "annotations": update,
        ],
        [
            "name": "set_attendees",
            "title": "Set attendees",
            "description": "Replace the attendee list of a finished meeting. Pass an empty list to clear it.",
            "inputSchema": schema(["id": idProperty, "attendees": ["type": "array", "items": ["type": "string"], "maxItems": MeetingEditor.maxAttendees]], required: ["id", "attendees"]),
            "annotations": update,
        ],
        [
            "name": "rename_speaker",
            "title": "Rename speaker",
            "description": "Rename a speaker label in a finished transcript, for every segment with that label. Muesli labels the microphone \"You\" and everyone else \"Others\".",
            "inputSchema": schema([
                "id": idProperty,
                "from": ["type": "string", "description": "Current label, e.g. Others."],
                "to": ["type": "string", "minLength": 1, "maxLength": MeetingEditor.maxSpeakerLength, "description": "New label, e.g. Ann."],
            ], required: ["id", "from", "to"]),
            "annotations": update,
        ],
        [
            "name": "delete_transcript",
            "title": "Delete transcript",
            "description": "Delete a finished meeting. Its transcript files are moved to the macOS Trash, so the user can restore them. Ask the user to confirm first.",
            "inputSchema": schema(["id": idProperty], required: ["id"]),
            "annotations": destructive,
        ],
    ]

    static func call(_ name: String, _ args: [String: Any]) -> Result<String, ToolError> {
        switch name {
        case "get_status": return .success(json(status()))
        case "list_transcripts": return listTranscripts(args)
        case "get_transcript": return getTranscript(args)
        case "rename_transcript":
            guard let title = args["title"] as? String else { return .failure(ToolError(message: "title is required")) }
            return managed(args) { MeetingEditor.rename($0, to: title) }
        case "set_attendees":
            guard let list = args["attendees"] as? [Any], list.allSatisfy({ $0 is String }) else {
                return .failure(ToolError(message: "attendees must be a list of names"))
            }
            return managed(args) { MeetingEditor.setAttendees($0, to: list.compactMap { $0 as? String }) }
        case "rename_speaker":
            guard let from = args["from"] as? String, let to = args["to"] as? String else {
                return .failure(ToolError(message: "from and to are required"))
            }
            return managed(args) { MeetingEditor.renameSpeaker($0, from: from, to: to) }
        case "delete_transcript":
            guard let id = args["id"] as? String else { return .failure(ToolError(message: "id is required")) }
            switch MeetingEditor.delete(id) {
            case .success(let m): return .success(json(DeletedOut(deleted: m.id, title: m.title, movedToTrash: true)))
            case .failure(let e): return .failure(ToolError(message: e.message))
            }
        default: return .failure(ToolError(message: "Unknown tool"))
        }
    }

    // MARK: get_status

    struct StatusOut: Codable {
        var appRunning: Bool
        var appState: String
        var detected: AppStatus.Detected?
        var current: AppStatus.Current?
        var lastReady: AppStatus.LastReady?
        var error: String?
        var updatedAt: Date?
    }

    static func status(now: Date = Date()) -> StatusOut {
        if let s = AppStatus.read(), s.isFresh(now: now) {
            return StatusOut(appRunning: true, appState: s.appState, detected: s.detected, current: s.current,
                             lastReady: s.lastReady, error: s.error, updatedAt: s.updatedAt)
        }
        // Stale or missing heartbeat: never claim a recording is running.
        let last = MeetingStore.latestReady().map { AppStatus.LastReady(id: $0.id, title: $0.title, endedAt: $0.endedAt) }
        return StatusOut(appRunning: false, appState: "notRunning", detected: nil, current: nil,
                         lastReady: last, error: nil, updatedAt: AppStatus.read()?.updatedAt)
    }

    // MARK: list_transcripts

    struct Summary: Codable {
        var id: String
        var title: String
        var source: String
        var startedAt: Date
        var endedAt: Date?
        var state: MeetingState
        var partial: Bool
        var durationMinutes: Int?
        var attendees: [String]
        var segmentCount: Int
    }

    private static func listTranscripts(_ args: [String: Any]) -> Result<String, ToolError> {
        var meetings = MeetingStore.all()
        if let s = args["since"] as? String {
            guard let since = parseDate(s) else { return .failure(ToolError(message: "since must be an ISO 8601 date-time, e.g. 2026-11-13T09:00:00Z")) }
            meetings = meetings.filter { $0.startedAt >= since }
        }
        if let s = args["state"] as? String {
            guard let state = MeetingState(rawValue: s) else { return .failure(ToolError(message: "Unknown state")) }
            meetings = meetings.filter { $0.state == state }
        }
        let limit = min(max(int(args["limit"]) ?? 20, 1), 100)
        let out = meetings.prefix(limit).map { m in
            Summary(id: m.id, title: m.title, source: m.source, startedAt: m.startedAt, endedAt: m.endedAt,
                    state: m.state, partial: isPartial(m),
                    durationMinutes: m.endedAt.map { Int($0.timeIntervalSince(m.startedAt) / 60) },
                    attendees: m.attendees, segmentCount: m.transcript.count)
        }
        return .success(json(["transcripts": Array(out)]))
    }

    // MARK: get_transcript

    struct TranscriptOut: Codable {
        var id: String
        var title: String
        var source: String
        var startedAt: Date
        var endedAt: Date?
        var attendees: [String]
        var state: MeetingState
        var partial: Bool
        var truncated: Bool
        var nextFromSeconds: Double?
        var segments: [TranscriptSegment]
    }

    private static func getTranscript(_ args: [String: Any]) -> Result<String, ToolError> {
        guard let id = args["id"] as? String, let m = MeetingStore.load(id) else {
            return .failure(ToolError(message: "No meeting with that id. Use list_transcripts to find ids."))
        }
        let from = double(args["fromSeconds"]) ?? 0
        let to = double(args["toSeconds"])
        let asJSON = (args["format"] as? String) == "json"
        let wanted = m.transcript.sorted { $0.start < $1.start }
            .filter { $0.start >= from && (to == nil || $0.start < to!) }
        let (segments, next) = page(wanted, cap: responseCap) { asJSON ? $0.text.count + 80 : Store.line($0).count }

        if asJSON {
            return .success(json(TranscriptOut(
                id: m.id, title: m.title, source: m.source, startedAt: m.startedAt, endedAt: m.endedAt,
                attendees: m.attendees, state: m.state, partial: isPartial(m), truncated: next != nil,
                nextFromSeconds: next, segments: segments)))
        }
        var out = Store.header(m)
        out += "- Id: \(m.id)\n- State: \(m.state.rawValue)\n"
        if isPartial(m) { out += "- Partial: true (not the complete meeting)\n" }
        out += "\n## Transcript\n\n" + Store.lines(segments)
        if segments.isEmpty { out += "(no transcript text in this range)\n" }
        if let next { out += "[truncated: call get_transcript again with fromSeconds=\(next)]\n" }
        return .success(out)
    }

    /// Takes segments until the size cap. Returns the page and the offset to continue from, if cut.
    /// Never splits segments that share a start time across pages, so paging by start offset loses nothing.
    static func page(_ segments: [TranscriptSegment], cap: Int, size: (TranscriptSegment) -> Int) -> ([TranscriptSegment], Double?) {
        var total = 0
        var count = 0
        for s in segments {
            total += size(s)
            if total > cap && count > 0 { break }
            count += 1
        }
        guard count < segments.count else { return (segments, nil) }
        let next = segments[count].start
        var end = count
        while end > 0 && segments[end - 1].start == next { end -= 1 }
        if end == 0 {                                   // a run of equal starts larger than the cap: send the whole run
            end = count
            while end < segments.count && segments[end].start == next { end += 1 }
            return (Array(segments[..<end]), end < segments.count ? segments[end].start : nil)
        }
        return (Array(segments[..<end]), next)
    }

    // MARK: managing meetings

    struct DeletedOut: Codable {
        var deleted: String
        var title: String
        var movedToTrash: Bool
    }

    private static func managed(_ args: [String: Any], _ change: (String) -> Result<Meeting, MeetingEditor.EditError>) -> Result<String, ToolError> {
        guard let id = args["id"] as? String else { return .failure(ToolError(message: "id is required")) }
        switch change(id) {
        case .success(let m): return .success(json(summary(m)))
        case .failure(let e): return .failure(ToolError(message: e.message))
        }
    }

    static func summary(_ m: Meeting) -> Summary {
        Summary(id: m.id, title: m.title, source: m.source, startedAt: m.startedAt, endedAt: m.endedAt,
                state: m.state, partial: isPartial(m),
                durationMinutes: m.endedAt.map { Int($0.timeIntervalSince(m.startedAt) / 60) },
                attendees: m.attendees, segmentCount: m.transcript.count)
    }

    // MARK: helpers

    static func isPartial(_ m: Meeting) -> Bool {
        m.partial == true || m.state == .recording || m.state == .finalizing
    }

    private static func json<T: Encodable>(_ value: T) -> String {
        (try? JSONCoding.encoder().encode(value)).flatMap { String(data: $0, encoding: .utf8) } ?? "{}"
    }

    private static func parseDate(_ s: String) -> Date? {
        let f = ISO8601DateFormatter()
        if let d = f.date(from: s) { return d }
        f.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        if let d = f.date(from: s) { return d }
        f.formatOptions = [.withFullDate]
        return f.date(from: s)
    }

    private static func double(_ v: Any?) -> Double? { (v as? NSNumber)?.doubleValue }
    private static func int(_ v: Any?) -> Int? { (v as? NSNumber)?.intValue }
}
