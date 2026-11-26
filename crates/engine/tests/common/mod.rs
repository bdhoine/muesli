#![allow(dead_code)]

use std::sync::Arc;
use std::time::{Duration, Instant};

use muesli_core::ports::{DetectionSample, PlatformWords};
use muesli_core::{DetectedMeeting, Preferences};
use muesli_engine::fake::{FakeSpeech, RecordingHost};
use muesli_engine::{Engine, Options};
use muesli_platform_fake::{FakePlatform, FakeUpdater};

pub struct Harness {
    pub dir: tempfile::TempDir,
    pub fakes: FakePlatform,
    pub speech: Arc<FakeSpeech>,
    pub host: Arc<RecordingHost>,
    pub engine: Arc<Engine>,
    /// Muesli's updates; only engines built with `with_updater` use it.
    pub updater: Arc<FakeUpdater>,
}

pub fn languages() -> Vec<String> {
    vec!["nl".into(), "en".into()]
}

impl Harness {
    /// A Mac that finished setup, with the model downloaded.
    pub fn ready() -> Self {
        Self::build(PlatformWords::MAC, true, FakeSpeech::ready())
    }

    /// A first launch: no config.json, no model.
    pub fn first_launch() -> Self {
        Self::build(PlatformWords::MAC, false, FakeSpeech::missing(1000))
    }

    pub fn build(words: PlatformWords, setup_done: bool, speech: FakeSpeech) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let fakes = FakePlatform::with_words(dir.path(), words);
        if setup_done {
            let paths = muesli_core::Paths::from_platform(&*fakes.paths);
            std::fs::create_dir_all(&paths.support).unwrap();
            Preferences { onboarding_done: true, ..Default::default() }.save(&paths).unwrap();
        }
        Self::with(dir, fakes, speech)
    }

    pub fn with(dir: tempfile::TempDir, fakes: FakePlatform, speech: FakeSpeech) -> Self {
        Self::start(dir, fakes, speech, None)
    }

    /// An engine that updates itself through `updater` (a release build).
    pub fn with_updater(dir: tempfile::TempDir, fakes: FakePlatform, speech: FakeSpeech, updater: Arc<FakeUpdater>) -> Self {
        Self::start(dir, fakes, speech, Some(updater))
    }

    fn start(dir: tempfile::TempDir, fakes: FakePlatform, speech: FakeSpeech, updater: Option<Arc<FakeUpdater>>) -> Self {
        let speech = Arc::new(speech);
        let host = Arc::new(RecordingHost::default());
        let mut options = Options::new(languages());
        if let Some(u) = &updater {
            options = options.with_updater(u.clone());
        }
        let engine = Engine::new(fakes.platform(), speech.clone(), host.clone(), options);
        engine.start();
        let updater = updater.unwrap_or_else(|| Arc::new(FakeUpdater::new(env!("CARGO_PKG_VERSION"))));
        Self { dir, fakes, speech, host, engine, updater }
    }

    /// Moves the clock on in steps of a second, ticking the engine each time.
    pub fn advance(&self, seconds: i64) {
        for _ in 0..seconds {
            self.fakes.clock.advance(chrono::Duration::seconds(1));
            self.engine.tick();
        }
    }

    /// Another app holds the microphone (a call), for `seconds`.
    pub fn call_for(&self, seconds: i64, teams: bool) {
        self.fakes.detector.set(DetectionSample { calendar: None, mic_busy: Some(true), teams_running: teams });
        self.advance(seconds);
    }

    pub fn calendar(&self, title: &str, id: &str) {
        self.fakes.detector.set_calendar(Some((DetectedMeeting::new(title, "calendar"), id.into())));
    }

    pub fn mic_free(&self) {
        self.fakes.detector.set_mic_busy(Some(false));
    }

    /// Both sides talk for `seconds` (a quiet tone the scripted transcriber turns into text).
    pub fn talk(&self, seconds: f64) {
        self.fakes.mic.feed_seconds(seconds);
        self.fakes.system_audio.feed_seconds(seconds);
    }

    pub fn menu_labels(&self) -> Vec<String> {
        use muesli_core::ports::TrayItem;
        self.engine
            .menu()
            .items
            .into_iter()
            .filter_map(|i| match i {
                TrayItem::Action { label, enabled, .. } => Some(if enabled { label } else { format!("({label})") }),
                TrayItem::Label { label } => Some(format!("# {label}")),
                TrayItem::Separator => None,
            })
            .collect()
    }

    pub fn transcribing(&self) -> bool {
        self.engine.app_status().app_state == "recording"
    }

    pub fn idle(&self) -> bool {
        self.engine.app_status().app_state == "idle"
    }
}

/// Waits up to 30 seconds for background work (audio threads, the transcriber, imports).
pub fn wait_for(what: &str, mut condition: impl FnMut() -> bool) {
    let start = Instant::now();
    while !condition() {
        assert!(start.elapsed() < Duration::from_secs(30), "timed out waiting for: {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}
