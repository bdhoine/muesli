//! Every contract against the fakes. Runs on every OS.

use muesli_core::ports::{MicCapture, SystemAudioCapture};
use muesli_platform_fake::{
    FakeAudio, FakeLoginItem, FakeNotifier, FakePaths, FakePermissions, FakePlatform, FakeServer, FakeTray, FakeUpdater, FolderTrash,
    ManualClock, ScriptedDetector,
};
use muesli_port_contracts as contract;

#[test]
fn clock() {
    let clock = ManualClock::new();
    contract::clock(&clock);
    clock.advance(chrono::Duration::seconds(60));
    contract::clock(&clock);
}

#[test]
fn paths() {
    let dir = tempfile::tempdir().unwrap();
    contract::platform_paths(&FakePaths::new(dir.path()));
}

#[test]
fn trash() {
    let dir = tempfile::tempdir().unwrap();
    let scratch = dir.path().join("files");
    std::fs::create_dir_all(&scratch).unwrap();
    let trash = FolderTrash::new(dir.path().join("Trash"), "Recycle Bin");
    contract::trash(&trash, &scratch);
    assert_eq!(trash.trashed().len(), 1);
}

#[test]
fn login_item() {
    contract::login_item(&FakeLoginItem::default());
}

#[test]
fn permissions() {
    contract::permissions(&FakePermissions::new());
    contract::permissions_request_decides(&FakePermissions::new());
    contract::permissions(&FakePermissions::all_allowed());
}

#[test]
fn notifier() {
    let notifier = FakeNotifier::default();
    contract::notifier(&notifier);
    contract::notifier_answers_once(&notifier, &|a| notifier.answer(a));
    notifier.set_enabled(false);
    contract::notifier(&notifier);
}

#[test]
fn tray() {
    let tray = FakeTray::default();
    contract::tray(&tray);
    assert_eq!(tray.draws(), 4);
}

#[test]
fn meeting_detector() {
    let detector = ScriptedDetector::default();
    contract::meeting_detector(&detector);
    detector.set_mic_busy(Some(true));
    contract::meeting_detector(&detector);
}

#[test]
fn microphone_and_system_audio() {
    let mic = FakeAudio::new();
    contract::audio_source(&|s, e| MicCapture::start(&mic, s, e), &|| mic.feed(&[0.1; 160]), false);
    let system = FakeAudio::new();
    contract::audio_source(&|s, e| SystemAudioCapture::start(&system, s, e), &|| system.feed(&[0.1; 160]), false);
}

#[test]
fn the_bundle_holds_one_of_each() {
    let dir = tempfile::tempdir().unwrap();
    let fakes = FakePlatform::new(dir.path());
    let p = fakes.platform();
    contract::clock(&*p.clock);
    contract::tray(&*p.tray);
    assert_eq!(p.words.trash, p.trash.name());
}

#[test]
fn updater() {
    use std::sync::atomic::Ordering;
    let updater = FakeUpdater::new("0.1.0");
    contract::updater(&updater); // no release
    for server in [
        FakeServer::Release("0.2.0".into()),
        FakeServer::Release("0.1.0".into()),
        FakeServer::Release("0.0.9".into()),
        FakeServer::Release("0.3.0-beta.1".into()),
        FakeServer::NetworkError("Could not connect to github.com (timed out after 15 s).".into()),
    ] {
        updater.set_server(server);
        contract::updater(&updater);
    }
    updater.publish("0.2.0");
    contract::updater_installs_only_verified_downloads(&updater, &|bad| updater.bad_signature.store(bad, Ordering::SeqCst));
    assert_eq!(updater.installed_on_quit(), 1);
    assert_eq!(updater.installed_now(), 0);
}
