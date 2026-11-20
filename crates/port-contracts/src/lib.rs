//! The contract of each port, as a test suite: what every adapter must do, whether it is a fake
//! or the real thing. `tests/fakes.rs` runs every suite against `platform-fake` on every OS;
//! `tests/native.rs` runs them against the adapters of the OS the tests run on.
//!
//! Each suite panics on the first broken rule, so it is called from a `#[test]`.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use chrono::{TimeZone, Utc};
use muesli_core::ports::{
    Access, AudioStream, Clock, LoginItem, MeetingDetector, Notice, Notifier, OnError, OnSamples, Permission, Permissions, PlatformPaths,
    Question, Trash, Tray, TrayItem, TrayMenu, UpdateError, Updater,
};
use muesli_core::Paths;

/// The clock never runs backwards and tells a plausible time.
pub fn clock(clock: &dyn Clock) {
    let a = clock.now();
    let b = clock.now();
    assert!(b >= a, "the clock ran backwards: {a} then {b}");
    assert!(a > Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap(), "implausible time {a}");
}

/// Folders are absolute and distinct where they must be; Muesli's folders derive from them.
pub fn platform_paths(paths: &dyn PlatformPaths) {
    for (what, dir) in [("data", paths.data_dir()), ("local data", paths.local_data_dir()), ("documents", paths.documents_dir())] {
        assert!(dir.is_absolute(), "{what} folder is not absolute: {}", dir.display());
    }
    assert!(paths.home_dir().is_absolute());
    assert_ne!(paths.data_dir(), paths.documents_dir(), "app data must not live in Documents");
    assert_eq!(paths.env("MUESLI_SURELY_NOT_SET_6F5B"), None);
    let muesli = Paths::from_platform(paths);
    if paths.env("MUESLI_SUPPORT_DIR").is_none() {
        assert_eq!(muesli.support, paths.data_dir().join("Muesli"));
        assert_eq!(muesli.models, paths.local_data_dir().join("Muesli"));
    }
    if paths.env("TRANSCRIPT_DIR").is_none() {
        assert_eq!(muesli.transcripts, paths.documents_dir().join("MeetingTranscripts"));
        assert!(!muesli.transcripts_set_by_environment());
    }
}

/// A trashed file is gone from its folder, a missing one is an error, and the trash has a name.
/// `scratch` is an empty folder the suite may write in.
pub fn trash(trash: &dyn Trash, scratch: &Path) {
    assert!(["Trash", "Recycle Bin"].contains(&trash.name()), "unexpected name {}", trash.name());
    let file = scratch.join("muesli contract test.md");
    std::fs::write(&file, "x").unwrap();
    trash.trash(&file).expect("trashing an existing file");
    assert!(!file.exists(), "the file is still there");
    assert!(trash.trash(&scratch.join("never existed.md")).is_err(), "a missing file must be an error");
}

/// What is set is what is read back. The original setting is restored.
pub fn login_item(item: &dyn LoginItem) {
    let before = item.is_enabled();
    for on in [!before, before] {
        item.set_enabled(on).expect("switching start at login");
        assert_eq!(item.is_enabled(), on);
    }
}

/// Every permission has an answer; one the platform does not have needs nothing and asking for it
/// changes nothing.
pub fn permissions(permissions: &dyn Permissions) {
    for p in Permission::ALL {
        let access = permissions.status(p);
        if access == Access::NotNeeded {
            permissions.request(p).expect("asking for a permission the platform does not have is a no-op");
            assert_eq!(permissions.status(p), Access::NotNeeded);
            assert!(access.is_granted());
        }
    }
    assert!(!Access::Denied.is_granted() && !Access::NotAsked.is_granted() && !Access::Unknown.is_granted());
}

/// Asking for a permission that was not asked yet leaves it decided (allowed or denied).
/// Only for adapters that can answer without a person: the fakes.
pub fn permissions_request_decides(permissions: &dyn Permissions) {
    for p in Permission::ALL {
        if permissions.status(p) == Access::NotAsked {
            permissions.request(p).unwrap();
            assert!(matches!(permissions.status(p), Access::Allowed | Access::Denied), "{p:?} still undecided");
        }
    }
}

/// Showing a question or notice either works or says why; it never panics or blocks.
pub fn notifier(notifier: &dyn Notifier) {
    let question = Question {
        title: "Looks like a meeting: Contract test".into(),
        body: "Detected via test. Start transcribing?".into(),
        accept: "Start transcribing".into(),
        decline: "Not now".into(),
    };
    if let Err(why) = notifier.ask(&question, Box::new(|_| {})) {
        assert!(!why.is_empty(), "a refusal says why");
    }
    let notice = Notice { title: "Saved: Contract test".into(), body: "Test.".into(), action: Some("Open".into()) };
    if let Err(why) = notifier.notify(&notice, Box::new(|| {})) {
        assert!(!why.is_empty(), "a refusal says why");
    }
}

/// A shown question is answered exactly once with what the user chose. `answer` plays the user.
pub fn notifier_answers_once(notifier: &dyn Notifier, answer: &dyn Fn(bool) -> bool) {
    for choice in [true, false] {
        let got = Arc::new(Mutex::new(Vec::new()));
        let sink = got.clone();
        let q = Question { title: "t".into(), body: "b".into(), accept: "Yes".into(), decline: "No".into() };
        notifier.ask(&q, Box::new(move |a| sink.lock().unwrap().push(a))).unwrap();
        assert!(answer(choice));
        assert!(!answer(choice), "a question is answered once");
        assert_eq!(*got.lock().unwrap(), vec![choice]);
    }
}

/// The tray takes any menu, the same one twice, and an empty one.
pub fn tray(tray: &dyn Tray) {
    let menu = TrayMenu {
        items: vec![
            TrayItem::Label { label: "Transcribing · Contract test".into() },
            TrayItem::Action { id: "stop".into(), label: "Stop and Save".into(), enabled: true, detail: Some("Started 14:30".into()) },
            TrayItem::Separator,
            TrayItem::Action { id: "quit".into(), label: "Quit Muesli".into(), enabled: true, detail: None },
        ],
        transcribing: true,
        tooltip: "Muesli".into(),
        transcribing_since: None,
        update_ready: true,
    };
    tray.show(&menu);
    tray.show(&menu);
    tray.show(&TrayMenu { transcribing: false, ..menu.clone() });
    tray.show(&TrayMenu::default());
}

/// Sampling is quick and stable: two samples in a row agree on what cannot change that fast.
pub fn meeting_detector(detector: &dyn MeetingDetector) {
    let now = Utc::now();
    let started = std::time::Instant::now();
    let a = detector.sample(now);
    let b = detector.sample(now);
    assert!(started.elapsed() < Duration::from_secs(2), "sampling must be quick; it runs every few seconds");
    assert_eq!(a.calendar.is_some(), b.calendar.is_some());
}

/// A source delivers samples only between `start` and `stop`, and stops when dropped.
/// `start` opens the source; `produce` makes it deliver something (feeds a fake, or waits for a
/// real device).
pub fn audio_source(start: &dyn Fn(OnSamples, OnError) -> Result<Box<dyn AudioStream>, String>, produce: &dyn Fn(), real_time: bool) {
    let count = Arc::new(AtomicUsize::new(0));
    let sink = count.clone();
    let mut stream = start(
        Box::new(move |s: &[f32]| {
            sink.fetch_add(s.len(), Ordering::SeqCst);
        }),
        Arc::new(|_| {}),
    )
    .expect("start");
    produce();
    if real_time {
        std::thread::sleep(Duration::from_millis(1500));
    }
    stream.stop();
    let heard = count.load(Ordering::SeqCst);
    assert!(heard > 0, "no samples arrived while running");
    produce();
    if real_time {
        std::thread::sleep(Duration::from_millis(300));
    }
    assert_eq!(count.load(Ordering::SeqCst), heard, "samples arrived after stop");

    // Dropping without stop also stops.
    let count = Arc::new(AtomicUsize::new(0));
    let sink = count.clone();
    let stream = start(
        Box::new(move |s: &[f32]| {
            sink.fetch_add(s.len(), Ordering::SeqCst);
        }),
        Arc::new(|_| {}),
    )
    .expect("restart");
    drop(stream);
    produce();
    if real_time {
        std::thread::sleep(Duration::from_millis(300));
    }
    assert_eq!(count.load(Ordering::SeqCst), 0, "samples arrived after drop");
}

/// The running version is a version; a check says "up to date", offers a newer stable version or
/// says why it could not check; nothing can be installed before a verified download.
pub fn updater(updater: &dyn Updater) {
    let current = updater.current_version();
    assert!(muesli_core::update::is_newer("999.0.0", &current), "the current version {current:?} is not a version");
    let started = std::time::Instant::now();
    match updater.check() {
        Ok(None) => {}
        Ok(Some(release)) => {
            assert!(muesli_core::update::is_newer(&release.version, &current), "offered {} while running {current}", release.version)
        }
        Err(why) => assert!(!why.to_string().is_empty(), "a failed check says why"),
    }
    assert!(started.elapsed() < Duration::from_secs(20), "a check gives up after 15 s");
    assert!(updater.install_and_restart().is_err(), "nothing downloaded, nothing to install");
    assert!(updater.install_on_quit().is_err(), "nothing downloaded, nothing to install");
}

/// With a newer release on offer: the download reports progress and can then be installed once;
/// a download that does not match the signature is never installed. `bad_signature` makes the
/// next download fail its verification. Only for adapters that can be driven: the fakes.
pub fn updater_installs_only_verified_downloads(updater: &dyn Updater, bad_signature: &dyn Fn(bool)) {
    let release = updater.check().expect("check").expect("a newer release is on offer");
    let seen = Arc::new(Mutex::new(Vec::new()));
    let sink = seen.clone();
    updater.download(&release, Box::new(move |received, total| sink.lock().unwrap().push((received, total)))).expect("download");
    let seen = seen.lock().unwrap().clone();
    assert!(!seen.is_empty(), "no progress reported");
    assert!(seen.windows(2).all(|w| w[0].0 <= w[1].0), "progress went backwards");
    updater.install_on_quit().expect("install a verified download");
    assert!(updater.install_and_restart().is_err(), "a download installs once");

    bad_signature(true);
    let release = updater.check().unwrap().unwrap();
    assert_eq!(updater.download(&release, Box::new(|_, _| {})), Err(UpdateError::BadSignature));
    assert!(updater.install_and_restart().is_err(), "a download that failed verification was discarded");
    bad_signature(false);
}
