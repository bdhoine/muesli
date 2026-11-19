use std::sync::{Arc, Mutex};

use muesli_core::store::MeetingStore;
use muesli_core::{Meeting, MeetingState, Paths, TranscriptSegment};
use muesli_mcp::tools::page;
use muesli_mcp::Server;
use serde_json::{json, Value};

fn call(server: &Server, name: &str, args: Value) -> Value {
    let req = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}});
    serde_json::from_slice(&server.handle(req.to_string().as_bytes()).unwrap()).unwrap()
}

#[test]
fn refusal_blocks_every_tool_and_is_reported_as_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let server = Server::new(Paths::new(dir.path().join("s"), dir.path().join("t")), trash(&dir)).with_refusal(|| Some("off".into()));
    let reply = call(&server, "get_status", json!({}));
    assert_eq!(reply["result"]["isError"], json!(true));
    assert_eq!(reply["result"]["content"][0]["text"], json!("off"));
}

#[test]
fn audit_sees_the_tool_and_the_meeting_id() {
    let dir = tempfile::tempdir().unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    let server = Server::new(Paths::new(dir.path().join("s"), dir.path().join("t")), trash(&dir))
        .with_audit(move |tool, id| sink.lock().unwrap().push((tool.to_string(), id.map(String::from))));
    call(&server, "rename_transcript", json!({"id": "a", "title": "x"}));
    call(&server, "get_status", json!({}));
    assert_eq!(*seen.lock().unwrap(), vec![("rename_transcript".to_string(), Some("a".to_string())), ("get_status".to_string(), None)]);
}

#[test]
fn a_fresh_heartbeat_is_reported_as_running_and_a_stale_one_is_not() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(dir.path().join("s"), dir.path().join("t"));
    let server = Server::new(paths.clone(), trash(&dir));
    let text = |v: &Value| serde_json::from_str::<Value>(v["result"]["content"][0]["text"].as_str().unwrap()).unwrap();

    assert_eq!(text(&call(&server, "get_status", json!({})))["appState"], json!("notRunning"));
    muesli_core::AppStatus::new("recording").write(&paths).unwrap();
    let status = text(&call(&server, "get_status", json!({})));
    assert_eq!(status["appRunning"], json!(true));
    assert_eq!(status["appState"], json!("recording"));

    let mut stale = muesli_core::AppStatus::new("recording");
    stale.updated_at -= chrono::Duration::seconds(120);
    stale.write(&paths).unwrap();
    let status = text(&call(&server, "get_status", json!({})));
    assert_eq!(status["appRunning"], json!(false));
    assert!(status.get("current").is_none());
}

#[test]
fn long_transcripts_are_paged_without_losing_segments() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(dir.path().join("s"), dir.path().join("t"));
    let mut m = Meeting::new("Long", "manual");
    m.state = MeetingState::Ready;
    m.transcript = (0..2000)
        .map(|i| TranscriptSegment { start: i as f64, end: i as f64 + 1.0, speaker: "You".into(), text: "word ".repeat(10) })
        .collect();
    MeetingStore::new(&paths).save(&m);
    let server = Server::new(paths, trash(&dir));

    let mut from = 0.0;
    let mut total = 0;
    for _ in 0..50 {
        let reply = call(&server, "get_transcript", json!({"id": m.id, "format": "json", "fromSeconds": from}));
        let body: Value = serde_json::from_str(reply["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
        total += body["segments"].as_array().unwrap().len();
        match body["nextFromSeconds"].as_f64() {
            Some(next) => from = next,
            None => break,
        }
    }
    assert_eq!(total, 2000);
}

#[test]
fn equal_start_runs_larger_than_the_cap_are_sent_whole() {
    let seg = |start: f64| TranscriptSegment { start, end: start + 1.0, speaker: "You".into(), text: "x".into() };
    let segments = vec![seg(0.0), seg(5.0), seg(5.0), seg(5.0), seg(9.0)];
    let (page1, next) = page(&segments, 10, |_| 6);
    assert_eq!(page1.len(), 1);
    assert_eq!(next, Some(5.0));
    let (page2, next) = page(&segments[1..], 10, |_| 6);
    assert_eq!(page2.len(), 3);
    assert_eq!(next, Some(9.0));
}

fn trash(dir: &tempfile::TempDir) -> std::sync::Arc<muesli_platform_fake::FolderTrash> {
    std::sync::Arc::new(muesli_platform_fake::FolderTrash::new(dir.path().join("Trash"), "Trash"))
}
