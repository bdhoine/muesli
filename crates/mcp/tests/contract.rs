//! Replays requests through the Rust server and compares every reply with what the Swift
//! `muesli-mcp` answered for the same requests and the same data (`fixtures/mcp/expected.jsonl`
//! at the repo root). If the protocol surface changes on purpose, regenerate it with
//! `scripts/regenerate-fixtures.sh`.

use std::path::Path;

use muesli_core::Paths;
use muesli_mcp::Server;
use serde_json::Value;

fn fixtures() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mcp")
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

/// Numbers by value (`4` and `4.0` are the same), tool result text parsed when it is JSON,
/// and the platform's name for the trash (Recycle Bin on Windows, just Trash on Linux) mapped back
/// to the macOS wording.
fn normalise(v: Value) -> Value {
    match v {
        Value::Number(n) => serde_json::json!(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.into_iter().map(normalise).collect()),
        Value::Object(o) => Value::Object(
            o.into_iter()
                .map(|(k, v)| {
                    let v = match (k.as_str(), v) {
                        ("text", Value::String(s)) => match serde_json::from_str::<Value>(&s) {
                            Ok(parsed) if parsed.is_object() => normalise(parsed),
                            _ => Value::String(s),
                        },
                        (_, v) => normalise(v),
                    };
                    (k, v)
                })
                .collect(),
        ),
        Value::String(s) => Value::String(s.replace("Recycle Bin", "macOS Trash").replace("the Trash", "the macOS Trash")),
        other => other,
    }
}

#[test]
fn replies_match_the_swift_server() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(dir.path().join("support"), dir.path().join("transcripts"));
    copy_dir(&fixtures().join("support"), &paths.support);

    let server = Server::new(paths.clone(), trash(&dir));
    let requests = std::fs::read_to_string(fixtures().join("requests.jsonl")).unwrap();
    let expected = std::fs::read_to_string(fixtures().join("expected.jsonl")).unwrap();

    let mut replies = Vec::new();
    for line in requests.lines().filter(|l| !l.is_empty()) {
        if let Some(reply) = server.handle(line.as_bytes()) {
            replies.push((line.to_string(), String::from_utf8(reply).unwrap()));
        }
    }
    let expected: Vec<&str> = expected.lines().collect();
    assert_eq!(replies.len(), expected.len(), "number of replies");
    for ((request, reply), want) in replies.iter().zip(expected) {
        let got: Value = serde_json::from_str(reply).unwrap();
        let want: Value = serde_json::from_str(want).unwrap();
        assert_eq!(normalise(got), normalise(want), "request: {request}");
    }
}

#[test]
fn envelopes_are_byte_identical_for_simple_requests() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(dir.path().join("support"), dir.path().join("transcripts"));
    let server = Server::new(paths, trash(&dir));
    let reply = server.handle(br#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#).unwrap();
    assert_eq!(String::from_utf8(reply).unwrap(), r#"{"id":2,"jsonrpc":"2.0","result":{}}"#);
    assert!(server.handle(br#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());
}

fn trash(dir: &tempfile::TempDir) -> std::sync::Arc<muesli_platform_fake::FolderTrash> {
    std::sync::Arc::new(muesli_platform_fake::FolderTrash::new(dir.path().join("Trash"), "Trash"))
}
