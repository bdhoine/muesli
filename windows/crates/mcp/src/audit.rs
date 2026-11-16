use std::io::Write;
use std::sync::Mutex;

use chrono::{SecondsFormat, Utc};
use muesli_core::Paths;
use serde_json::json;

static LOCK: Mutex<()> = Mutex::new(());

/// Append-only record of tool calls that change meetings: which tool, which meeting, when.
pub fn record(paths: &Paths, tool: &str, meeting_id: Option<&str>) {
    let mut entry = json!({ "time": Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true), "tool": tool });
    if let Some(id) = meeting_id {
        entry["meetingId"] = json!(id);
    }
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _ = std::fs::create_dir_all(&paths.support);
    if let Ok(mut file) = std::fs::OpenOptions::new().create(true).append(true).open(paths.audit_log()) {
        let _ = writeln!(file, "{entry}");
    }
}
