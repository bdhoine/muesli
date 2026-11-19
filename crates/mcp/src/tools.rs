use std::path::Path;

use chrono::{DateTime, NaiveDate, Utc};
use muesli_core::editor::{EditError, MeetingEditor, MAX_ATTENDEES, MAX_SPEAKER_LENGTH, MAX_TITLE_LENGTH};
use muesli_core::json::{iso_date, iso_date_opt, to_pretty_sorted};
use muesli_core::status::{Current, Detected, LastReady};
use muesli_core::store::{MeetingStore, Store};
use muesli_core::{AppStatus, Meeting, MeetingState, Paths, TranscriptSegment};
use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::trash_name;
use muesli_core::ports::Trash;
use std::sync::Arc;

pub const RESPONSE_CAP: usize = 60_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolError {
    pub message: String,
}

impl ToolError {
    fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl From<EditError> for ToolError {
    fn from(e: EditError) -> Self {
        Self::new(e.message())
    }
}

type Discard = Box<dyn Fn(&Path) + Send + Sync>;

/// Three read tools, plus four tools that manage finished meetings in the store and the
/// transcripts folder.
pub struct Tools {
    paths: Paths,
    discard: Discard,
}

pub fn instructions() -> String {
    format!(
        "Access to Muesli, a local meeting transcriber. Use get_status to see whether a meeting is detected or being recorded, list_transcripts to find meetings, and get_transcript to read one. A transcript is complete when its state is \"ready\". Finished meetings can be managed: rename_transcript, set_attendees, rename_speaker (for example \"Others\" to a person's name) and delete_transcript, which moves the files to the {}. Confirm with the user before deleting. Recordings cannot be started or stopped.",
        trash_name()
    )
}

impl Tools {
    pub const READ_NAMES: [&'static str; 3] = ["get_status", "list_transcripts", "get_transcript"];
    pub const MANAGE_NAMES: [&'static str; 4] = ["rename_transcript", "set_attendees", "rename_speaker", "delete_transcript"];

    pub fn is_tool(name: &str) -> bool {
        Self::READ_NAMES.contains(&name) || Self::MANAGE_NAMES.contains(&name)
    }

    /// Deleted transcripts go to `trash` (the Trash / Recycle Bin) so the user can restore them.
    pub fn new(paths: Paths, trash: Arc<dyn Trash>) -> Self {
        Self {
            paths,
            discard: Box::new(move |p| {
                if p.exists() && trash.trash(p).is_err() {
                    let _ = std::fs::remove_file(p);
                }
            }),
        }
    }

    /// Replaces how deleted files are discarded (tests do not touch the real trash).
    pub fn with_discard(mut self, discard: impl Fn(&Path) + Send + Sync + 'static) -> Self {
        self.discard = Box::new(discard);
        self
    }

    pub fn definitions() -> Value {
        let read_only = json!({ "readOnlyHint": true, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false });
        let update = json!({ "readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false });
        let destructive = json!({ "readOnlyHint": false, "destructiveHint": true, "idempotentHint": false, "openWorldHint": false });
        let id_property = json!({ "type": "string", "description": "Meeting id from list_transcripts or get_status." });
        let schema = |properties: Value, required: Value| json!({ "type": "object", "properties": properties, "required": required, "additionalProperties": false });
        json!([
            {
                "name": "get_status",
                "title": "Muesli status",
                "description": "Whether Muesli is running, whether a call or meeting is detected, whether it is recording, and the most recent finished transcript.",
                "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false },
                "annotations": read_only,
            },
            {
                "name": "list_transcripts",
                "title": "List transcripts",
                "description": "List meetings known to Muesli, newest first. state is recording, finalizing, ready or failed; only ready transcripts are complete.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "since": { "type": "string", "description": "ISO 8601 date-time; only meetings started at or after it." },
                        "limit": { "type": "integer", "minimum": 1, "maximum": 100, "description": "Default 20." },
                        "state": { "type": "string", "enum": ["recording", "finalizing", "ready", "failed"] },
                    },
                    "additionalProperties": false,
                },
                "annotations": read_only,
            },
            {
                "name": "get_transcript",
                "title": "Get transcript",
                "description": "Read one meeting transcript by id. Works while the meeting is still recording (partial is then true). Long transcripts are paged: when truncated is true, call again with fromSeconds set to nextFromSeconds.",
                "inputSchema": {
                    "type": "object",
                    "properties": {
                        "id": { "type": "string", "description": "Meeting id from list_transcripts or get_status." },
                        "format": { "type": "string", "enum": ["markdown", "json"], "description": "Default markdown." },
                        "fromSeconds": { "type": "number", "minimum": 0, "description": "Only segments starting at or after this offset." },
                        "toSeconds": { "type": "number", "minimum": 0, "description": "Only segments starting before this offset." },
                    },
                    "required": ["id"],
                    "additionalProperties": false,
                },
                "annotations": read_only,
            },
            {
                "name": "rename_transcript",
                "title": "Rename transcript",
                "description": "Change a finished meeting's title. The transcript files in the user's folder are renamed to match.",
                "inputSchema": schema(json!({ "id": id_property, "title": { "type": "string", "minLength": 1, "maxLength": MAX_TITLE_LENGTH } }), json!(["id", "title"])),
                "annotations": update,
            },
            {
                "name": "set_attendees",
                "title": "Set attendees",
                "description": "Replace the attendee list of a finished meeting. Pass an empty list to clear it.",
                "inputSchema": schema(json!({ "id": id_property, "attendees": { "type": "array", "items": { "type": "string" }, "maxItems": MAX_ATTENDEES } }), json!(["id", "attendees"])),
                "annotations": update,
            },
            {
                "name": "rename_speaker",
                "title": "Rename speaker",
                "description": "Rename a speaker label in a finished transcript, for every segment with that label. Muesli labels the microphone \"You\" and everyone else \"Others\".",
                "inputSchema": schema(json!({
                    "id": id_property,
                    "from": { "type": "string", "description": "Current label, e.g. Others." },
                    "to": { "type": "string", "minLength": 1, "maxLength": MAX_SPEAKER_LENGTH, "description": "New label, e.g. Ann." },
                }), json!(["id", "from", "to"])),
                "annotations": update,
            },
            {
                "name": "delete_transcript",
                "title": "Delete transcript",
                "description": format!("Delete a finished meeting. Its transcript files are moved to the {}, so the user can restore them. Ask the user to confirm first.", trash_name()),
                "inputSchema": schema(json!({ "id": id_property }), json!(["id"])),
                "annotations": destructive,
            },
        ])
    }

    pub fn call(&self, name: &str, args: &Map<String, Value>) -> Result<String, ToolError> {
        match name {
            "get_status" => Ok(json_text(&self.status(Utc::now()))),
            "list_transcripts" => self.list_transcripts(args),
            "get_transcript" => self.get_transcript(args),
            "rename_transcript" => {
                let title = str_arg(args, "title").ok_or_else(|| ToolError::new("title is required"))?;
                self.managed(args, |editor, id| editor.rename(id, title))
            }
            "set_attendees" => {
                let list = args
                    .get("attendees")
                    .and_then(Value::as_array)
                    .filter(|a| a.iter().all(Value::is_string))
                    .ok_or_else(|| ToolError::new("attendees must be a list of names"))?;
                let names: Vec<String> = list.iter().filter_map(|v| v.as_str().map(String::from)).collect();
                self.managed(args, |editor, id| editor.set_attendees(id, &names))
            }
            "rename_speaker" => {
                let (Some(from), Some(to)) = (str_arg(args, "from"), str_arg(args, "to")) else {
                    return Err(ToolError::new("from and to are required"));
                };
                self.managed(args, |editor, id| editor.rename_speaker(id, from, to))
            }
            "delete_transcript" => {
                let id = str_arg(args, "id").ok_or_else(|| ToolError::new("id is required"))?;
                let editor = self.editor();
                let m = editor.delete(id)?;
                Ok(json_text(&Deleted { deleted: m.id, title: m.title, moved_to_trash: true }))
            }
            _ => Err(ToolError::new("Unknown tool")),
        }
    }

    fn editor(&self) -> MeetingEditor<'_> {
        MeetingEditor::new(&self.paths, self.paths.export_folder()).with_discard(&*self.discard)
    }

    fn managed(
        &self,
        args: &Map<String, Value>,
        change: impl FnOnce(&MeetingEditor<'_>, &str) -> Result<Meeting, EditError>,
    ) -> Result<String, ToolError> {
        let id = str_arg(args, "id").ok_or_else(|| ToolError::new("id is required"))?;
        let editor = self.editor();
        Ok(json_text(&summary(&change(&editor, id)?)))
    }

    // MARK: get_status

    pub fn status(&self, now: DateTime<Utc>) -> StatusOut {
        let stored = AppStatus::read(&self.paths);
        if let Some(s) = stored.as_ref().filter(|s| s.is_fresh(now)) {
            return StatusOut {
                app_running: true,
                app_state: s.app_state.clone(),
                detected: s.detected.clone(),
                current: s.current.clone(),
                last_ready: s.last_ready.clone(),
                error: s.error.clone(),
                updated_at: Some(s.updated_at),
            };
        }
        // Stale or missing heartbeat: never claim a recording is running.
        let last = MeetingStore::new(&self.paths).latest_ready().map(|m| LastReady { id: m.id, title: m.title, ended_at: m.ended_at });
        StatusOut {
            app_running: false,
            app_state: "notRunning".into(),
            detected: None,
            current: None,
            last_ready: last,
            error: None,
            updated_at: stored.map(|s| s.updated_at),
        }
    }

    // MARK: list_transcripts

    fn list_transcripts(&self, args: &Map<String, Value>) -> Result<String, ToolError> {
        let mut meetings = MeetingStore::new(&self.paths).all();
        if let Some(s) = args.get("since").and_then(Value::as_str) {
            let since = parse_date(s).ok_or_else(|| ToolError::new("since must be an ISO 8601 date-time, e.g. 2026-11-13T09:00:00Z"))?;
            meetings.retain(|m| m.started_at >= since);
        }
        if let Some(s) = args.get("state").and_then(Value::as_str) {
            let state: MeetingState = s.parse().map_err(|_| ToolError::new("Unknown state"))?;
            meetings.retain(|m| m.state == state);
        }
        let limit = number(args.get("limit")).map_or(20, |n| n as i64).clamp(1, 100) as usize;
        let transcripts: Vec<Summary> = meetings.iter().take(limit).map(summary).collect();
        Ok(json_text(&json!({ "transcripts": transcripts })))
    }

    // MARK: get_transcript

    fn get_transcript(&self, args: &Map<String, Value>) -> Result<String, ToolError> {
        let m = str_arg(args, "id")
            .and_then(|id| MeetingStore::new(&self.paths).load(id))
            .ok_or_else(|| ToolError::new("No meeting with that id. Use list_transcripts to find ids."))?;
        let from = number(args.get("fromSeconds")).unwrap_or(0.0);
        let to = number(args.get("toSeconds"));
        let as_json = args.get("format").and_then(Value::as_str) == Some("json");

        let mut wanted: Vec<TranscriptSegment> =
            m.transcript.iter().filter(|s| s.start >= from && to.is_none_or(|to| s.start < to)).cloned().collect();
        wanted.sort_by(|a, b| a.start.total_cmp(&b.start));
        let (segments, next) =
            page(&wanted, RESPONSE_CAP, |s| if as_json { s.text.chars().count() + 80 } else { Store::line(s).chars().count() });

        if as_json {
            return Ok(json_text(&TranscriptOut {
                id: m.id.clone(),
                title: m.title.clone(),
                source: m.source.clone(),
                started_at: m.started_at,
                ended_at: m.ended_at,
                attendees: m.attendees.clone(),
                state: m.state,
                partial: m.is_partial(),
                truncated: next.is_some(),
                next_from_seconds: next,
                segments,
            }));
        }
        let mut out = Store::header(&m);
        out += &format!("- Id: {}\n- State: {}\n", m.id, m.state.as_str());
        if m.is_partial() {
            out += "- Partial: true (not the complete meeting)\n";
        }
        out += "\n## Transcript\n\n";
        out += &Store::lines(&segments);
        if segments.is_empty() {
            out += "(no transcript text in this range)\n";
        }
        if let Some(next) = next {
            out += &format!("[truncated: call get_transcript again with fromSeconds={}]\n", swift_double(next));
        }
        Ok(out)
    }
}

/// Takes segments until the size cap. Returns the page and the offset to continue from, if cut.
/// Never splits segments that share a start time across pages, so paging by start offset loses nothing.
pub fn page(
    segments: &[TranscriptSegment],
    cap: usize,
    size: impl Fn(&TranscriptSegment) -> usize,
) -> (Vec<TranscriptSegment>, Option<f64>) {
    let mut total = 0;
    let mut count = 0;
    for s in segments {
        total += size(s);
        if total > cap && count > 0 {
            break;
        }
        count += 1;
    }
    if count >= segments.len() {
        return (segments.to_vec(), None);
    }
    let next = segments[count].start;
    let mut end = count;
    while end > 0 && segments[end - 1].start == next {
        end -= 1;
    }
    if end == 0 {
        // A run of equal starts larger than the cap: send the whole run.
        end = count;
        while end < segments.len() && segments[end].start == next {
            end += 1;
        }
        return (segments[..end].to_vec(), segments.get(end).map(|s| s.start));
    }
    (segments[..end].to_vec(), Some(next))
}

// MARK: output shapes

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusOut {
    pub app_running: bool,
    pub app_state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detected: Option<Detected>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current: Option<Current>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_ready: Option<LastReady>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, with = "iso_date_opt", skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub id: String,
    pub title: String,
    pub source: String,
    #[serde(with = "iso_date")]
    pub started_at: DateTime<Utc>,
    #[serde(default, with = "iso_date_opt", skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<DateTime<Utc>>,
    pub state: MeetingState,
    pub partial: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_minutes: Option<i64>,
    pub attendees: Vec<String>,
    pub segment_count: usize,
}

pub fn summary(m: &Meeting) -> Summary {
    Summary {
        id: m.id.clone(),
        title: m.title.clone(),
        source: m.source.clone(),
        started_at: m.started_at,
        ended_at: m.ended_at,
        state: m.state,
        partial: m.is_partial(),
        duration_minutes: m.ended_at.map(|e| (e - m.started_at).num_seconds() / 60),
        attendees: m.attendees.clone(),
        segment_count: m.transcript.len(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TranscriptOut {
    id: String,
    title: String,
    source: String,
    #[serde(with = "iso_date")]
    started_at: DateTime<Utc>,
    #[serde(default, with = "iso_date_opt", skip_serializing_if = "Option::is_none")]
    ended_at: Option<DateTime<Utc>>,
    attendees: Vec<String>,
    state: MeetingState,
    partial: bool,
    truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    next_from_seconds: Option<f64>,
    segments: Vec<TranscriptSegment>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Deleted {
    deleted: String,
    title: String,
    moved_to_trash: bool,
}

// MARK: helpers

fn json_text<T: Serialize>(value: &T) -> String {
    to_pretty_sorted(value)
}

fn str_arg<'a>(args: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    args.get(key).and_then(Value::as_str)
}

fn number(v: Option<&Value>) -> Option<f64> {
    v.and_then(Value::as_f64)
}

/// Full date-time (with or without fractional seconds), or a plain date at midnight UTC.
fn parse_date(s: &str) -> Option<DateTime<Utc>> {
    if let Ok(d) = DateTime::parse_from_rfc3339(s) {
        return Some(d.with_timezone(&Utc));
    }
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok().and_then(|d| d.and_hms_opt(0, 0, 0)).map(|d| d.and_utc())
}

/// Swift prints a whole `Double` with one decimal (`30.0`).
fn swift_double(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{v:.1}")
    } else {
        v.to_string()
    }
}
