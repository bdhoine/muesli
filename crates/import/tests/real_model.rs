//! Transcribes real audio with the real models. Ignored by default: set `MUESLI_TEST_MODEL`,
//! `MUESLI_TEST_VAD` and `MUESLI_TEST_AUDIO` (a speech recording in any supported format) and run
//! `cargo test -p muesli-import --features whisper -- --ignored --nocapture`.
#![cfg(feature = "whisper")]

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use muesli_core::Paths;
use muesli_import::import_file;
use muesli_whisper::Transcriber;

fn env(name: &str) -> PathBuf {
    PathBuf::from(std::env::var(name).unwrap_or_else(|_| panic!("set {name}")))
}

#[test]
#[ignore = "needs the real models and a speech file"]
fn a_speech_file_gives_text() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(dir.path().join("support"), dir.path().join("transcripts"));
    std::fs::create_dir_all(&paths.support).unwrap();
    std::fs::create_dir_all(paths.transcripts_dir()).unwrap();
    let stt = Transcriber::new(&env("MUESLI_TEST_MODEL"), Some(&env("MUESLI_TEST_VAD")), vec!["en".into()]).expect("model loads");
    let meeting = import_file(&env("MUESLI_TEST_AUDIO"), &stt, &paths, &AtomicBool::new(false), |p| println!("progress {p:?}")).unwrap();
    for s in &meeting.transcript {
        println!("{:>6.1}s  {}", s.start, s.text);
    }
    assert!(!meeting.transcript.is_empty());
}
