mod common;

use chrono::Duration;
use common::*;
use muesli_core::store::{MeetingStore, Store};
use muesli_core::{Meeting, MeetingState, TranscriptSegment};

#[test]
fn save_writes_a_markdown_and_json_pair() {
    let env = Env::new();
    let m = simple("a", MeetingState::Ready);
    let md = Store::save(&m, &env.paths.transcripts);
    assert!(md.exists());
    assert!(md.with_extension("json").exists());
    let text = std::fs::read_to_string(&md).unwrap();
    assert!(text.starts_with("# Weekly sync\n"));
    assert!(text.contains("- Duration: 30 min\n"));
    assert!(text.contains("- Attendees: Ann\n"));
    assert!(text.contains("[00:00] **You**: Segment 0\n\n[00:05] **Others**: Segment 1\n\n"));
}

#[test]
fn file_name_falls_back_and_is_bounded() {
    let mut m = simple("a", MeetingState::Ready);
    m.title = "???".into();
    assert!(Store::file_stem(&m).ends_with("_Meeting"));
    m.title = "x".repeat(200);
    assert_eq!(Store::file_stem(&m).split_once('_').unwrap().1.split_once('_').unwrap().1.len(), 60);
    m.title = "Q3 / planning: part 2".into();
    assert!(Store::file_stem(&m).ends_with("_Q3-planning-part-2"));
}

#[test]
fn markdown_omits_what_is_unknown() {
    let mut m = simple("a", MeetingState::Recording);
    m.attendees.clear();
    let text = Store::markdown(&m);
    assert!(!text.contains("Ended"));
    assert!(!text.contains("Duration"));
    assert!(!text.contains("Attendees"));
    assert!(!text.contains("Partial"));
    m.partial = Some(true);
    assert!(Store::markdown(&m).contains("- Partial: recording was interrupted"));
}

#[test]
fn timestamps_past_an_hour_keep_counting_minutes() {
    let mut m = simple("a", MeetingState::Ready);
    m.transcript = vec![TranscriptSegment { start: 3725.0, end: 3730.0, speaker: "You".into(), text: "Late".into() }];
    assert!(Store::markdown(&m).contains("[62:05] **You**: Late"));
}

#[test]
fn recent_lists_newest_markdown_first_and_honours_the_limit() {
    let env = Env::new();
    for (i, title) in ["One", "Two", "Three"].iter().enumerate() {
        let m = meeting(&format!("m{i}"), MeetingState::Ready, 1, title, t0() + Duration::hours(i as i64));
        let md = Store::save(&m, &env.paths.transcripts);
        let when = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_800_000_000 + i as u64 * 100);
        std::fs::File::options().write(true).open(&md).unwrap().set_modified(when).unwrap();
    }
    let recent = Store::recent(&env.paths.transcripts, 2);
    assert_eq!(recent.len(), 2);
    assert!(recent[0].to_string_lossy().contains("Three"));
    assert!(recent[1].to_string_lossy().contains("Two"));
    let list = Store::recent_transcripts(&env.paths.transcripts, 5);
    assert_eq!(list[0].title, "Three");
    assert_eq!(list[0].duration_minutes(), Some(30));
    assert!(Store::recent_transcripts(&env.paths.transcripts, 0).is_empty());
}

#[test]
fn store_round_trip_and_rewrite_during_recording() {
    let env = Env::new();
    let store = MeetingStore::new(&env.paths);
    let mut m = meeting("live", MeetingState::Recording, 1, "Live", t0());
    store.save(&m);
    assert_eq!(store.load("live").unwrap().transcript.len(), 1);
    m.transcript.push(TranscriptSegment { start: 30.0, end: 34.0, speaker: "Others".into(), text: "More".into() });
    store.save(&m);
    assert_eq!(store.load("live").unwrap().transcript.len(), 2);
    assert_eq!(store.all().len(), 1);
}

#[test]
fn store_ignores_foreign_files_and_unsafe_ids() {
    let env = Env::new();
    let store = MeetingStore::new(&env.paths);
    std::fs::write(env.paths.meetings().join("broken.json"), "not json").unwrap();
    std::fs::write(env.paths.meetings().join("notes.txt"), "x").unwrap();
    store.save(&simple("ok", MeetingState::Ready));
    assert_eq!(store.all().iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["ok"]);

    store.save(&simple("../escape", MeetingState::Ready));
    assert!(!env.paths.support.join("escape.json").exists());
    assert!(store.load("../config").is_none());
    assert!(store.load("").is_none());
    assert!(store.load("ok").is_some());
}

#[test]
fn new_meeting_ids_are_accepted_by_the_store() {
    let env = Env::new();
    let store = MeetingStore::new(&env.paths);
    let m = Meeting::new("t", "manual");
    assert_eq!(m.state, MeetingState::Recording);
    store.save(&m);
    assert!(store.load(&m.id).is_some());
}

#[test]
fn latest_ready_picks_the_last_ended() {
    let env = Env::new();
    let store = MeetingStore::new(&env.paths);
    store.save(&meeting("a", MeetingState::Ready, 2, "A", t0()));
    store.save(&meeting("b", MeetingState::Ready, 2, "B", t0() + Duration::seconds(7200)));
    store.save(&meeting("c", MeetingState::Recording, 2, "C", t0() + Duration::seconds(9000)));
    store.save(&meeting("d", MeetingState::Failed, 2, "D", t0() + Duration::seconds(9500)));
    assert_eq!(store.latest_ready().unwrap().id, "b");
}

#[test]
fn transcripts_written_before_the_store_existed_are_imported() {
    let env = Env::new();
    let store = MeetingStore::new(&env.paths);
    std::fs::create_dir_all(&env.paths.transcripts).unwrap();
    let legacy = r#"{"id":"FBD41FE0-CBBD-47C9-A89B-69F86F42FB55","title":"Meeting","source":"manual",
      "startedAt":"2026-11-13T15:32:30Z","endedAt":"2026-11-13T15:32:56Z","attendees":[],
      "transcript":[{"start":0,"end":2,"speaker":"You","text":"Hello"}]}"#;
    std::fs::write(env.paths.transcripts.join("2026-11-13_1532_Meeting.json"), legacy).unwrap();
    std::fs::write(env.paths.transcripts.join("broken.json"), "not json").unwrap();
    std::fs::write(env.paths.transcripts.join("2026-11-13_1532_Meeting.md"), "# Meeting").unwrap();

    let imported = store.import_transcripts(&env.paths.transcripts);
    assert_eq!(imported.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["FBD41FE0-CBBD-47C9-A89B-69F86F42FB55"]);
    let m = store.load("FBD41FE0-CBBD-47C9-A89B-69F86F42FB55").unwrap();
    assert_eq!(m.state, MeetingState::Ready); // files written before states existed
    assert_eq!(m.transcript.len(), 1);
    assert_eq!(store.all().len(), 1);
}

#[test]
fn import_never_overwrites_the_store() {
    let env = Env::new();
    let store = MeetingStore::new(&env.paths);
    let mut stored = meeting("same", MeetingState::Ready, 3, "Same", t0());
    store.save(&stored);
    stored.transcript.clear();
    Store::save(&stored, &env.paths.transcripts);
    assert!(store.import_transcripts(&env.paths.transcripts).is_empty());
    assert_eq!(store.load("same").unwrap().transcript.len(), 3);
}

#[test]
fn interrupted_recording_is_saved_as_partial() {
    let env = Env::new();
    let store = MeetingStore::new(&env.paths);
    let mut m = meeting("crashed", MeetingState::Recording, 0, "Crashed call", t0());
    m.transcript = vec![
        TranscriptSegment { start: 30.0, end: 58.0, speaker: "Others".into(), text: "Later".into() },
        TranscriptSegment { start: 0.0, end: 20.0, speaker: "You".into(), text: "Earlier".into() },
    ];
    store.save(&m);
    store.save(&simple("fine", MeetingState::Ready));

    let changed = store.recover_interrupted();
    assert_eq!(changed.iter().map(|m| m.id.as_str()).collect::<Vec<_>>(), ["crashed"]);
    let back = store.load("crashed").unwrap();
    assert_eq!(back.state, MeetingState::Ready);
    assert_eq!(back.partial, Some(true));
    assert_eq!(back.transcript.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(), ["Earlier", "Later"]);
    assert_eq!(back.ended_at, Some(m.started_at + Duration::seconds(58)));

    let exported: Vec<_> =
        Store::recent(&env.paths.transcripts, 5).into_iter().filter(|p| p.to_string_lossy().contains("Crashed-call")).collect();
    assert_eq!(exported.len(), 1);
    assert!(std::fs::read_to_string(&exported[0]).unwrap().contains("- Partial: recording was interrupted"));
    assert_eq!(store.load("fine").unwrap().partial, None);
}

#[test]
fn interrupted_recording_with_nothing_transcribed_fails() {
    let env = Env::new();
    let store = MeetingStore::new(&env.paths);
    store.save(&meeting("empty", MeetingState::Finalizing, 0, "Empty", t0()));
    store.recover_interrupted();
    assert_eq!(store.load("empty").unwrap().state, MeetingState::Failed);
    assert!(Store::recent(&env.paths.transcripts, 5).is_empty());
}

#[test]
fn recovery_is_idempotent() {
    let env = Env::new();
    let store = MeetingStore::new(&env.paths);
    store.save(&simple("crashed", MeetingState::Recording));
    assert_eq!(store.recover_interrupted().len(), 1);
    assert_eq!(store.recover_interrupted().len(), 0);
    assert_eq!(Store::recent(&env.paths.transcripts, 5).len(), 1);
}

#[test]
fn appending_and_finishing_a_recording() {
    use muesli_core::store::shorten_title;
    let env = Env::new();
    let store = MeetingStore::new(&env.paths);
    let mut m = meeting("live", MeetingState::Recording, 0, "Live call", t0());
    store.append(&mut m, vec![TranscriptSegment { start: 30.0, end: 31.0, speaker: "Others".into(), text: "b".into() }]);
    store.append(&mut m, vec![TranscriptSegment { start: 0.0, end: 1.0, speaker: "You".into(), text: "a".into() }]);
    store.append(&mut m, vec![]);
    assert_eq!(store.load("live").unwrap().transcript.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(), ["a", "b"]);

    let end = t0() + Duration::minutes(12);
    let md = store.finish(&mut m, end);
    assert!(md.exists() && md.with_extension("json").exists());
    let saved = store.load("live").unwrap();
    assert_eq!((saved.state, saved.ended_at), (MeetingState::Ready, Some(end)));
    assert_eq!(store.latest_ready().unwrap().id, "live");

    assert_eq!(shorten_title("Short", 44), "Short");
    let long = shorten_title("2026-11-16_1352_AEM-header-footer-via-content-domain-service.summary", 44);
    assert_eq!(long.chars().count(), 44);
    assert!(long.starts_with("2026-11-16_1352_AEM-header-f") && long.ends_with("ervice.summary") && long.contains('…'));
    assert_eq!(shorten_title("ééééé", 4).chars().count(), 4); // counts characters, not bytes
}

#[test]
fn concurrent_writers_never_corrupt_a_meeting_file() {
    let env = Env::new();
    let paths = env.paths.clone();
    let handles: Vec<_> = (0..8)
        .map(|i| {
            let paths = paths.clone();
            std::thread::spawn(move || {
                let store = MeetingStore::new(&paths);
                for n in 0..40 {
                    store.save(&meeting("shared", MeetingState::Ready, (i + n) % 5 + 1, "Shared", t0()));
                }
            })
        })
        .collect();
    let reader = MeetingStore::new(&env.paths);
    for _ in 0..200 {
        if env.paths.meetings().join("shared.json").exists() {
            assert!(reader.load("shared").is_some(), "a reader saw a partial file");
        }
    }
    handles.into_iter().for_each(|h| h.join().unwrap());
    assert!(reader.load("shared").is_some());
    assert_eq!(std::fs::read_dir(env.paths.meetings()).unwrap().count(), 1); // no temp files left
}
