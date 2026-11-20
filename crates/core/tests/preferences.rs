#![allow(clippy::field_reassign_with_default)] // tests read better one field at a time

mod common;

use common::Env;
use muesli_core::languages;
use muesli_core::preferences::REMOVED_KEYS;
use muesli_core::status::{AppStatus, LastReady};
use muesli_core::whisper_model::{legacy_models, remove_legacy_models, DEFAULT_MODEL, LEGACY_FILES, VAD_MODEL};
use muesli_core::Preferences;

#[test]
fn defaults_are_safe() {
    let p = Preferences::default();
    assert!(!p.mcp_enabled);
    assert!(!p.claude_desktop_config_entry);
    assert!(p.languages.is_none());
    assert!(!p.onboarding_done);
    assert!(p.auto_update_check, "checking for updates is on by default");
    assert!(p.update_last_checked.is_none() && p.update_notified.is_none() && p.last_run_version.is_none());
}

#[test]
fn a_config_from_before_updates_checks_for_updates() {
    let p: Preferences = serde_json::from_str(r#"{"mcpEnabled":true,"onboardingDone":true}"#).unwrap();
    assert!(p.auto_update_check);
    assert!(p.last_run_version.is_none());
    let p: Preferences = serde_json::from_str(r#"{"autoUpdateCheck":false,"updateLastChecked":"2027-01-15T09:00:00Z"}"#).unwrap();
    assert!(!p.auto_update_check);
    assert_eq!(p.update_last_checked.unwrap().to_rfc3339(), "2027-01-15T09:00:00+00:00");
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
    assert!(p.onboarding_done);
}

#[test]
fn old_keys_are_dropped_silently_and_the_rest_is_kept() {
    let env = Env::new();
    std::fs::create_dir_all(&env.paths.support).unwrap();
    let old = r#"{"autoStop":true,"endDelaySeconds":120,"recentLimit":3,"customModelPath":"/m/x.bin","savedLanguages":["fr"],
        "dropFolder":"/Users/me/Audio","dropFolderCleanup":"trash","model":"medium",
        "mcpEnabled":true,"claudeDesktopConfigEntry":true,"languages":["nl","en"],"onboardingDone":true,"transcriptsFolder":"/t"}"#;
    std::fs::write(env.paths.config(), old).unwrap();
    let (p, dropped) = Preferences::migrate(&env.paths);
    assert_eq!(dropped.len(), REMOVED_KEYS.len());
    assert!(p.mcp_enabled && p.claude_desktop_config_entry && p.onboarding_done);
    assert_eq!(p.languages, Some(vec!["nl".to_string(), "en".to_string()]));
    assert_eq!(p.transcripts_folder.as_deref(), Some("/t"));
    let written = std::fs::read_to_string(env.paths.config()).unwrap();
    for key in REMOVED_KEYS {
        assert!(!written.contains(&format!("\"{key}\"")), "{key} is still in config.json");
    }
    assert_eq!(Preferences::load(&env.paths), p);
    // A second run finds nothing to do and leaves the file alone.
    assert!(Preferences::migrate(&env.paths).1.is_empty());
}

#[test]
fn a_current_config_is_not_rewritten_by_the_migration() {
    let env = Env::new();
    std::fs::create_dir_all(&env.paths.support).unwrap();
    std::fs::write(env.paths.config(), r#"{"mcpEnabled":true}"#).unwrap();
    let (p, dropped) = Preferences::migrate(&env.paths);
    assert!(dropped.is_empty() && p.mcp_enabled);
    assert_eq!(std::fs::read_to_string(env.paths.config()).unwrap(), r#"{"mcpEnabled":true}"#);
}

#[test]
fn preferences_round_trip() {
    let env = Env::new();
    let mut p = Preferences::default();
    p.languages = Some(vec!["fr".into()]);
    p.onboarding_done = true;
    p.save(&env.paths).unwrap();
    assert_eq!(Preferences::load(&env.paths), p);
}

#[test]
fn there_is_one_model_and_one_voice_model() {
    let env = Env::new();
    let p = Preferences::default();
    assert_eq!(p.model_path(&env.paths), env.paths.support.join("ggml-large-v3-turbo-q5_0.bin"));
    assert_eq!(p.vad_model_path(&env.paths), env.paths.support.join("ggml-silero-v5.1.2.bin"));
}

#[test]
fn languages_are_added_removed_and_reset() {
    let defaults = vec!["nl".to_string(), "en".to_string()];
    let mut p = Preferences::default();
    p.add_language("fr-BE", &defaults);
    assert_eq!(p.languages, Some(vec!["nl".to_string(), "en".to_string(), "fr".to_string()]));
    p.add_language("fr", &defaults); // no duplicates
    p.add_language("xx", &defaults); // not supported
    assert_eq!(p.effective_languages(&defaults).len(), 3);
    p.remove_language("nl", &defaults);
    p.remove_language("en", &defaults);
    p.remove_language("fr", &defaults); // the last one stays
    assert_eq!(p.effective_languages(&defaults), vec!["fr".to_string()]);
    p.reset_languages();
    assert_eq!(p.effective_languages(&defaults), defaults);
}

#[test]
fn model_files_and_urls() {
    for m in [DEFAULT_MODEL, VAD_MODEL] {
        assert!(m.url.ends_with(m.file_name));
        assert!(m.minimum_valid_bytes() < m.approximate_bytes);
    }
    assert_eq!(DEFAULT_MODEL.approximate_bytes, 574_041_195);
}

#[test]
fn old_model_files_are_found_and_removed_on_request() {
    let env = Env::new();
    std::fs::create_dir_all(&env.paths.models).unwrap();
    assert_eq!(legacy_models(&env.paths), (vec![], 0));
    std::fs::write(env.paths.models.join(LEGACY_FILES[0]), vec![0u8; 10]).unwrap();
    std::fs::write(env.paths.models.join(LEGACY_FILES[2]), vec![0u8; 5]).unwrap();
    std::fs::write(env.paths.models.join("ggml-custom.bin"), b"mine").unwrap(); // not ours to delete
    assert_eq!(legacy_models(&env.paths).1, 15);
    assert_eq!(remove_legacy_models(&env.paths), 2);
    assert!(env.paths.models.join("ggml-custom.bin").exists());
    assert_eq!(legacy_models(&env.paths), (vec![], 0));
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
    let s = |a: Activity| {
        let x = AppStatus::for_activity(&a);
        (x.app_state, x.error)
    };
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
    assert!(!DEFAULT_MODEL.is_installed(&env.paths));
    std::fs::create_dir_all(&env.paths.models).unwrap();
    std::fs::write(DEFAULT_MODEL.path(&env.paths), b"tiny").unwrap();
    assert!(!DEFAULT_MODEL.is_installed(&env.paths));
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

mod platform_paths {
    use muesli_core::Paths;
    use muesli_platform_fake::FakePaths;

    #[test]
    fn folders_come_from_the_platform() {
        let dir = tempfile::tempdir().unwrap();
        let platform = FakePaths::new(dir.path());
        let paths = Paths::from_platform(&platform);
        assert_eq!(paths.support, dir.path().join("data/Muesli"));
        assert_eq!(paths.models, dir.path().join("local/Muesli"));
        assert_eq!(paths.transcripts, dir.path().join("Documents/MeetingTranscripts"));
        assert!(!paths.transcripts_set_by_environment());
    }

    #[test]
    fn the_environment_overrides_them() {
        let dir = tempfile::tempdir().unwrap();
        let platform = FakePaths::new(dir.path());
        platform.set_env("MUESLI_SUPPORT_DIR", Some("~/support"));
        platform.set_env("TRANSCRIPT_DIR", Some("/elsewhere/t"));
        let paths = Paths::from_platform(&platform);
        assert_eq!(paths.support, dir.path().join("support"));
        assert_eq!(paths.models, paths.support, "an overridden support folder keeps the models too");
        assert_eq!(paths.transcripts, std::path::PathBuf::from("/elsewhere/t"));
        assert!(paths.transcripts_set_by_environment());
        let mut prefs = muesli_core::Preferences::default();
        prefs.transcripts_folder = Some("/chosen".into());
        assert_eq!(paths.with_preferences(&prefs).transcripts, std::path::PathBuf::from("/elsewhere/t"));
    }
}
