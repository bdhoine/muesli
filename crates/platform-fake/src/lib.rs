//! Fake adapters for every port. They build on every OS, touch nothing outside the folder they are
//! given and can be driven by hand: tests move the clock, script what the detector sees, feed
//! samples or play a WAV file as microphone and system audio, and answer notifications.
//!
//! `FakePlatform::new(root)` builds one of each and the [`Platform`] bundle that holds them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use chrono::{DateTime, Duration, TimeZone, Utc};
use muesli_core::ports::{
    Access, AudioStream, Click, Clock, DetectionSample, LoginItem, MeetingDetector, MicCapture, Notice, Notifier, OnAction, OnAnswer,
    OnError, OnProgress, OnSamples, Permission, Permissions, Platform, PlatformPaths, PlatformWords, Question, Release, SystemAudioCapture,
    Trash, Tray, TrayMenu, UpdateError, Updater,
};
use muesli_core::DetectedMeeting;

// ---------------------------------------------------------------------------------------------

/// A clock that only moves when told to.
pub struct ManualClock {
    now: Mutex<DateTime<Utc>>,
}

impl ManualClock {
    /// Starts at 2027-01-15 09:00 UTC, a Friday morning.
    pub fn new() -> Self {
        Self::at(Utc.with_ymd_and_hms(2027, 1, 15, 9, 0, 0).unwrap())
    }

    pub fn at(now: DateTime<Utc>) -> Self {
        Self { now: Mutex::new(now) }
    }

    pub fn advance(&self, by: Duration) {
        *self.now.lock().unwrap() += by;
    }

    pub fn set(&self, now: DateTime<Utc>) {
        *self.now.lock().unwrap() = now;
    }
}

impl Default for ManualClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for ManualClock {
    fn now(&self) -> DateTime<Utc> {
        *self.now.lock().unwrap()
    }
}

// ---------------------------------------------------------------------------------------------

struct Live {
    on_samples: OnSamples,
    on_error: OnError,
}

/// What `FakeAudio::feed_seconds` feeds: `seconds` of a steady tone at 16 kHz.
pub fn tone(seconds: f64) -> Vec<f32> {
    (0..(seconds * 16_000.0) as usize).map(|i| 0.1 * ((i as f32) * 0.05).sin()).collect()
}

/// A microphone or system-audio source. While a stream is open, `feed` hands it samples
/// (16 kHz mono), `play_file` plays an audio file through it and `break_stream` makes it fail.
#[derive(Clone, Default)]
pub struct FakeAudio {
    live: Arc<Mutex<Option<Live>>>,
    /// When set, `start` fails with this message (no device, permission denied).
    pub fail_with: Arc<Mutex<Option<String>>>,
    starts: Arc<AtomicUsize>,
}

impl FakeAudio {
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a stream is open.
    pub fn is_running(&self) -> bool {
        self.live.lock().unwrap().is_some()
    }

    /// How often a stream was opened.
    pub fn starts(&self) -> usize {
        self.starts.load(Ordering::SeqCst)
    }

    /// Hands samples to the open stream; ignored when none is open.
    pub fn feed(&self, samples: &[f32]) {
        if let Some(live) = self.live.lock().unwrap().as_mut() {
            (live.on_samples)(samples);
        }
    }

    /// `seconds` of a steady tone: speech, as far as the stand-in voice detector can tell.
    pub fn feed_seconds(&self, seconds: f64) {
        self.feed(&tone(seconds));
    }

    /// `seconds` of digital silence.
    pub fn feed_silence(&self, seconds: f64) {
        self.feed(&vec![0.0; (seconds * 16_000.0) as usize]);
    }

    /// Decodes an audio file (any format `muesli-audio` reads) and feeds all of it at once.
    pub fn play_file(&self, path: &Path) -> Result<(), String> {
        let mut blocks = Vec::new();
        let cancel = AtomicBool::new(false);
        muesli_audio::decode_file(path, &cancel, |block, _| blocks.extend_from_slice(block)).map_err(|e| e.to_string())?;
        self.feed(&blocks);
        Ok(())
    }

    /// The stream breaks for good, as when a device is unplugged.
    pub fn break_stream(&self, why: &str) {
        let on_error = self.live.lock().unwrap().as_ref().map(|l| l.on_error.clone());
        if let Some(on_error) = on_error {
            on_error(why.to_string());
        }
    }

    fn open(&self, on_samples: OnSamples, on_error: OnError) -> Result<Box<dyn AudioStream>, String> {
        if let Some(why) = self.fail_with.lock().unwrap().clone() {
            return Err(why);
        }
        self.starts.fetch_add(1, Ordering::SeqCst);
        *self.live.lock().unwrap() = Some(Live { on_samples, on_error });
        Ok(Box::new(FakeStream { live: self.live.clone() }))
    }
}

struct FakeStream {
    live: Arc<Mutex<Option<Live>>>,
}

impl AudioStream for FakeStream {
    fn stop(&mut self) {
        self.live.lock().unwrap().take();
    }
}

impl Drop for FakeStream {
    fn drop(&mut self) {
        self.stop();
    }
}

impl MicCapture for FakeAudio {
    fn start(&self, on_samples: OnSamples, on_error: OnError) -> Result<Box<dyn AudioStream>, String> {
        self.open(on_samples, on_error)
    }
}

impl SystemAudioCapture for FakeAudio {
    fn start(&self, on_samples: OnSamples, on_error: OnError) -> Result<Box<dyn AudioStream>, String> {
        self.open(on_samples, on_error)
    }
}

// ---------------------------------------------------------------------------------------------

/// A meeting detector that reports whatever it is told.
#[derive(Default)]
pub struct ScriptedDetector {
    sample: Mutex<DetectionSample>,
}

impl ScriptedDetector {
    pub fn set(&self, sample: DetectionSample) {
        *self.sample.lock().unwrap() = sample;
    }

    pub fn set_mic_busy(&self, busy: Option<bool>) {
        self.sample.lock().unwrap().mic_busy = busy;
    }

    pub fn set_teams_running(&self, running: bool) {
        self.sample.lock().unwrap().teams_running = running;
    }

    pub fn set_calendar(&self, meeting: Option<(DetectedMeeting, String)>) {
        self.sample.lock().unwrap().calendar = meeting;
    }
}

impl MeetingDetector for ScriptedDetector {
    fn sample(&self, _now: DateTime<Utc>) -> DetectionSample {
        self.sample.lock().unwrap().clone()
    }
}

// ---------------------------------------------------------------------------------------------

/// Records questions and notices; `answer` and `click_action` play the user.
pub struct FakeNotifier {
    /// When false, `ask` and `notify` fail as with notifications switched off.
    pub enabled: AtomicBool,
    questions: Mutex<Vec<Question>>,
    notices: Mutex<Vec<Notice>>,
    pending_answer: Mutex<Option<(OnAnswer, OnAction)>>,
    pending_action: Mutex<Option<OnAction>>,
}

impl Default for FakeNotifier {
    fn default() -> Self {
        Self {
            enabled: AtomicBool::new(true),
            questions: Default::default(),
            notices: Default::default(),
            pending_answer: Default::default(),
            pending_action: Default::default(),
        }
    }
}

impl FakeNotifier {
    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::SeqCst);
    }

    pub fn questions(&self) -> Vec<Question> {
        self.questions.lock().unwrap().clone()
    }

    pub fn notices(&self) -> Vec<Notice> {
        self.notices.lock().unwrap().clone()
    }

    /// The question waiting for an answer, if any.
    pub fn pending_question(&self) -> Option<Question> {
        if self.pending_answer.lock().unwrap().is_some() {
            self.questions.lock().unwrap().last().cloned()
        } else {
            None
        }
    }

    /// The notice whose button was not clicked yet, if any.
    pub fn pending_notice(&self) -> Option<Notice> {
        if self.pending_action.lock().unwrap().is_some() {
            self.notices.lock().unwrap().last().cloned()
        } else {
            None
        }
    }

    /// Dismisses the notice without clicking.
    pub fn dismiss_notice(&self) {
        self.pending_action.lock().unwrap().take();
    }

    /// Answers the last question shown. Returns false when none is waiting.
    pub fn answer(&self, accept: bool) -> bool {
        let Some((on_answer, _)) = self.pending_answer.lock().unwrap().take() else { return false };
        on_answer(accept);
        true
    }

    /// Clicks the last question's notification itself, not a button. What happens follows the
    /// question's `click`: it is answered yes, or Muesli is brought forward and the question
    /// keeps waiting. Returns false when none is waiting.
    pub fn click_question(&self) -> bool {
        let Some(click) = self.pending_question().map(|q| q.click) else { return false };
        match click {
            Click::Accept => self.answer(true),
            Click::Open => {
                let pending = self.pending_answer.lock().unwrap();
                if let Some((_, on_open)) = pending.as_ref() {
                    on_open();
                }
                true
            }
        }
    }

    /// Clicks the last notice's button.
    pub fn click_action(&self) -> bool {
        let Some(on_action) = self.pending_action.lock().unwrap().take() else { return false };
        on_action();
        true
    }

    fn check(&self) -> Result<(), String> {
        if self.enabled.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err("Notifications are off.".into())
        }
    }
}

impl Notifier for FakeNotifier {
    fn ask(&self, question: &Question, on_answer: OnAnswer, on_open: OnAction) -> Result<(), String> {
        self.check()?;
        self.questions.lock().unwrap().push(question.clone());
        *self.pending_answer.lock().unwrap() = Some((on_answer, on_open));
        Ok(())
    }

    fn notify(&self, notice: &Notice, on_action: OnAction) -> Result<(), String> {
        self.check()?;
        self.notices.lock().unwrap().push(notice.clone());
        *self.pending_action.lock().unwrap() = Some(on_action);
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------

/// Permissions as a table. `request` grants unless `deny_requests` is set.
pub struct FakePermissions {
    table: Mutex<HashMap<Permission, Access>>,
    pub deny_requests: AtomicBool,
    requests: Mutex<Vec<Permission>>,
    opened_settings: Mutex<Vec<Permission>>,
}

impl FakePermissions {
    /// Everything not asked yet, as on a fresh Mac.
    pub fn new() -> Self {
        Self::with(Permission::ALL.iter().map(|p| (*p, Access::NotAsked)))
    }

    /// Everything allowed.
    pub fn all_allowed() -> Self {
        Self::with(Permission::ALL.iter().map(|p| (*p, Access::Allowed)))
    }

    pub fn with(entries: impl IntoIterator<Item = (Permission, Access)>) -> Self {
        Self {
            table: Mutex::new(entries.into_iter().collect()),
            deny_requests: AtomicBool::new(false),
            requests: Default::default(),
            opened_settings: Default::default(),
        }
    }

    pub fn set(&self, permission: Permission, access: Access) {
        self.table.lock().unwrap().insert(permission, access);
    }

    pub fn requests(&self) -> Vec<Permission> {
        self.requests.lock().unwrap().clone()
    }

    pub fn opened_settings(&self) -> Vec<Permission> {
        self.opened_settings.lock().unwrap().clone()
    }
}

impl Default for FakePermissions {
    fn default() -> Self {
        Self::new()
    }
}

impl Permissions for FakePermissions {
    fn status(&self, permission: Permission) -> Access {
        *self.table.lock().unwrap().get(&permission).unwrap_or(&Access::Unknown)
    }

    fn request(&self, permission: Permission) -> Result<(), String> {
        self.requests.lock().unwrap().push(permission);
        let mut table = self.table.lock().unwrap();
        let entry = table.entry(permission).or_insert(Access::NotAsked);
        if *entry == Access::NotAsked {
            *entry = if self.deny_requests.load(Ordering::SeqCst) { Access::Denied } else { Access::Allowed };
        }
        Ok(())
    }

    fn open_settings(&self, permission: Permission) -> Result<(), String> {
        self.opened_settings.lock().unwrap().push(permission);
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------

/// Keeps the menu it was last asked to show.
#[derive(Default)]
pub struct FakeTray {
    menu: Mutex<TrayMenu>,
    draws: AtomicUsize,
}

impl FakeTray {
    pub fn menu(&self) -> TrayMenu {
        self.menu.lock().unwrap().clone()
    }

    pub fn draws(&self) -> usize {
        self.draws.load(Ordering::SeqCst)
    }
}

impl Tray for FakeTray {
    fn show(&self, menu: &TrayMenu) {
        self.draws.fetch_add(1, Ordering::SeqCst);
        *self.menu.lock().unwrap() = menu.clone();
    }
}

// ---------------------------------------------------------------------------------------------

/// A trash that is a folder: files keep their name (a number is added on a clash).
pub struct FolderTrash {
    folder: PathBuf,
    name: &'static str,
    trashed: Mutex<Vec<(PathBuf, PathBuf)>>,
}

impl FolderTrash {
    pub fn new(folder: impl Into<PathBuf>, name: &'static str) -> Self {
        Self { folder: folder.into(), name, trashed: Default::default() }
    }

    /// (where it was, where it is now) for every file trashed.
    pub fn trashed(&self) -> Vec<(PathBuf, PathBuf)> {
        self.trashed.lock().unwrap().clone()
    }
}

impl Trash for FolderTrash {
    fn trash(&self, path: &Path) -> Result<(), String> {
        if !path.exists() {
            return Err(format!("{} does not exist", path.display()));
        }
        std::fs::create_dir_all(&self.folder).map_err(|e| e.to_string())?;
        let name = path.file_name().ok_or("no file name")?.to_string_lossy().into_owned();
        let mut target = self.folder.join(&name);
        let mut n = 2;
        while target.exists() {
            target = self.folder.join(format!("{n} {name}"));
            n += 1;
        }
        std::fs::rename(path, &target).map_err(|e| e.to_string())?;
        self.trashed.lock().unwrap().push((path.to_path_buf(), target));
        Ok(())
    }

    fn name(&self) -> &'static str {
        self.name
    }
}

// ---------------------------------------------------------------------------------------------

/// Folders under one root, and an environment of its own.
pub struct FakePaths {
    root: PathBuf,
    env: Mutex<HashMap<String, String>>,
}

impl FakePaths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into(), env: Default::default() }
    }

    pub fn set_env(&self, name: &str, value: Option<&str>) {
        let mut env = self.env.lock().unwrap();
        match value {
            Some(v) => env.insert(name.into(), v.into()),
            None => env.remove(name),
        };
    }
}

impl PlatformPaths for FakePaths {
    fn data_dir(&self) -> PathBuf {
        self.root.join("data")
    }

    fn local_data_dir(&self) -> PathBuf {
        self.root.join("local")
    }

    fn documents_dir(&self) -> PathBuf {
        self.root.join("Documents")
    }

    fn home_dir(&self) -> PathBuf {
        self.root.clone()
    }

    fn env(&self, name: &str) -> Option<String> {
        self.env.lock().unwrap().get(name).cloned()
    }
}

// ---------------------------------------------------------------------------------------------

#[derive(Default)]
pub struct FakeLoginItem {
    on: AtomicBool,
}

impl LoginItem for FakeLoginItem {
    fn is_enabled(&self) -> bool {
        self.on.load(Ordering::SeqCst)
    }

    fn set_enabled(&self, on: bool) -> Result<(), String> {
        self.on.store(on, Ordering::SeqCst);
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------------

/// What GitHub offers the fake updater.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FakeServer {
    /// No release, or a release without `latest.json`: up to date.
    NoRelease,
    /// The latest release has this version (newer, equal or older than the app).
    Release(String),
    /// The check fails with this reason (no connection, a timeout).
    NetworkError(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Download {
    None,
    /// Waiting for `finish_download` / `fail_download` (only with `hold_downloads`).
    Running,
    Done,
}

struct UpdaterState {
    server: FakeServer,
    found: Option<Release>,
    download: Download,
    /// The outcome a held download ends with.
    outcome: Option<Result<(), UpdateError>>,
    on_progress: Option<Arc<OnProgress>>,
}

/// Muesli's updates without a network. Tests and the simulator say what the server offers
/// (`set_server`), whether the download matches its signature (`bad_signature`), and whether
/// downloads run at once or step by step (`hold_downloads`, then `progress` and `finish_download`).
/// Installing only records that it happened.
pub struct FakeUpdater {
    current: String,
    state: Mutex<UpdaterState>,
    changed: Condvar,
    /// The next download does not match the signature.
    pub bad_signature: AtomicBool,
    /// Downloads wait for `finish_download` or `fail_download`.
    pub hold_downloads: AtomicBool,
    checks: AtomicUsize,
    installed_now: AtomicUsize,
    installed_on_quit: AtomicUsize,
}

impl FakeUpdater {
    /// The running app is `current`; GitHub has no release yet.
    pub fn new(current: &str) -> Self {
        Self {
            current: current.into(),
            state: Mutex::new(UpdaterState {
                server: FakeServer::NoRelease,
                found: None,
                download: Download::None,
                outcome: None,
                on_progress: None,
            }),
            changed: Condvar::new(),
            bad_signature: AtomicBool::new(false),
            hold_downloads: AtomicBool::new(false),
            checks: AtomicUsize::new(0),
            installed_now: AtomicUsize::new(0),
            installed_on_quit: AtomicUsize::new(0),
        }
    }

    pub fn set_server(&self, server: FakeServer) {
        self.state.lock().unwrap().server = server;
    }

    /// GitHub's latest release becomes `version`.
    pub fn publish(&self, version: &str) {
        self.set_server(FakeServer::Release(version.into()));
    }

    /// How often `check` was called.
    pub fn checks(&self) -> usize {
        self.checks.load(Ordering::SeqCst)
    }

    /// Whether a held download is waiting.
    pub fn downloading(&self) -> bool {
        self.state.lock().unwrap().download == Download::Running
    }

    /// The held download reached `received` of `total` bytes.
    pub fn progress(&self, received: u64, total: u64) {
        let on_progress = self.state.lock().unwrap().on_progress.clone();
        if let Some(f) = on_progress {
            f(received, Some(total));
        }
    }

    /// The held download completes (and is verified, unless `bad_signature` is set).
    pub fn finish_download(&self) {
        let mut s = self.state.lock().unwrap();
        s.outcome = Some(Ok(()));
        self.changed.notify_all();
    }

    /// The held download breaks off.
    pub fn fail_download(&self, why: &str) {
        let mut s = self.state.lock().unwrap();
        s.outcome = Some(Err(UpdateError::Network(why.into())));
        self.changed.notify_all();
    }

    /// How often "install and restart" ran.
    pub fn installed_now(&self) -> usize {
        self.installed_now.load(Ordering::SeqCst)
    }

    /// How often "install on quit" ran.
    pub fn installed_on_quit(&self) -> usize {
        self.installed_on_quit.load(Ordering::SeqCst)
    }

    fn install(&self, counter: &AtomicUsize) -> Result<(), UpdateError> {
        let mut s = self.state.lock().unwrap();
        if s.download != Download::Done {
            return Err(UpdateError::Install("No update was downloaded.".into()));
        }
        s.download = Download::None;
        counter.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

impl Updater for FakeUpdater {
    fn current_version(&self) -> String {
        self.current.clone()
    }

    fn check(&self) -> Result<Option<Release>, UpdateError> {
        self.checks.fetch_add(1, Ordering::SeqCst);
        let mut s = self.state.lock().unwrap();
        let found = match &s.server {
            FakeServer::NoRelease => None,
            FakeServer::NetworkError(why) => return Err(UpdateError::Network(why.clone())),
            FakeServer::Release(version) => muesli_core::update::is_newer(version, &self.current)
                .then(|| Release { version: version.trim_start_matches('v').to_string(), notes: Some("Bug fixes.".into()) }),
        };
        s.found = found.clone();
        Ok(found)
    }

    fn download(&self, release: &Release, on_progress: OnProgress) -> Result<(), UpdateError> {
        let on_progress = Arc::new(on_progress);
        let mut s = self.state.lock().unwrap();
        if s.found.as_ref() != Some(release) {
            return Err(UpdateError::Network("This release was not found by the last check.".into()));
        }
        s.download = Download::Running;
        s.outcome = None;
        s.on_progress = Some(on_progress.clone());
        let outcome = if self.hold_downloads.load(Ordering::SeqCst) {
            while s.outcome.is_none() {
                s = self.changed.wait(s).unwrap();
            }
            s.outcome.take().unwrap()
        } else {
            drop(s);
            on_progress(50, Some(100));
            on_progress(100, Some(100));
            s = self.state.lock().unwrap();
            Ok(())
        };
        s.on_progress = None;
        let outcome =
            outcome.and_then(|()| if self.bad_signature.load(Ordering::SeqCst) { Err(UpdateError::BadSignature) } else { Ok(()) });
        s.download = if outcome.is_ok() { Download::Done } else { Download::None };
        outcome
    }

    fn install_and_restart(&self) -> Result<(), UpdateError> {
        self.install(&self.installed_now)
    }

    fn install_on_quit(&self) -> Result<(), UpdateError> {
        self.install(&self.installed_on_quit)
    }
}

// ---------------------------------------------------------------------------------------------

/// One fake of each port, kept typed so a test can drive them, plus the bundle the engine gets.
#[derive(Clone)]
pub struct FakePlatform {
    pub clock: Arc<ManualClock>,
    pub mic: FakeAudio,
    pub system_audio: FakeAudio,
    pub detector: Arc<ScriptedDetector>,
    pub notifier: Arc<FakeNotifier>,
    pub permissions: Arc<FakePermissions>,
    pub tray: Arc<FakeTray>,
    pub trash: Arc<FolderTrash>,
    pub paths: Arc<FakePaths>,
    pub login_item: Arc<FakeLoginItem>,
    pub words: PlatformWords,
}

impl FakePlatform {
    /// Everything under `root`; macOS words. The trash is `root/Trash`.
    pub fn new(root: &Path) -> Self {
        Self::with_words(root, PlatformWords::MAC)
    }

    pub fn with_words(root: &Path, words: PlatformWords) -> Self {
        Self {
            clock: Arc::new(ManualClock::new()),
            mic: FakeAudio::new(),
            system_audio: FakeAudio::new(),
            detector: Arc::new(ScriptedDetector::default()),
            notifier: Arc::new(FakeNotifier::default()),
            permissions: Arc::new(FakePermissions::all_allowed()),
            tray: Arc::new(FakeTray::default()),
            trash: Arc::new(FolderTrash::new(root.join("Trash"), words.trash)),
            paths: Arc::new(FakePaths::new(root)),
            login_item: Arc::new(FakeLoginItem::default()),
            words,
        }
    }

    pub fn platform(&self) -> Platform {
        Platform {
            clock: self.clock.clone(),
            mic: Arc::new(self.mic.clone()),
            system_audio: Arc::new(self.system_audio.clone()),
            detector: self.detector.clone(),
            notifier: self.notifier.clone(),
            permissions: self.permissions.clone(),
            tray: self.tray.clone(),
            trash: self.trash.clone(),
            paths: self.paths.clone(),
            login_item: self.login_item.clone(),
            words: self.words.clone(),
        }
    }
}
