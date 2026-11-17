//! Rules behind the Settings window. The macOS app has the same tests (SettingsLogicTests.swift).
mod common;

use common::Env;
use muesli_core::assistants::{ClaudeDesktopExtension, ExtensionState};
use muesli_core::Preferences;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

#[test]
fn switching_to_system_languages_keeps_the_own_list() {
    let mut p = Preferences { languages: Some(s(&["fr", "de"])), ..Default::default() };
    p.set_uses_system_languages(true, &s(&["nl", "en"]));
    assert!(p.uses_system_languages());
    assert_eq!(p.saved_languages, Some(s(&["fr", "de"])));
    p.set_uses_system_languages(false, &s(&["nl", "en"]));
    assert_eq!(p.languages, Some(s(&["fr", "de"]))); // the own list is back
    p.set_uses_system_languages(false, &s(&["nl", "en"]));
    assert_eq!(p.languages, Some(s(&["fr", "de"])));
}

#[test]
fn first_own_list_starts_from_the_system_languages() {
    let mut p = Preferences::default();
    p.set_uses_system_languages(false, &s(&["nl", "en"]));
    assert_eq!(p.languages, Some(s(&["nl", "en"])));
    p.set_uses_system_languages(true, &[]);
    p.set_uses_system_languages(true, &[]); // on twice must not wipe the saved list
    assert_eq!(p.saved_languages, Some(s(&["nl", "en"])));
}

#[test]
fn saved_languages_round_trip_and_stay_out_of_old_files() {
    let env = Env::new();
    assert!(!muesli_core::json::to_pretty_sorted(&Preferences::default()).contains("savedLanguages"));
    let p = Preferences { saved_languages: Some(s(&["fr"])), ..Default::default() };
    p.save(&env.paths).unwrap();
    assert_eq!(Preferences::load(&env.paths).saved_languages, Some(s(&["fr"])));
}

#[test]
fn claude_counts_as_missing_when_the_app_is_not_installed() {
    let env = Env::new();
    let claude = env.dir.path().join("Claude");
    std::fs::create_dir_all(&claude).unwrap(); // leftover data folder
    assert_eq!(ClaudeDesktopExtension::state_with("1.0.0", &claude, Some(false)), ExtensionState::ClaudeMissing);
    assert_eq!(ClaudeDesktopExtension::state_with("1.0.0", &claude, Some(true)), ExtensionState::NotInstalled);
    assert_eq!(ClaudeDesktopExtension::state_with("1.0.0", &claude, None), ExtensionState::NotInstalled);
    assert_eq!(ClaudeDesktopExtension::state_with("1.0.0", &env.dir.path().join("none"), Some(true)), ExtensionState::NotInstalled);
}
