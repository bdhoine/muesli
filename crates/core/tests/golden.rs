//! Files written by the Swift app (`fixtures/core` at the repo root) must load, and what this
//! crate writes must mean the same thing. If a format changes on purpose, regenerate them with
//! `scripts/regenerate-fixtures.sh`.

use muesli_core::json::to_pretty_sorted;
use muesli_core::store::Store;
use muesli_core::{AppStatus, Meeting, MeetingState, Preferences};

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!("{}/../../fixtures/core/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

/// Swift writes `4`, serde writes `4.0` for the same double: compare numbers by value.
fn normalise(v: serde_json::Value) -> serde_json::Value {
    use serde_json::Value::*;
    match v {
        Number(n) => serde_json::json!(n.as_f64().unwrap()),
        Array(a) => Array(a.into_iter().map(normalise).collect()),
        Object(o) => Object(o.into_iter().map(|(k, v)| (k, normalise(v))).collect()),
        other => other,
    }
}

fn same_json(a: &str, b: &str) {
    let a: serde_json::Value = serde_json::from_str(a).unwrap();
    let b: serde_json::Value = serde_json::from_str(b).unwrap();
    assert_eq!(normalise(a), normalise(b));
}

#[test]
fn swift_meeting_loads_and_reserialises_to_the_same_data() {
    let text = fixture("meeting.json");
    let m: Meeting = serde_json::from_str(&text).unwrap();
    assert_eq!(m.id, "golden-1");
    assert_eq!(m.state, MeetingState::Ready);
    assert_eq!(m.partial, Some(true));
    assert_eq!(m.transcript.len(), 3);
    assert_eq!(m.started_at.to_rfc3339(), "2027-01-15T08:00:00+00:00");
    same_json(&text, &to_pretty_sorted(&m));
}

#[test]
fn markdown_matches_what_the_swift_app_writes() {
    let m: Meeting = serde_json::from_str(&fixture("meeting.json")).unwrap();
    assert_eq!(Store::markdown(&m), fixture("meeting.md"));
}

#[test]
fn file_stem_has_the_same_shape_as_swift() {
    let m: Meeting = serde_json::from_str(&fixture("meeting.json")).unwrap();
    let ours = Store::file_stem(&m);
    let theirs = fixture("stem.txt");
    // The time part is in the local zone of whoever generated or runs the test.
    assert_eq!(ours.len(), theirs.len());
    assert_eq!(ours.split_once('_').unwrap().1.split_once('_').unwrap().1, "Weekly-sync");
    assert_eq!(theirs.split_once('_').unwrap().1.split_once('_').unwrap().1, "Weekly-sync");
}

#[test]
fn config_loads_and_reserialises_to_the_same_data() {
    let text = fixture("config.json");
    let p: Preferences = serde_json::from_str(&text).unwrap();
    assert!(p.mcp_enabled);
    assert_eq!(p.languages.as_deref(), Some(&["nl".to_string(), "en".to_string()][..]));
    assert!(p.onboarding_done);
    assert_eq!(p.transcripts_folder.as_deref(), Some("/Users/me/Documents/Meetings"));
    assert!(p.auto_update_check);
    assert_eq!(p.last_run_version.as_deref(), Some("0.1.0"));
    assert_eq!(p.update_notified.as_deref(), Some("0.2.0"));
    assert_eq!(p.update_last_checked.map(|d| d.to_rfc3339()).as_deref(), Some("2027-01-15T09:00:00+00:00"));
    same_json(&text, &to_pretty_sorted(&p));
}

#[test]
fn a_config_from_0_0_2_loads_with_its_old_keys_dropped() {
    let text = fixture("config-0.0.2.json");
    let p: Preferences = serde_json::from_str(&text).unwrap();
    assert!(p.mcp_enabled && p.onboarding_done);
    assert!(p.auto_update_check, "a config from before updates checks for them");
    assert_eq!(p.languages.as_deref(), Some(&["nl".to_string(), "en".to_string()][..]));
    let again = to_pretty_sorted(&p);
    for key in muesli_core::preferences::REMOVED_KEYS {
        assert!(!again.contains(&format!("\"{key}\"")), "{key}");
    }
}

#[test]
fn swift_state_loads_and_reserialises_to_the_same_data() {
    let text = fixture("state.json");
    let s: AppStatus = serde_json::from_str(&text).unwrap();
    assert_eq!(s.app_state, "recording");
    assert_eq!(s.pid, 4242);
    assert_eq!(s.current.as_ref().unwrap().segment_count, 3);
    same_json(&text, &to_pretty_sorted(&s));
}
