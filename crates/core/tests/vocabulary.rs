//! The vocabulary: its file, its rules and Whisper's prompt.

use muesli_core::vocabulary::{whisper_prompt, Vocabulary, MAX_TERMS, MAX_TERM_LENGTH};
use muesli_core::Paths;

fn terms(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

#[test]
fn terms_are_cleaned_and_kept_once_whatever_their_case() {
    let mut v = Vocabulary::default();
    v.add(&terms(&["  LocalVQE ", "Barry\n d'Hoine", "", "localvqe", "Muesli", "MUESLI"])).unwrap();
    assert_eq!(v.terms, ["LocalVQE", "Barry d'Hoine", "Muesli"]);
    v.add(&terms(&["Ghent"])).unwrap();
    assert_eq!(v.terms.last().unwrap(), "Ghent", "a new term goes to the end");
}

#[test]
fn removing_ignores_case_and_unknown_terms() {
    let mut v = Vocabulary::default();
    v.add(&terms(&["LocalVQE", "Muesli"])).unwrap();
    v.remove(&terms(&["localvqe", "nothing"]));
    assert_eq!(v.terms, ["Muesli"]);
}

#[test]
fn limits_refuse_the_whole_change() {
    let mut v = Vocabulary::default();
    assert!(v.add(&terms(&["ok", &"x".repeat(MAX_TERM_LENGTH + 1)])).is_err());
    assert!(v.terms.is_empty(), "nothing added when one term is refused");
    let many: Vec<String> = (0..MAX_TERMS).map(|i| format!("term{i}")).collect();
    v.add(&many).unwrap();
    assert!(v.add(&terms(&["one more"])).is_err());
    assert_eq!(v.terms.len(), MAX_TERMS);
}

#[test]
fn the_file_is_saved_and_loaded_and_a_missing_one_is_empty() {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(dir.path(), dir.path().join("t"));
    assert_eq!(Vocabulary::load(&paths), Vocabulary::default());
    let mut v = Vocabulary::default();
    v.add(&terms(&["LocalVQE"])).unwrap();
    v.save(&paths).unwrap();
    assert_eq!(Vocabulary::load(&paths), v);
    std::fs::write(paths.vocabulary(), "not json").unwrap();
    assert_eq!(Vocabulary::load(&paths), Vocabulary::default());
}

#[test]
fn the_prompt_lists_the_vocabulary_then_the_meeting() {
    let mut v = Vocabulary::default();
    v.add(&terms(&["LocalVQE", "Muesli"])).unwrap();
    assert_eq!(
        whisper_prompt(&v, "Budget review Q3", &terms(&["Anna Peeters", "muesli"])),
        "Glossary: LocalVQE, Muesli, Budget review Q3, Anna Peeters."
    );
    assert_eq!(whisper_prompt(&v, "Teams call", &[]), "Glossary: LocalVQE, Muesli.", "a generic title says nothing");
    assert_eq!(whisper_prompt(&Vocabulary::default(), "Meeting", &[]), "");
}
