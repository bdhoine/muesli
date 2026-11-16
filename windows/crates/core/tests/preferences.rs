#![allow(clippy::field_reassign_with_default)] // tests read better one field at a time

mod common;

use common::Env;
use muesli_core::languages;
use muesli_core::status::{AppStatus, LastReady};
use muesli_core::{Preferences, WhisperModel};

#[test]
fn defaults_are_safe() {
    let p = Preferences::default();
    assert!(!p.mcp_enabled);
    assert!(!p.claude_desktop_config_entry);
    assert!(!p.auto_stop);
    assert_eq!(p.end_delay_seconds, 60);
    assert_eq!(p.recent_limit, 5);
    assert_eq!(p.model, WhisperModel::Small);
    assert!(p.languages.is_none());
    assert!(!p.onboarding_done);
}

#[test]
fn corrupt_or_missing_preferences_fall_back_to_defaults() {
    let env = Env::new();
    assert_eq!(Preferences::load(&env.paths), Preferences::default());
    std::fs::create_dir_all(&env.paths.support).unwrap();
    std::fs::write(env.paths.config(), "{").unwrap();
    assert_eq!(Preferences::load(&env.paths), Preferences::default());
    assert!(!Preferences::load(&env.paths).onboarding_done);
}

#[test]
fn config_from_before_onboarding_counts_as_done() {
    let p: Preferences = serde_json::from_str(r#"{"autoStop":true}"#).unwrap();
    assert!(p.auto_stop);
    assert!(p.onboarding_done);
    assert_eq!(p.model, WhisperModel::Small);
    assert!(p.custom_model_path.is_none());
}

#[test]
fn unknown_model_falls_back_to_small() {
    let p: Preferences = serde_json::from_str(r#"{"model":"huge"}"#).unwrap();
    assert_eq!(p.model, WhisperModel::Small);
}

#[test]
fn preferences_round_trip() {
    let env = Env::new();
    let mut p = Preferences::default();
    p.model = WhisperModel::Medium;
    p.custom_model_path = Some("/x/y.bin".into());
    p.languages = Some(vec!["fr".into()]);
    p.onboarding_done = true;
    p.save(&env.paths).unwrap();
    assert_eq!(Preferences::load(&env.paths), p);
}

#[test]
fn model_path_prefers_the_custom_file() {
    let env = Env::new();
    let mut p = Preferences::default();
    p.model = WhisperModel::Large;
    assert_eq!(p.model_path(&env.paths), env.paths.support.join("ggml-large-v3.bin"));
    p.custom_model_path = Some("/tmp/my-model.bin".into());
    assert_eq!(p.model_path(&env.paths).to_str(), Some("/tmp/my-model.bin"));
    p.custom_model_path = Some(String::new());
    assert_eq!(p.model_path(&env.paths), WhisperModel::Large.path(&env.paths));
}

#[test]
fn model_files_and_urls() {
    assert_eq!(WhisperModel::Medium.file_name(), "ggml-medium.bin");
    assert_eq!(WhisperModel::Large.file_name(), "ggml-large-v3.bin");
    for m in WhisperModel::ALL {
        assert!(m.download_url().ends_with(m.file_name()));
        assert!(m.minimum_valid_bytes() < m.approximate_bytes());
    }
}

#[test]
fn default_languages_are_os_language_plus_english() {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    assert_eq!(languages::defaults(&s(&["nl-BE", "en"])), s(&["nl", "en"]));
    assert_eq!(languages::defaults(&s(&["en-BE"])), s(&["en"]));
    assert_eq!(languages::defaults(&s(&["xx-YY"])), s(&["en"]));
    assert_eq!(languages::defaults(&s(&["iw-IL"])), s(&["he", "en"]));
}

#[test]
fn best_language_ignores_languages_outside_the_set() {
    let probs = [("nl".to_string(), 0.6f32), ("en".to_string(), 0.3), ("fr".to_string(), 0.9)].into();
    let allowed = vec!["nl".to_string(), "en".to_string()];
    assert_eq!(languages::best(&probs, &allowed).as_deref(), Some("nl"));
}

#[test]
fn chosen_languages_are_validated() {
    let mut p = Preferences::default();
    let fallback = vec!["nl".to_string(), "en".to_string()];
    assert_eq!(p.effective_languages(&fallback), fallback);
    p.languages = Some(vec!["FR-be".into(), "xx".into(), "fr".into(), "de".into()]);
    assert_eq!(p.effective_languages(&fallback), ["fr", "de"]);
    p.languages = Some(vec!["xx".into()]);
    assert_eq!(p.effective_languages(&fallback), fallback);
}

#[test]
fn recent_limit_is_clamped() {
    let mut p = Preferences::default();
    p.recent_limit = 99;
    assert_eq!(p.effective_recent_limit(), 10);
    p.recent_limit = -3;
    assert_eq!(p.effective_recent_limit(), 0);
}

#[test]
fn status_file_round_trip() {
    let env = Env::new();
    assert!(AppStatus::read(&env.paths).is_none());
    let mut s = AppStatus::new("error");
    s.error = Some("Whisper model missing".into());
    s.last_ready = Some(LastReady { id: "m".into(), title: "t".into(), ended_at: None });
    s.write(&env.paths).unwrap();
    let back = AppStatus::read(&env.paths).unwrap();
    assert_eq!(back.schema_version, 1);
    assert_eq!(back.app_state, "error");
    assert_eq!(back.error.as_deref(), Some("Whisper model missing"));
    assert_eq!(back.last_ready.unwrap().id, "m");
    assert_eq!(back.pid, std::process::id());
    assert!(back.current.is_none());
}

#[test]
fn heartbeat_freshness_window() {
    let now = chrono::Utc::now();
    let mut s = AppStatus::new("idle");
    s.updated_at = now - chrono::Duration::seconds(29);
    assert!(s.is_fresh(now));
    s.updated_at = now - chrono::Duration::seconds(30);
    assert!(!s.is_fresh(now));
}

#[test]
fn corrupt_status_file_reads_as_not_running() {
    let env = Env::new();
    std::fs::create_dir_all(&env.paths.support).unwrap();
    std::fs::write(env.paths.state(), "{").unwrap();
    assert!(AppStatus::read(&env.paths).is_none());
}

#[test]
fn activity_maps_to_state_json() {
    use muesli_core::status::Activity;
    let s = |a: Activity| { let x = AppStatus::for_activity(&a); (x.app_state, x.error) };
    assert_eq!(s(Activity::Idle { preparing: false }), ("idle".into(), None));
    assert_eq!(s(Activity::Idle { preparing: true }), ("preparing".into(), None));
    assert_eq!(s(Activity::Recording), ("recording".into(), None));
    assert_eq!(s(Activity::Finalizing), ("finalizing".into(), None));
    assert_eq!(s(Activity::Error("x".into())), ("error".into(), Some("x".into())));
    assert_eq!(s(Activity::PermissionMissing("m".into())), ("permissionMissing".into(), Some("m".into())));
}

#[test]
fn model_is_installed_only_when_large_enough() {
    let env = Env::new();
    assert!(!WhisperModel::Small.is_installed(&env.paths));
    std::fs::create_dir_all(&env.paths.models).unwrap();
    std::fs::write(WhisperModel::Small.path(&env.paths), b"tiny").unwrap();
    assert!(!WhisperModel::Small.is_installed(&env.paths));
    assert_eq!("ready".parse::<muesli_core::MeetingState>(), Ok(muesli_core::MeetingState::Ready));
    assert!("bogus".parse::<muesli_core::MeetingState>().is_err());
    let id = muesli_core::models::new_id();
    assert_eq!(id.len(), 36);
    assert!(id.chars().all(|c| c == '-' || c.is_ascii_digit() || c.is_ascii_uppercase()));
}

#[test]
fn a_chosen_transcripts_folder_replaces_the_default() {
    let env = Env::new();
    let mut p = Preferences::default();
    assert_eq!(env.paths.clone().with_preferences(&p).transcripts, env.paths.transcripts);
    p.transcripts_folder = Some("/somewhere/else".into());
    let chosen = env.paths.clone().with_preferences(&p);
    assert_eq!(chosen.transcripts.to_str(), Some("/somewhere/else"));
    assert_eq!(chosen.support, env.paths.support); // nothing else moves
    p.transcripts_folder = Some("  ".into());
    assert_eq!(env.paths.clone().with_preferences(&p).transcripts, env.paths.transcripts);

    // Stored in config.json only when set, so files from the macOS app stay unchanged.
    assert!(!muesli_core::json::to_pretty_sorted(&Preferences::default()).contains("transcriptsFolder"));
    p.transcripts_folder = Some("/x".into());
    p.save(&env.paths).unwrap();
    assert_eq!(Preferences::load(&env.paths).transcripts_folder.as_deref(), Some("/x"));
}
