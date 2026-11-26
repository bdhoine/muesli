//! Muesli's own updates (UP1 to UP8): when it checks, what it shows when an update is ready, and
//! when it keeps quiet; Restart, Quit, and the first start of a new version.

mod common;

use std::sync::atomic::Ordering;
use std::sync::Arc;

use common::{wait_for, Harness};
use muesli_core::ports::{Access, Permission, PlatformWords, TrayItem};
use muesli_core::Preferences;
use muesli_engine::fake::{FakeSpeech, HostCall};
use muesli_engine::{Intent, Window, CHECK_EVERY, FIRST_CHECK_AFTER};
use muesli_platform_fake::{FakePlatform, FakeServer, FakeUpdater};

const RUNNING: &str = env!("CARGO_PKG_VERSION");

/// Muesli after setup, with `prefs` in its config.json, updating through a fake GitHub.
fn muesli(words: PlatformWords, prefs: Preferences) -> Harness {
    let dir = tempfile::tempdir().unwrap();
    let fakes = FakePlatform::with_words(dir.path(), words);
    let paths = muesli_core::Paths::from_platform(&*fakes.paths);
    std::fs::create_dir_all(&paths.support).unwrap();
    Preferences { onboarding_done: true, ..prefs }.save(&paths).unwrap();
    Harness::with_updater(dir, fakes, FakeSpeech::ready(), Arc::new(FakeUpdater::new(RUNNING)))
}

fn mac() -> Harness {
    muesli(PlatformWords::MAC, Preferences::default())
}

/// Runs the first check (30 s after start) and waits for its answer.
fn first_check(h: &Harness) {
    let before = h.updater.checks();
    h.advance(FIRST_CHECK_AFTER);
    wait_for("a check", || h.updater.checks() > before);
    wait_for("its answer", || {
        let u = h.engine.settings_view().updates;
        u.can_check || u.restart
    });
}

fn first_item(h: &Harness) -> Option<String> {
    match h.engine.menu().items.first() {
        Some(TrayItem::Action { label, .. }) => Some(label.clone()),
        _ => None,
    }
}

fn notices(h: &Harness) -> Vec<String> {
    h.fakes.notifier.notices().into_iter().map(|n| n.title).collect()
}

#[test]
fn the_first_check_waits_30_seconds_and_then_comes_every_24_hours() {
    let h = mac();
    h.advance(FIRST_CHECK_AFTER - 1);
    assert_eq!(h.updater.checks(), 0, "starting stays quick");
    first_check(&h);
    assert_eq!(h.updater.checks(), 1);
    let checked = Preferences::load(&h.engine.paths()).update_last_checked.map(|d| d.to_rfc3339());
    assert_eq!(checked.as_deref(), Some("2027-01-15T09:00:30+00:00"), "updateLastChecked is saved, 30 s after start");
    assert_eq!(h.engine.settings_view().updates.last_checked.as_deref(), Some("2027-01-15T09:00:30Z"));

    // Up to date, by itself: nothing to see, not even in Settings.
    let view = h.engine.settings_view().updates;
    assert_eq!(view.status, None);
    assert_eq!(view.title, format!("Muesli {RUNNING}"));
    assert!(view.auto_check && view.available);
    assert_eq!(first_item(&h).as_deref(), Some("Start Transcribing"));

    // The clock is now 29 s past that check.
    h.fakes.clock.advance(chrono::Duration::seconds(CHECK_EVERY - 31));
    h.engine.tick();
    assert_eq!(h.updater.checks(), 1, "not before 24 hours");
    h.advance(5);
    wait_for("the daily check", || h.updater.checks() == 2);
}

#[test]
fn a_ready_update_is_offered_in_the_menu_the_icon_the_window_and_one_notification() {
    let h = mac();
    h.updater.publish("9.0.0");
    first_check(&h);
    wait_for("downloaded", || h.engine.settings_view().updates.restart);

    assert_eq!(first_item(&h).as_deref(), Some("Restart to Update to 9.0.0"));
    let menu = h.engine.menu();
    assert!(menu.update_ready && !menu.transcribing, "the Pumpkin seed dot");
    assert_eq!(h.engine.main_view().update.unwrap().text, "Muesli 9.0.0 is ready to install");
    let settings = h.engine.settings_view().updates;
    assert_eq!(settings.status.as_deref(), Some("9.0.0 is ready"));
    assert!(settings.can_restart);

    let notice = h.fakes.notifier.pending_notice().expect("a notification");
    assert_eq!(notice.title, "Muesli 9.0.0 is ready to install");
    assert_eq!(notice.action.as_deref(), Some("Restart"));
    h.advance(5);
    assert_eq!(notices(&h).len(), 1, "one notification per version");
    assert_eq!(Preferences::load(&h.engine.paths()).update_notified.as_deref(), Some("9.0.0"));

    // Not now hides the banner; the menu, the dot and Settings stay.
    h.engine.intent(Intent::DismissUpdate);
    assert!(h.engine.main_view().update.is_none());
    assert_eq!(first_item(&h).as_deref(), Some("Restart to Update to 9.0.0"));
    assert!(h.engine.menu().update_ready);

    // What's new opens the release notes.
    h.engine.intent(Intent::OpenReleaseNotes { version: "9.0.0".into() });
    assert!(h.host.calls().contains(&HostCall::OpenUrl("https://github.com/bdhoine/muesli/releases/tag/v9.0.0".into())));

    // The notification's Restart installs and starts the new version.
    assert!(h.fakes.notifier.click_action());
    assert_eq!(h.updater.installed_now(), 1);
    assert_eq!(h.updater.installed_on_quit(), 0);
}

#[test]
fn the_restart_item_in_the_menu_installs() {
    let h = mac();
    h.updater.publish("9.0.0");
    first_check(&h);
    wait_for("downloaded", || h.engine.settings_view().updates.restart);
    h.engine.intent(Intent::Tray { id: "update".into() });
    assert_eq!(h.updater.installed_now(), 1);
}

#[test]
fn quitting_with_an_update_ready_installs_it_without_starting_again() {
    let h = mac();
    h.updater.publish("9.0.0");
    first_check(&h);
    wait_for("downloaded", || h.engine.settings_view().updates.restart);
    h.engine.intent(Intent::Quit);
    assert_eq!(h.updater.installed_on_quit(), 1);
    assert_eq!(h.updater.installed_now(), 0);
    assert_eq!(h.host.calls().last(), Some(&HostCall::Quit));
}

#[test]
fn quitting_without_an_update_installs_nothing() {
    let h = mac();
    first_check(&h);
    h.engine.intent(Intent::Quit);
    assert_eq!(h.updater.installed_on_quit(), 0);
}

#[test]
fn while_transcribing_every_prompt_waits_until_stop_and_save() {
    let h = mac();
    h.updater.publish("9.0.0");
    h.engine.intent(Intent::StartTranscribing);
    wait_for("transcribing", || h.transcribing());
    first_check(&h);
    wait_for("downloaded while transcribing", || h.engine.settings_view().updates.restart);

    assert_ne!(first_item(&h).as_deref(), Some("Restart to Update to 9.0.0"));
    let menu = h.engine.menu();
    assert!(menu.transcribing && !menu.update_ready, "the red dot wins");
    assert!(h.engine.main_view().update.is_none());
    assert!(notices(&h).is_empty());
    let settings = h.engine.settings_view().updates;
    assert!(settings.restart && !settings.can_restart, "Restart is not available");
    h.engine.intent(Intent::RestartToUpdate);
    assert_eq!(h.updater.installed_now(), 0, "nothing installs while transcribing");

    h.engine.intent(Intent::StopAndSave);
    wait_for("saved", || h.idle());
    h.advance(1);
    assert_eq!(first_item(&h).as_deref(), Some("Restart to Update to 9.0.0"));
    assert!(h.engine.main_view().update.is_some());
    assert_eq!(notices(&h), vec!["Muesli 9.0.0 is ready to install".to_string()]);
}

#[test]
fn while_the_speech_model_downloads_prompts_wait() {
    let dir = tempfile::tempdir().unwrap();
    let fakes = FakePlatform::new(dir.path());
    let paths = muesli_core::Paths::from_platform(&*fakes.paths);
    std::fs::create_dir_all(&paths.support).unwrap();
    Preferences { onboarding_done: true, ..Default::default() }.save(&paths).unwrap();
    let updater = Arc::new(FakeUpdater::new(RUNNING));
    updater.publish("9.0.0");
    let h = Harness::with_updater(dir, fakes, FakeSpeech::missing(1000), updater);
    first_check(&h);
    wait_for("downloaded", || h.engine.settings_view().updates.restart);
    assert!(h.engine.main_view().update.is_none());
    assert!(!h.engine.menu().update_ready);
    assert!(notices(&h).is_empty());
    h.speech.finish();
    h.advance(1);
    assert!(h.engine.main_view().update.is_some());
    assert_eq!(notices(&h).len(), 1);
}

#[test]
fn an_update_is_offered_once_queued_audio_files_are_done() {
    let h = mac();
    h.updater.publish("9.0.0");
    first_check(&h);
    wait_for("downloaded", || h.engine.settings_view().updates.restart);
    h.fakes.notifier.dismiss_notice();
    // Transcribing waits for the meeting, so the file stays in the list.
    h.engine.intent(Intent::StartTranscribing);
    wait_for("transcribing", || h.transcribing());
    let file = h.dir.path().join("memo.wav");
    std::fs::copy(concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/audio/tone-stereo-48k.wav"), &file).unwrap();
    h.engine.intent(Intent::TranscribeFiles { paths: vec![file] });
    h.engine.intent(Intent::StopAndSave);
    wait_for("the file is done", || h.idle() && h.engine.main_view().imports.is_empty());
    assert!(h.engine.main_view().update.is_some(), "offered once the file is done");
}

#[test]
fn a_download_that_does_not_match_the_signature_is_discarded() {
    let h = mac();
    h.updater.publish("9.0.0");
    h.updater.bad_signature.store(true, Ordering::SeqCst);
    first_check(&h);
    assert!(!h.engine.settings_view().updates.restart);
    assert!(h.engine.main_view().update.is_none());
    assert!(notices(&h).is_empty());
    assert_eq!(h.engine.settings_view().updates.status, None, "a check by itself fails silently");
    h.engine.intent(Intent::RestartToUpdate);
    assert_eq!(h.updater.installed_now(), 0);

    // From Settings the reason shows.
    h.engine.intent(Intent::CheckForUpdates);
    wait_for("the answer", || h.engine.settings_view().updates.status.is_some());
    let view = h.engine.settings_view().updates;
    assert_eq!(
        view.status.as_deref(),
        Some("Couldn't check for updates: The download did not match Muesli's signature, so it was discarded.")
    );
    assert_eq!(view.check_label, "Try again");
}

#[test]
fn a_failed_check_by_itself_is_silent_and_tried_again_next_time() {
    let h = mac();
    h.updater.set_server(FakeServer::NetworkError("Could not connect to github.com".into()));
    first_check(&h);
    assert_eq!(h.engine.settings_view().updates.status, None);
    assert!(notices(&h).is_empty());
    assert_eq!(Preferences::load(&h.engine.paths()).update_last_checked, None, "never checked successfully");
    h.updater.publish("9.0.0");
    h.fakes.clock.advance(chrono::Duration::seconds(CHECK_EVERY));
    h.engine.tick();
    wait_for("downloaded", || h.engine.settings_view().updates.restart);
}

#[test]
fn checks_from_settings_answer_inline() {
    let h = mac();
    h.engine.intent(Intent::CheckForUpdates);
    wait_for("up to date", || h.engine.settings_view().updates.status.as_deref() == Some("Muesli is up to date"));
    assert!(h.engine.settings_view().updates.last_checked.is_some());

    h.updater.set_server(FakeServer::NetworkError("Could not connect to github.com (timed out after 15 s)".into()));
    h.engine.intent(Intent::CheckForUpdates);
    wait_for("an error", || h.engine.settings_view().updates.tone == "warn");
    let view = h.engine.settings_view().updates;
    assert_eq!(view.status.as_deref(), Some("Couldn't check for updates: Could not connect to github.com (timed out after 15 s)."));
    assert_eq!(view.check_label, "Try again");

    // Downloading, step by step.
    h.updater.hold_downloads.store(true, Ordering::SeqCst);
    h.updater.publish("9.0.0");
    h.engine.intent(Intent::CheckForUpdates);
    wait_for("downloading", || h.updater.downloading());
    h.updater.progress(42, 100);
    let view = h.engine.settings_view().updates;
    assert_eq!(view.status.as_deref(), Some("Downloading 9.0.0… 42%"));
    assert_eq!(view.progress, Some(0.42));
    assert!(!view.can_check);
    h.updater.finish_download();
    wait_for("ready", || h.engine.settings_view().updates.status.as_deref() == Some("9.0.0 is ready"));
    h.engine.intent(Intent::RestartToUpdate);
    assert_eq!(h.updater.installed_now(), 1);
}

#[test]
fn never_a_downgrade_or_a_pre_release() {
    let h = mac();
    for version in ["0.0.0", RUNNING, "99.0.0-beta.1"] {
        h.updater.publish(version);
        h.engine.intent(Intent::CheckForUpdates);
        wait_for("up to date", || h.engine.settings_view().updates.status.as_deref() == Some("Muesli is up to date"));
    }
    assert!(h.engine.main_view().update.is_none());
}

#[test]
fn the_switch_stops_checks_by_itself_but_not_from_settings() {
    let h = mac();
    h.engine.intent(Intent::SetAutoUpdateCheck { on: false });
    assert!(!Preferences::load(&h.engine.paths()).auto_update_check);
    assert!(!h.engine.settings_view().updates.auto_check);
    h.advance(FIRST_CHECK_AFTER + 5);
    h.fakes.clock.advance(chrono::Duration::seconds(CHECK_EVERY * 3));
    h.engine.tick();
    assert_eq!(h.updater.checks(), 0);
    h.engine.intent(Intent::CheckForUpdates);
    wait_for("a check", || h.updater.checks() == 1);
}

#[test]
fn a_build_without_updates_never_checks() {
    let h = Harness::ready();
    h.advance(FIRST_CHECK_AFTER + 5);
    h.engine.intent(Intent::CheckForUpdates);
    let view = h.engine.settings_view().updates;
    assert!(!view.available);
    assert_eq!(view.title, format!("Muesli {RUNNING}"));
    assert_eq!(view.status, None);
}

#[test]
fn the_first_start_of_a_new_version_says_so_once() {
    let h = muesli(PlatformWords::MAC, Preferences { last_run_version: Some("0.0.0".into()), ..Default::default() });
    let notice = h.fakes.notifier.pending_notice().expect("a notification");
    assert_eq!(notice.title, format!("Muesli updated to {RUNNING}"));
    assert_eq!(notice.action.as_deref(), Some("What's new"));
    assert!(h.fakes.notifier.click_action());
    assert!(h.host.calls().contains(&HostCall::OpenUrl(format!("https://github.com/bdhoine/muesli/releases/tag/v{RUNNING}"))));
    assert_eq!(Preferences::load(&h.engine.paths()).last_run_version.as_deref(), Some(RUNNING));
    assert!(!h.host.is_open(Window::Setup), "permissions are still allowed");

    // A first start, or a version from before updates: nothing to say.
    let h = mac();
    assert!(notices(&h).is_empty());
    assert_eq!(Preferences::load(&h.engine.paths()).last_run_version.as_deref(), Some(RUNNING));
    // The same version again: nothing either.
    let h = muesli(PlatformWords::MAC, Preferences { last_run_version: Some(RUNNING.into()), ..Default::default() });
    assert!(notices(&h).is_empty());
}

#[test]
fn macos_asks_for_permissions_again_after_an_update() {
    let dir = tempfile::tempdir().unwrap();
    let fakes = FakePlatform::new(dir.path());
    fakes.permissions.set(Permission::Microphone, Access::NotAsked);
    fakes.permissions.set(Permission::SystemAudio, Access::NotAsked);
    let paths = muesli_core::Paths::from_platform(&*fakes.paths);
    std::fs::create_dir_all(&paths.support).unwrap();
    Preferences { onboarding_done: true, last_run_version: Some("0.0.0".into()), ..Default::default() }.save(&paths).unwrap();
    let h = Harness::with_updater(dir, fakes, FakeSpeech::ready(), Arc::new(FakeUpdater::new(RUNNING)));

    assert!(h.host.is_open(Window::Setup));
    let setup = h.engine.setup_view();
    assert_eq!(setup.step, 2, "the permissions screen");
    assert_eq!(setup.subtitle.as_deref(), Some("Muesli was updated, and macOS asks for these permissions again."));
    assert!(!setup.can_go_back && setup.finish && setup.counter.is_empty() && setup.model.is_none());
    assert!(!setup.primary_enabled, "waits for the required ones");
    h.engine.intent(Intent::RequestPermission { permission: Permission::Microphone });
    h.engine.intent(Intent::RequestPermission { permission: Permission::SystemAudio });
    wait_for("allowed", || h.engine.setup_view().primary_enabled);
    h.engine.intent(Intent::FinishSetup);
    assert!(!h.host.is_open(Window::Setup));
    assert!(Preferences::load(&h.engine.paths()).onboarding_done);
}

#[test]
fn windows_keeps_its_permissions_after_an_update() {
    let dir = tempfile::tempdir().unwrap();
    let fakes = FakePlatform::with_words(dir.path(), PlatformWords::WINDOWS);
    fakes.permissions.set(Permission::Microphone, Access::Denied);
    let paths = muesli_core::Paths::from_platform(&*fakes.paths);
    std::fs::create_dir_all(&paths.support).unwrap();
    Preferences { onboarding_done: true, last_run_version: Some("0.0.0".into()), ..Default::default() }.save(&paths).unwrap();
    let h = Harness::with_updater(dir, fakes, FakeSpeech::ready(), Arc::new(FakeUpdater::new(RUNNING)));
    assert_eq!(notices(&h), vec![format!("Muesli updated to {RUNNING}")]);
    assert!(!h.host.is_open(Window::Setup));
}

#[test]
fn setup_records_the_version_so_the_next_update_is_noticed() {
    let h = Harness::first_launch();
    assert_eq!(Preferences::load(&h.engine.paths()).last_run_version, None);
    h.engine.intent(Intent::FinishSetup);
    assert_eq!(Preferences::load(&h.engine.paths()).last_run_version.as_deref(), Some(RUNNING));
}
