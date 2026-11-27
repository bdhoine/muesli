use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::thread::JoinHandle;

use chrono::{DateTime, Duration, Utc};
use muesli_core::assistants::{AssistantClient, ClaudeDesktopConfig, ClaudeDesktopExtension, ExtensionState};
use muesli_core::editor::MeetingEditor;
use muesli_core::ports::{Access, Click, Notice, OnError, Permission, Platform, Question, TrayItem, TrayMenu, Updater};
use muesli_core::status::{Activity as StatusActivity, AppStatus, Current, Detected, FileJob, LastReady};
use muesli_core::store::{menu_subtitle_local, shorten_title, MeetingStore, Store};
use muesli_core::whisper_model::DEFAULT_MODEL;
use muesli_core::{
    languages, AudioChunk, DetectedMeeting, DetectionEvent, DetectionLogic, Meeting, MeetingState, Pacer, Paths, Preferences, Vocabulary,
    VERSION,
};
use serde::Serialize;

mod updates;

pub use updates::{CHECK_EVERY, FIRST_CHECK_AFTER};

use crate::final_pass::{self, Kept};
use crate::host::{Host, Window};
use crate::intent::Intent;
use crate::live::Live;
use crate::speech::{ModelStatus, SpeechModel, SpeedUp, SpeedUpStage, Transcriber};
use crate::views::*;

/// How long the microphone must be free before a call counts as ended. Fixed; no setting.
pub const CALL_END_DELAY: i64 = 60;
/// How long "The call seems to have ended" waits for an answer before Muesli stops and saves.
pub const ANSWER_TIMEOUT: i64 = 5 * 60;
/// How long a toast (and its Undo) stays.
pub const TOAST_SECONDS: i64 = 6;
/// How often the detector looks at the system.
const DETECT_EVERY: i64 = 5;
/// How often `state.json` is rewritten as a heartbeat.
const HEARTBEAT_EVERY: i64 = 10;
/// The latest transcripts in the tray menu. Fixed; no setting.
const RECENT_IN_MENU: usize = 3;

/// What the engine needs besides the ports.
#[derive(Clone)]
pub struct Options {
    /// The system language and English: the default transcription languages.
    pub system_languages: Vec<String>,
    /// Muesli's own updates. `None` in builds that never update (debug builds, Linux).
    pub updater: Option<Arc<dyn Updater>>,
}

impl Options {
    pub fn new(system_languages: Vec<String>) -> Self {
        Self { system_languages, updater: None }
    }

    pub fn with_updater(mut self, updater: Arc<dyn Updater>) -> Self {
        self.updater = Some(updater);
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Activity {
    Idle,
    /// Loading the model and opening the audio streams.
    Starting,
    Transcribing,
    Saving,
    /// The end-of-meeting pass (`final_pass.rs`); details in `Inner::improving`.
    Improving,
    Error(String),
}

/// The end-of-meeting pass running on a stopped meeting.
struct Improving {
    meeting: Arc<Mutex<Meeting>>,
    done: usize,
    total: usize,
    /// Set by Start or Quit: the pass stops and the live text stays.
    cancel: Arc<AtomicBool>,
}

#[derive(Debug, Clone, PartialEq)]
enum PromptKind {
    Start(DetectedMeeting),
    Stop,
}

#[derive(Debug, Clone)]
struct Prompt {
    id: u64,
    kind: PromptKind,
    question: Question,
    asked_at: DateTime<Utc>,
    /// Asked in Muesli's own window because notifications could not.
    in_window: bool,
}

struct Session {
    meeting: Arc<Mutex<Meeting>>,
    /// Tells the audio thread to stop; it then stops both streams and closes the chunk channel.
    stop_audio: mpsc::Sender<()>,
    audio: Option<JoinHandle<()>>,
    /// Hands back the model and the blocks it transcribed, for the end-of-meeting pass.
    consumer: Option<JoinHandle<(Transcriber, Vec<Kept>)>>,
    /// Whisper's prompt for this meeting: the vocabulary, its title and attendees.
    prompt: String,
}

#[derive(Debug, Clone, PartialEq)]
enum ImportState {
    Waiting,
    Running {
        progress: Option<f64>,
    },
    /// Cancel was pressed while running: the worker is stopping, and the result is thrown away.
    Cancelling,
    Failed(String),
}

#[derive(Debug, Clone)]
struct Import {
    id: u64,
    path: PathBuf,
    name: String,
    state: ImportState,
    cancel: Arc<AtomicBool>,
}

#[derive(Debug, Clone)]
struct PendingDelete {
    id: String,
    at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct ToastState {
    text: String,
    undo: bool,
    at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum StopReason {
    Asked,
    NoAnswer,
}

struct Inner {
    prefs: Preferences,
    activity: Activity,
    session: Option<Session>,
    improving: Option<Improving>,
    detection: DetectionLogic,
    last_detection: Option<DateTime<Utc>>,
    last_heartbeat: Option<DateTime<Utc>>,
    prompt: Option<Prompt>,
    next_id: u64,
    imports: Vec<Import>,
    worker: bool,
    selected: Option<String>,
    pending_delete: Option<PendingDelete>,
    toast: Option<ToastState>,
    setup_step: usize,
    /// Setup reached the AI assistants step once: the switch was turned on then, never again.
    assistants_offered: bool,
    /// The last refresh caused by a download: when, and what kind of state it showed.
    last_download_refresh: Option<(DateTime<Utc>, DownloadKind)>,
    last_ready: Option<LastReady>,
    claude_status: Option<String>,
    /// Disconnect was pressed while the extension is installed: Settings says where to uninstall it.
    claude_disconnecting: bool,
    assistant: String,
    capture_error: Option<String>,
    /// Why the last term could not be added to the vocabulary (Settings).
    vocabulary_error: Option<String>,
    started: bool,
    quitting: bool,
    update: updates::UpdateState,
}

/// Muesli. Create it with [`Engine::new`], call [`Engine::start`] once and [`Engine::tick`]
/// about once a second; send it [`Intent`]s and ask it for views.
pub struct Engine {
    platform: Platform,
    speech: Arc<dyn SpeechModel>,
    host: Arc<dyn Host>,
    options: Options,
    base: Paths,
    me: Weak<Engine>,
    inner: Mutex<Inner>,
}

/// Setup's screens: 0 Welcome, 1 Languages, 2 Permissions, 3 AI assistants, 4 Done.
const SETUP_DONE: usize = 4;

/// Download progress refreshes the windows and the tray at most this often (A10).
const DOWNLOAD_REFRESH_MS: i64 = 250;

/// What a download refresh shows, without the numbers: a change of kind refreshes at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DownloadKind(u8, u8);

impl DownloadKind {
    fn of(status: &ModelStatus, speed: Option<SpeedUpStage>) -> Self {
        let model = match status {
            ModelStatus::Missing => 0,
            ModelStatus::Downloading { .. } => 1,
            ModelStatus::Ready => 2,
            ModelStatus::Failed { .. } => 3,
        };
        let speed = match speed {
            None => 0,
            Some(SpeedUpStage::Waiting) => 1,
            Some(SpeedUpStage::Downloading(_)) => 2,
            Some(SpeedUpStage::Preparing) => 3,
            Some(SpeedUpStage::Ready) => 4,
            Some(SpeedUpStage::Failed) => 5,
        };
        Self(model, speed)
    }
}

/// "40%" for the end-of-meeting pass.
fn improving_percent(improving: Option<&Improving>) -> String {
    let (done, total) = improving.map_or((0, 0), |i| (i.done, i.total));
    format!("{}%", (done * 100).checked_div(total).unwrap_or(0))
}

fn minutes(start: DateTime<Utc>, end: Option<DateTime<Utc>>) -> Option<i64> {
    end.map(|e| ((e - start).num_seconds() as f64 / 60.0).round() as i64)
}

fn clock_text(seconds: f64) -> String {
    let t = seconds.max(0.0) as i64;
    if t >= 3600 {
        format!("{}:{:02}:{:02}", t / 3600, (t % 3600) / 60, t % 60)
    } else {
        format!("{:02}:{:02}", t / 60, t % 60)
    }
}

/// How a meeting came about, in words.
/// The Claude Desktop row: connected through the extension or Claude Desktop's settings file, or
/// not connected. `file_status` is what the last change of that file reported.
/// `disconnecting`: Disconnect was pressed while the extension is installed, so the row says where
/// Claude Desktop uninstalls it (another app cannot).
fn claude_desktop_row(
    extension: &ExtensionState,
    through_file: bool,
    can_connect: bool,
    file_status: Option<&str>,
    disconnecting: bool,
) -> ClaudeDesktopRow {
    const WHAT: &str = "Claude can read and search your transcripts.";
    const HOW: &str = "In Claude Desktop, open Settings → Extensions, choose Muesli and Uninstall.";
    let (status, tone, button, detail, disconnect) = match extension {
        ExtensionState::Installed { .. } => ("Connected", "ok", None, if disconnecting { HOW } else { WHAT }.to_string(), true),
        ExtensionState::Outdated { .. } => (
            "An older version is connected",
            "warn",
            Some("Update"),
            if disconnecting { HOW.to_string() } else { format!("{WHAT} Update to get everything this Muesli can do.") },
            true,
        ),
        _ if through_file => {
            let note = file_status.and_then(|s| s.strip_prefix("Registered with Claude Desktop.")).map(str::trim).filter(|s| !s.is_empty());
            ("Connected", "ok", None, note.map_or_else(|| WHAT.to_string(), |n| format!("{WHAT} {n}")), true)
        }
        _ => (
            "Not connected",
            "",
            can_connect.then_some("Connect"),
            file_status.filter(|s| !s.starts_with("Removed")).unwrap_or("Lets Claude read and search your transcripts.").to_string(),
            false,
        ),
    };
    ClaudeDesktopRow {
        status: status.into(),
        tone: tone.into(),
        button: button.map(Into::into),
        detail: Some(detail),
        can_disconnect: disconnect,
    }
}

pub fn source_label(source: &str) -> String {
    match source {
        "calendar" => "Calendar",
        "teams" => "Teams call",
        "audio" => "Call",
        "manual" => "Started by hand",
        "file" => "Audio file",
        other => other,
    }
    .to_string()
}

fn lines(meeting: &Meeting) -> Vec<Line> {
    meeting.transcript.iter().map(|s| Line { time: clock_text(s.start), speaker: s.speaker.clone(), text: s.text.clone() }).collect()
}

fn iso(d: DateTime<Utc>) -> String {
    d.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

impl Engine {
    pub fn new(platform: Platform, speech: Arc<dyn SpeechModel>, host: Arc<dyn Host>, options: Options) -> Arc<Self> {
        let base = Paths::from_platform(&*platform.paths);
        let (prefs, _dropped) = Preferences::migrate(&base);
        Arc::new_cyclic(|me| Self {
            platform,
            speech,
            host,
            options,
            base,
            me: me.clone(),
            inner: Mutex::new(Inner {
                prefs,
                activity: Activity::Idle,
                session: None,
                improving: None,
                detection: {
                    let mut d = DetectionLogic::default();
                    d.end_delay = Duration::seconds(CALL_END_DELAY);
                    d
                },
                last_detection: None,
                last_heartbeat: None,
                prompt: None,
                next_id: 1,
                imports: Vec::new(),
                worker: false,
                selected: None,
                pending_delete: None,
                toast: None,
                setup_step: 0,
                assistants_offered: false,
                last_download_refresh: None,
                last_ready: None,
                claude_status: None,
                claude_disconnecting: false,
                assistant: AssistantClient::ClaudeCode.name().to_string(),
                capture_error: None,
                vocabulary_error: None,
                started: false,
                quitting: false,
                update: updates::UpdateState::default(),
            }),
        })
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn me(&self) -> Arc<Engine> {
        self.me.upgrade().expect("the engine is alive while it runs")
    }

    fn now(&self) -> DateTime<Utc> {
        self.platform.clock.now()
    }

    /// Where things are now; the transcripts folder follows the preferences.
    pub fn paths(&self) -> Paths {
        self.base.clone().with_preferences(&self.lock().prefs)
    }

    pub fn preferences(&self) -> Preferences {
        self.lock().prefs.clone()
    }

    /// Changes the preferences and writes `config.json`. The file is written under the lock, so
    /// changes from two threads (an update check, a click in Settings) land in order.
    fn save_prefs(&self, change: impl FnOnce(&mut Preferences)) {
        let mut s = self.lock();
        change(&mut s.prefs);
        let _ = std::fs::create_dir_all(&self.base.support);
        let _ = s.prefs.save(&self.base);
    }

    fn model_ready(&self) -> bool {
        self.speech.status().is_ready()
    }

    /// The first start: recover what a crash left, pick up transcripts, start downloading the
    /// model when setup is done, and open setup when it is not.
    pub fn start(&self) {
        self.start_with(false);
    }

    /// `start`, and with `open_main_window` the main window opens too once setup is done. The
    /// shell says so when a person started Muesli, and not when it started by itself at login.
    pub fn start_with(&self, open_main_window: bool) {
        let paths = self.paths();
        let _ = std::fs::create_dir_all(&paths.support);
        let store = MeetingStore::new(&paths);
        store.recover_interrupted();
        store.import_transcripts(&paths.transcripts);
        let onboarding_done = {
            let mut s = self.lock();
            s.started = true;
            s.last_ready = store.latest_ready().map(|m| LastReady { id: m.id, title: m.title, ended_at: m.ended_at });
            s.prefs.onboarding_done
        };
        self.sync_assistants();
        if onboarding_done {
            self.announce_extension_update();
            self.download_model(); // whenever a file went missing
            self.speech.prepare(); // a first transcription should not wait for one-time work
            if open_main_window {
                self.host.show(Window::Main);
            }
        } else {
            self.host.show(Window::Setup);
        }
        self.start_updates();
        self.changed();
    }

    fn download_model(&self) {
        let me = self.me.clone();
        self.speech.download(Box::new(move || {
            if let Some(engine) = me.upgrade() {
                engine.speech.prepare(); // the model, or the Core ML encoder, may have just arrived
                engine.download_changed();
            }
        }));
    }

    /// A download reported progress. On a fast line that is dozens of times a second; refreshing
    /// that often rebuilt the tray and re-rendered every window so fast that clicks in setup never
    /// landed. So: at once when the state changes kind (started, finished, failed), else at most
    /// every [`DOWNLOAD_REFRESH_MS`].
    fn download_changed(&self) {
        let kind = DownloadKind::of(&self.speech.status(), self.speech.speed_up().map(|s| s.stage));
        let now = self.now();
        {
            let mut s = self.lock();
            if let Some((at, last)) = s.last_download_refresh {
                if last == kind && (now - at).num_milliseconds() < DOWNLOAD_REFRESH_MS {
                    return;
                }
            }
            s.last_download_refresh = Some((now, kind));
        }
        self.changed();
    }

    /// Refreshes everything that shows state: the tray, the windows, `state.json`.
    fn changed(&self) {
        self.platform.tray.show(&self.menu());
        self.host.changed();
        self.heartbeat();
    }

    fn heartbeat(&self) {
        let status = self.app_status();
        let paths = self.paths();
        let _ = std::fs::create_dir_all(&paths.support);
        let _ = status.write(&paths);
        self.lock().last_heartbeat = Some(self.now());
    }

    /// The `state.json` other processes (`muesli-mcp`) read. Its values are unchanged.
    pub fn app_status(&self) -> AppStatus {
        let downloading = matches!(self.speech.status(), ModelStatus::Downloading { .. });
        let paths = self.paths();
        let s = self.lock();
        let running = s.imports.iter().find(|i| matches!(i.state, ImportState::Running { .. }));
        let activity = match &s.activity {
            Activity::Idle | Activity::Starting if running.is_some() => StatusActivity::TranscribingFile,
            Activity::Idle | Activity::Starting => StatusActivity::Idle { preparing: downloading },
            Activity::Transcribing => StatusActivity::Recording,
            Activity::Saving | Activity::Improving => StatusActivity::Finalizing,
            Activity::Error(message) => StatusActivity::Error(message.clone()),
        };
        let mut out = AppStatus::for_activity(&activity);
        out.updated_at = self.now();
        out.transcripts_folder = Some(paths.transcripts.to_string_lossy().into_owned());
        out.last_ready = s.last_ready.clone();
        out.file_job = running.map(|r| FileJob {
            name: r.name.clone(),
            progress: match r.state {
                ImportState::Running { progress } => progress,
                _ => None,
            },
            waiting: s.imports.iter().filter(|i| i.state == ImportState::Waiting).count() as u32,
        });
        if let Some((d, since)) = s.detection.in_progress.clone() {
            out.detected = Some(Detected { title: d.title, source: d.source, since });
        }
        let current = s.session.as_ref().map(|x| &x.meeting).or(s.improving.as_ref().map(|i| &i.meeting));
        if let Some(meeting) = current {
            let m = meeting.lock().unwrap();
            out.current = Some(Current {
                id: m.id.clone(),
                title: m.title.clone(),
                source: m.source.clone(),
                started_at: m.started_at,
                segment_count: m.transcript.len() as u32,
            });
        }
        out
    }

    // -----------------------------------------------------------------------------------------
    // Time

    /// Call about once a second. Runs detection every few seconds, the 5-minute answer timeout,
    /// the end of the undo toast and the `state.json` heartbeat.
    pub fn tick(&self) {
        let now = self.now();
        let (detect, beat, timed_out, expired_delete, toast_gone) = {
            let mut s = self.lock();
            if !s.started || s.quitting {
                return;
            }
            let detect = s.prefs.onboarding_done && s.last_detection.is_none_or(|t| now - t >= Duration::seconds(DETECT_EVERY));
            if detect {
                s.last_detection = Some(now);
            }
            let beat = s.last_heartbeat.is_none_or(|t| now - t >= Duration::seconds(HEARTBEAT_EVERY));
            let timed_out = match &s.prompt {
                Some(p) if p.kind == PromptKind::Stop && now - p.asked_at >= Duration::seconds(ANSWER_TIMEOUT) => s.prompt.take(),
                _ => None,
            };
            let expired_delete = match &s.pending_delete {
                Some(d) if now - d.at >= Duration::seconds(TOAST_SECONDS) => s.pending_delete.take(),
                _ => None,
            };
            let toast_gone = match &s.toast {
                Some(t) if now - t.at >= Duration::seconds(TOAST_SECONDS) => s.toast.take().is_some(),
                _ => false,
            };
            (detect, beat, timed_out, expired_delete, toast_gone)
        };
        // Some model states change without a download event (the speed-up finishes setting up in
        // the background): show them when the tick sees them.
        let kind = DownloadKind::of(&self.speech.status(), self.speech.speed_up().map(|s| s.stage));
        let stale = {
            let mut s = self.lock();
            match s.last_download_refresh {
                Some((_, shown)) => shown != kind,
                None => {
                    s.last_download_refresh = Some((now, kind));
                    false
                }
            }
        };
        if stale {
            self.download_changed();
        }
        if let Some(prompt) = timed_out {
            if prompt.in_window {
                self.host.close(Window::Prompt);
            }
            self.stop(StopReason::NoAnswer);
        }
        if let Some(delete) = &expired_delete {
            self.commit_delete(delete);
        }
        if detect {
            self.detect(now);
        }
        self.tick_updates(now);
        if expired_delete.is_some() || toast_gone {
            self.changed();
        } else if beat {
            self.heartbeat();
        }
    }

    fn detect(&self, now: DateTime<Utc>) {
        let sample = self.platform.detector.sample(now);
        let events = {
            let mut s = self.lock();
            let transcribing = matches!(s.activity, Activity::Transcribing | Activity::Starting);
            s.detection.tick_sample(now, transcribing, sample)
        };
        let mut changed = false;
        for event in events {
            match event {
                DetectionEvent::Suggest(meeting) => {
                    let idle = matches!(self.lock().activity, Activity::Idle | Activity::Improving);
                    if idle && self.model_ready() {
                        self.ask(PromptKind::Start(meeting));
                    }
                }
                DetectionEvent::CallEnded => {
                    if self.lock().activity == Activity::Transcribing {
                        self.ask(PromptKind::Stop);
                    }
                }
                DetectionEvent::InProgressChanged => changed = true,
            }
        }
        if changed {
            self.changed();
        }
    }

    // -----------------------------------------------------------------------------------------
    // Questions

    fn question(kind: &PromptKind) -> Question {
        match kind {
            PromptKind::Start(m) => Question {
                title: format!("Looks like a meeting: {}", m.title),
                body: format!("Detected via {}. Start transcribing?", m.source),
                accept: "Start transcribing".into(),
                decline: "Not now".into(),
                click: Click::Accept,
            },
            PromptKind::Stop => Question {
                title: "The call seems to have ended".into(),
                body: "Muesli is still transcribing. Stop and save the transcript?".into(),
                accept: "Stop and save".into(),
                decline: "Keep transcribing".into(),
                click: Click::Open, // stopping stays a deliberate press of the button
            },
        }
    }

    /// Asks as a notification; when that cannot be shown, in Muesli's own prompt window.
    fn ask(&self, kind: PromptKind) {
        let question = Self::question(&kind);
        let (id, replaced_window) = {
            let mut s = self.lock();
            let id = s.next_id;
            s.next_id += 1;
            let replaced_window = s.prompt.as_ref().is_some_and(|p| p.in_window);
            s.prompt = Some(Prompt { id, kind, question: question.clone(), asked_at: self.now(), in_window: false });
            (id, replaced_window)
        };
        let me = self.me.clone();
        let asked = self.platform.notifier.ask(
            &question,
            Box::new(move |accept| {
                if let Some(engine) = me.upgrade() {
                    engine.intent(Intent::AnswerPrompt { prompt: Some(id), accept });
                }
            }),
            {
                let me = self.me.clone();
                Box::new(move || {
                    if let Some(engine) = me.upgrade() {
                        engine.host.show(Window::Main);
                    }
                })
            },
        );
        if asked.is_err() {
            if let Some(p) = self.lock().prompt.as_mut().filter(|p| p.id == id) {
                p.in_window = true;
            }
            self.host.show(Window::Prompt);
        } else if replaced_window {
            self.host.close(Window::Prompt);
        }
        self.changed();
    }

    fn answer(&self, id: Option<u64>, accept: bool) {
        let prompt = {
            let mut s = self.lock();
            match (&s.prompt, id) {
                (Some(p), Some(id)) if p.id != id => return, // an older notification, answered late
                _ => s.prompt.take(),
            }
        };
        let Some(prompt) = prompt else { return };
        if prompt.in_window {
            self.host.close(Window::Prompt);
        }
        if accept {
            match prompt.kind {
                PromptKind::Start(meeting) => self.start_transcribing(meeting),
                PromptKind::Stop => self.stop(StopReason::Asked),
            }
        }
        self.changed();
    }

    // -----------------------------------------------------------------------------------------
    // Transcribing

    /// Starts transcribing in the background. Does nothing unless Muesli is idle, setup is done
    /// and the speech model is ready.
    fn start_transcribing(&self, detected: DetectedMeeting) {
        // The end-of-meeting pass gives way: it stops, the meeting before is saved with its live
        // text, then this one starts.
        let improving = self.lock().improving.as_ref().map(|i| i.cancel.clone());
        if let Some(cancel) = improving {
            cancel.store(true, Ordering::SeqCst);
            for _ in 0..600 {
                if self.lock().activity != Activity::Improving {
                    break;
                }
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
        {
            let mut s = self.lock();
            if s.activity != Activity::Idle || !s.prefs.onboarding_done || !self.model_ready() {
                return;
            }
            s.activity = Activity::Starting;
            s.capture_error = None;
            // A Start question still open is answered by starting.
            if s.prompt.as_ref().is_some_and(|p| matches!(p.kind, PromptKind::Start(_))) {
                let p = s.prompt.take().unwrap();
                if p.in_window {
                    drop(s);
                    self.host.close(Window::Prompt);
                }
            }
        }
        self.changed();
        let me = self.me();
        std::thread::spawn(move || {
            if let Err(message) = me.run_start(detected) {
                me.lock().activity = Activity::Error(message);
            }
            me.changed();
        });
    }

    fn run_start(&self, detected: DetectedMeeting) -> Result<(), String> {
        let languages = self.lock().prefs.effective_languages(&self.options.system_languages);
        let transcriber = self.speech.transcriber(languages)?;
        let paths = self.paths();
        let mut meeting = Meeting::new(&detected.title, &detected.source);
        meeting.started_at = self.now();
        meeting.attendees = detected.attendees.clone();
        let prompt = muesli_core::vocabulary::whisper_prompt(&Vocabulary::load(&paths), &meeting.title, &meeting.attendees);
        meeting.state = MeetingState::Recording;
        MeetingStore::new(&paths).save(&meeting);
        let id = meeting.id.clone();
        let meeting = Arc::new(Mutex::new(meeting));

        // Blocks go to one consumer, so one model is never used from two threads. It also times
        // Whisper, so blocks grow when it falls behind (`Pacer`).
        let (chunk_tx, chunk_rx) = mpsc::channel::<AudioChunk>();
        let pacer = Arc::new(Mutex::new(Pacer::default()));
        let consumer = {
            let (me, meeting, pacer, prompt) = (self.me(), meeting.clone(), pacer.clone(), prompt.clone());
            std::thread::spawn(move || {
                let mut kept = Vec::new();
                for mut chunk in chunk_rx {
                    chunk.prompt = prompt.clone();
                    kept.push(Kept::of(&chunk)); // for the end-of-meeting pass, in memory only
                    let started = std::time::Instant::now();
                    let result = transcriber.transcribe(&chunk);
                    let audio = chunk.samples.len() as f64 / muesli_core::segmenter::SAMPLE_RATE as f64;
                    pacer.lock().unwrap().record(audio, started.elapsed().as_secs_f64());
                    if let Ok(segments) = result {
                        if !segments.is_empty() {
                            let paths = me.paths(); // the engine's lock first, then the meeting's
                            MeetingStore::new(&paths).append(&mut meeting.lock().unwrap(), segments);
                            me.changed(); // the live transcript in the main window
                        }
                    }
                }
                (transcriber, kept)
            })
        };

        // Some platforms tie a stream to its thread, so one thread owns both.
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let (started_tx, started_rx) = mpsc::channel::<Result<(), String>>();
        let (mic, system_audio) = (self.platform.mic.clone(), self.platform.system_audio.clone());
        let me = self.me.clone();
        let detectors = vec![("You", self.speech.speech_detector()), ("Others", self.speech.speech_detector())];
        let echo = self.speech.echo_canceller();
        let audio = std::thread::spawn(move || {
            let live = Live::start(detectors, echo, pacer, chunk_tx);
            // A stream broke for good (device unplugged, permission revoked): keep what we have.
            let on_error: OnError = Arc::new(move |message| {
                if let Some(engine) = me.upgrade() {
                    engine.lock().capture_error = Some(message);
                    engine.stop(StopReason::Asked);
                }
            });
            let streams = mic
                .start(live.feed("You"), on_error.clone())
                .and_then(|m| system_audio.start(live.feed("Others"), on_error).map(|s| [m, s]));
            match streams {
                Ok(mut streams) => {
                    let _ = started_tx.send(Ok(()));
                    let _ = stop_rx.recv(); // until stop() or the sender is dropped
                    for stream in streams.iter_mut() {
                        stream.stop(); // hands over what was still queued
                    }
                    drop(streams);
                    live.finish(); // the rest of each stream; the worker's end closes the channel
                }
                Err(e) => {
                    live.finish();
                    let _ = started_tx.send(Err(e));
                }
            }
        });

        if let Err(message) = started_rx.recv().unwrap_or_else(|_| Err("Audio capture did not start.".into())) {
            let _ = audio.join();
            let _ = consumer.join();
            let mut failed = meeting.lock().unwrap().clone();
            failed.state = MeetingState::Failed;
            MeetingStore::new(&paths).save(&failed);
            return Err(format!("Muesli could not start transcribing: {message}"));
        }
        let mut s = self.lock();
        s.session = Some(Session { meeting, stop_audio: stop_tx, audio: Some(audio), consumer: Some(consumer), prompt });
        s.activity = Activity::Transcribing;
        s.selected = Some(id);
        Ok(())
    }

    /// Stops, transcribes what is left and saves, in the background.
    fn stop(&self, reason: StopReason) {
        let session = {
            let mut s = self.lock();
            if s.activity != Activity::Transcribing {
                return;
            }
            s.activity = Activity::Saving;
            if s.prompt.as_ref().is_some_and(|p| p.kind == PromptKind::Stop) {
                if let Some(p) = s.prompt.take() {
                    if p.in_window {
                        drop(s);
                        self.host.close(Window::Prompt);
                        s = self.lock();
                    }
                }
            }
            s.session.take()
        };
        self.changed();
        let Some(mut session) = session else { return };
        let me = self.me();
        std::thread::spawn(move || {
            // Lock order: the engine before a meeting, never the other way round.
            let paths = me.paths();
            {
                let mut m = session.meeting.lock().unwrap();
                m.state = MeetingState::Finalizing;
                MeetingStore::new(&paths).save(&m);
            }
            let _ = session.stop_audio.send(());
            if let Some(audio) = session.audio.take() {
                let _ = audio.join();
            }
            let finished = session.consumer.take().and_then(|consumer| consumer.join().ok()); // the last chunks
            if let Some((transcriber, kept)) = finished {
                me.improve(&session.meeting, transcriber.as_ref(), &kept, &session.prompt);
            }
            let mut m = session.meeting.lock().unwrap().clone();
            MeetingStore::new(&me.paths()).finish(&mut m, me.now());
            {
                let mut s = me.lock();
                s.improving = None;
                s.last_ready = Some(LastReady { id: m.id.clone(), title: m.title.clone(), ended_at: m.ended_at });
                s.activity = match s.capture_error.take() {
                    Some(why) => Activity::Error(format!("Transcribing stopped: {why} The transcript so far was saved.")),
                    None => Activity::Idle,
                };
            }
            if reason == StopReason::NoAnswer {
                let id = m.id.clone();
                let opener = me.me.clone();
                let notice = Notice {
                    title: format!("Saved: {}", m.title),
                    body: "No answer for 5 minutes, so Muesli stopped and saved the transcript.".into(),
                    action: Some("Open".into()),
                };
                let _ = me.platform.notifier.notify(
                    &notice,
                    Box::new(move || {
                        if let Some(engine) = opener.upgrade() {
                            engine.intent(Intent::OpenMeeting { id: id.clone() });
                        }
                    }),
                );
            }
            me.changed();
            me.kick_imports(); // files waited for the recording
        });
    }

    /// The end-of-meeting pass (`final_pass.rs`): transcribes the meeting's speech again and,
    /// unless it is cancelled (Start, Quit) or fails, replaces the live text with the result.
    fn improve(&self, meeting: &Arc<Mutex<Meeting>>, transcriber: &(dyn muesli_import::SpeechToText + Send), kept: &[Kept], prompt: &str) {
        let chunks = final_pass::regroup(kept);
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut s = self.lock();
            if chunks.is_empty() || s.quitting {
                return;
            }
            s.activity = Activity::Improving;
            s.improving = Some(Improving { meeting: meeting.clone(), done: 0, total: chunks.len(), cancel: cancel.clone() });
        }
        self.changed();
        let result = final_pass::run(transcriber, chunks, prompt, &cancel, |done, total| {
            if let Some(i) = self.lock().improving.as_mut() {
                (i.done, i.total) = (done, total);
            }
            self.changed();
        });
        if let Some(mut segments) = result.filter(|s| !s.is_empty()) {
            segments.sort_by(|a, b| a.start.total_cmp(&b.start));
            muesli_core::echo::drop_echoes(&mut segments);
            let paths = self.paths(); // the engine's lock first, then the meeting's
            let mut m = meeting.lock().unwrap();
            m.transcript = segments;
            MeetingStore::new(&paths).save(&m);
        }
    }

    // -----------------------------------------------------------------------------------------
    // Audio files

    fn queue_files(&self, paths: Vec<PathBuf>) {
        {
            let mut s = self.lock();
            for path in paths.into_iter().filter(|p| muesli_core::import::is_audio(p) && p.is_file()) {
                if s.imports.iter().any(|i| i.path == path && !matches!(i.state, ImportState::Failed(_))) {
                    continue;
                }
                let id = s.next_id;
                s.next_id += 1;
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                s.imports.push(Import { id, path, name, state: ImportState::Waiting, cancel: Arc::new(AtomicBool::new(false)) });
            }
        }
        self.host.show(Window::Main); // progress lives in the main window's list
        self.kick_imports();
        self.changed();
    }

    /// Starts the worker if files wait and none runs. One file at a time; a recording goes first,
    /// and nothing starts before the speech model is ready.
    fn kick_imports(&self) {
        {
            let mut s = self.lock();
            if s.worker || !s.imports.iter().any(|i| i.state == ImportState::Waiting) {
                return;
            }
            if s.activity != Activity::Idle && !matches!(s.activity, Activity::Error(_)) {
                return;
            }
            if !self.model_ready() {
                return;
            }
            s.worker = true;
        }
        let me = self.me();
        std::thread::spawn(move || me.work_imports());
    }

    fn work_imports(&self) {
        let mut transcriber = None;
        loop {
            let next = {
                let mut s = self.lock();
                let busy = !matches!(s.activity, Activity::Idle | Activity::Error(_));
                let next = if busy || s.quitting { None } else { s.imports.iter_mut().find(|i| i.state == ImportState::Waiting) };
                match next {
                    Some(item) => {
                        item.state = ImportState::Running { progress: None };
                        Some(item.clone())
                    }
                    None => {
                        s.worker = false;
                        None
                    }
                }
            };
            let Some(item) = next else { break };
            self.changed();
            if transcriber.is_none() {
                let languages = self.lock().prefs.effective_languages(&self.options.system_languages);
                match self.speech.transcriber(languages) {
                    Ok(t) => transcriber = Some(t),
                    Err(why) => {
                        self.set_import(item.id, Some(ImportState::Failed(why)));
                        continue;
                    }
                }
            }
            let stt = transcriber.as_deref().unwrap();
            let me = self.me();
            let id = item.id;
            let result = muesli_import::import_file(&item.path, stt, &self.paths(), &item.cancel, move |progress| {
                me.set_import(id, Some(ImportState::Running { progress }));
            });
            match result {
                Ok(m) => {
                    self.lock().last_ready = Some(LastReady { id: m.id.clone(), title: m.title.clone(), ended_at: m.ended_at });
                    self.set_import(id, None);
                }
                Err(muesli_import::ImportError::Cancelled) => {
                    // Cancel means no transcript. Only when Muesli quits does what was done stay,
                    // so the import carries on at the next start.
                    if !self.lock().quitting {
                        muesli_import::discard_partial(&item.path, &self.paths());
                    }
                    self.set_import(id, None);
                }
                Err(e) => self.set_import(id, Some(ImportState::Failed(e.to_string()))),
            }
        }
        self.changed();
    }

    /// Updates an import, or removes it (`None`).
    fn set_import(&self, id: u64, state: Option<ImportState>) {
        {
            let mut s = self.lock();
            match state {
                Some(state) => {
                    if let Some(item) = s.imports.iter_mut().find(|i| i.id == id) {
                        // Progress that arrives after Cancel does not undo "Cancelling…".
                        if !(item.state == ImportState::Cancelling && matches!(state, ImportState::Running { .. })) {
                            item.state = state;
                        }
                    }
                }
                None => s.imports.retain(|i| i.id != id),
            }
        }
        self.changed();
    }

    // -----------------------------------------------------------------------------------------
    // Finished meetings

    fn editor<'a>(&'a self, paths: &'a Paths) -> MeetingEditor<'a> {
        let trash = self.platform.trash.clone();
        MeetingEditor::new(paths, paths.transcripts.clone()).with_discard(move |p| {
            if p.exists() && trash.trash(p).is_err() {
                let _ = std::fs::remove_file(p);
            }
        })
    }

    fn rename(&self, id: &str, title: &str) {
        let paths = self.paths();
        if let Err(e) = self.editor(&paths).rename(id, title) {
            self.toast(e.message(), false);
        }
        self.changed();
    }

    fn delete(&self, id: &str) {
        let paths = self.paths();
        let Some(meeting) = MeetingStore::new(&paths).load(id) else { return };
        if !matches!(meeting.state, MeetingState::Ready | MeetingState::Failed) {
            return; // only finished meetings
        }
        let previous = {
            let mut s = self.lock();
            let previous = s.pending_delete.replace(PendingDelete { id: id.to_string(), at: self.now() });
            if s.selected.as_deref() == Some(id) {
                s.selected = None;
            }
            previous
        };
        if let Some(previous) = previous {
            self.commit_delete(&previous); // one undo at a time
        }
        self.toast(format!("“{}” moved to the {}", shorten_title(&meeting.title, 40), self.platform.trash.name()), true);
        self.changed();
    }

    fn commit_delete(&self, delete: &PendingDelete) {
        let paths = self.paths();
        let _ = self.editor(&paths).delete(&delete.id);
        let mut s = self.lock();
        if s.last_ready.as_ref().is_some_and(|r| r.id == delete.id) {
            s.last_ready = None;
        }
    }

    fn toast(&self, text: String, undo: bool) {
        self.lock().toast = Some(ToastState { text, undo, at: self.now() });
    }

    /// The Markdown file of a finished meeting, if it is there.
    fn exported_file(&self, id: &str) -> Option<PathBuf> {
        let paths = self.paths();
        let meeting = MeetingStore::new(&paths).load(id)?;
        let file = paths.transcripts.join(format!("{}.md", Store::file_stem(&meeting)));
        file.exists().then_some(file)
    }

    // -----------------------------------------------------------------------------------------
    // AI assistants

    /// Records where `muesli-mcp` is (for the Claude plugin) and brings Claude Desktop's config
    /// file in line with the preferences.
    fn sync_assistants(&self) {
        let Some(helper) = self.host.helper_path() else {
            self.lock().claude_status = Some("muesli-mcp is missing next to the app.".into());
            return;
        };
        let paths = self.paths();
        let _ = muesli_core::assistants::record_helper_path(&paths, &helper);
        let prefs = self.preferences();
        let dir = ClaudeDesktopConfig::default_directory(&*self.platform.paths);
        let status = ClaudeDesktopConfig::sync(&prefs, &helper, self.extension_state().is_installed(), &dir);
        self.lock().claude_status = Some(status).filter(|s| !s.is_empty());
    }

    /// Whether Claude Desktop has an older Muesli extension than the one in this app, so an update
    /// is ready. The update itself goes through Claude: opening the bundled extension shows
    /// Claude's update dialog. Claude keeps its own record of what it installed, so Muesli never
    /// replaces the extension's files itself.
    fn extension_update_ready(&self) -> bool {
        self.preferences().mcp_enabled
            && self.host.extension_path().is_some()
            && matches!(self.extension_state(), ExtensionState::Outdated { .. })
    }

    /// At start: a system notification with Update, once per Muesli version. Muesli's window shows
    /// a banner as well (`extension_update` in the main view) until the update is done or put away.
    fn announce_extension_update(&self) {
        if !self.extension_update_ready() || self.preferences().extension_update_notified.as_deref() == Some(VERSION) {
            return;
        }
        self.save_prefs(|p| p.extension_update_notified = Some(VERSION.into()));
        let opener = self.me.clone();
        let notice = Notice {
            title: "Claude Desktop extension update".into(),
            body: "A newer Muesli extension for Claude Desktop is ready.".into(),
            action: Some("Update".into()),
        };
        let _ = self.platform.notifier.notify(
            &notice,
            Box::new(move || {
                if let Some(engine) = opener.upgrade() {
                    engine.intent(Intent::ConnectClaudeDesktop);
                }
            }),
        );
    }

    fn extension_state(&self) -> ExtensionState {
        let dir = ClaudeDesktopConfig::default_directory(&*self.platform.paths);
        ClaudeDesktopExtension::state_with(VERSION, &dir, Some(self.host.claude_installed()))
    }

    // -----------------------------------------------------------------------------------------
    // Intents

    pub fn intent(&self, intent: Intent) {
        match intent {
            Intent::StartTranscribing => self.start_transcribing(DetectedMeeting::new("Meeting", "manual")),
            Intent::StopAndSave => self.stop(StopReason::Asked),
            Intent::AnswerPrompt { prompt, accept } => self.answer(prompt, accept),

            // Until setup is done, opening Muesli (again) shows setup, not an empty main window.
            Intent::OpenMuesli => {
                let window = if self.lock().prefs.onboarding_done { Window::Main } else { Window::Setup };
                self.host.show(window);
            }
            Intent::OpenSettings => self.host.show(Window::Settings),
            Intent::Tray { id } => self.tray(&id),
            Intent::OpenMeeting { id } => {
                self.lock().selected = Some(id);
                self.host.show(Window::Main);
                self.changed();
            }
            Intent::Quit => {
                self.shutdown();
                self.install_on_quit();
                self.host.quit();
            }

            Intent::ChooseAudioFiles => {
                if self.model_ready() {
                    self.host.pick_audio_files();
                }
            }
            Intent::TranscribeFiles { paths } => self.queue_files(paths),
            Intent::CancelImport { id } => {
                {
                    let mut s = self.lock();
                    if let Some(item) = s.imports.iter_mut().find(|i| i.id == id) {
                        item.cancel.store(true, Ordering::SeqCst);
                        if matches!(item.state, ImportState::Running { .. }) {
                            item.state = ImportState::Cancelling; // removed by the worker once it has stopped
                        }
                    }
                    s.imports.retain(|i| i.id != id || matches!(i.state, ImportState::Running { .. } | ImportState::Cancelling));
                }
                self.changed();
            }
            Intent::DismissImport { id } => {
                self.lock().imports.retain(|i| i.id != id || !matches!(i.state, ImportState::Failed(_)));
                self.changed();
            }

            Intent::Select { id } => {
                self.lock().selected = Some(id);
                self.host.changed();
            }
            Intent::Rename { id, title } => self.rename(&id, &title),
            Intent::Delete { id } => self.delete(&id),
            Intent::Undo => {
                let mut s = self.lock();
                if let Some(d) = s.pending_delete.take() {
                    s.selected = Some(d.id);
                }
                s.toast = None;
                drop(s);
                self.changed();
            }
            Intent::DismissToast => {
                let pending = {
                    let mut s = self.lock();
                    s.toast = None;
                    s.pending_delete.take()
                };
                if let Some(d) = pending {
                    self.commit_delete(&d);
                }
                self.changed();
            }
            Intent::ShowInFileManager { id } => {
                let target = id.and_then(|id| self.exported_file(&id)).unwrap_or_else(|| {
                    let folder = self.paths().transcripts;
                    let _ = std::fs::create_dir_all(&folder);
                    folder
                });
                self.host.reveal(&target);
            }
            Intent::DismissError => {
                let mut s = self.lock();
                if matches!(s.activity, Activity::Error(_)) {
                    s.activity = Activity::Idle;
                }
                drop(s);
                self.changed();
                self.kick_imports();
            }

            Intent::ChooseTranscriptsFolder => self.host.pick_transcripts_folder(),
            Intent::SetTranscriptsFolder { path } => {
                self.save_prefs(|p| p.transcripts_folder = Some(path.to_string_lossy().into_owned()));
                let paths = self.paths();
                let _ = std::fs::create_dir_all(&paths.transcripts);
                MeetingStore::new(&paths).import_transcripts(&paths.transcripts);
                self.changed();
            }
            Intent::SetStartAtLogin { on } => {
                let _ = self.platform.login_item.set_enabled(on);
                self.changed();
            }
            Intent::AddLanguage { code } => {
                let defaults = self.options.system_languages.clone();
                self.save_prefs(|p| p.add_language(&code, &defaults));
                self.changed();
            }
            Intent::RemoveLanguage { code } => {
                let defaults = self.options.system_languages.clone();
                self.save_prefs(|p| p.remove_language(&code, &defaults));
                self.changed();
            }
            Intent::ResetLanguages => {
                self.save_prefs(|p| p.reset_languages());
                self.changed();
            }
            Intent::AddTerm { term } => {
                let paths = self.paths();
                let mut vocabulary = Vocabulary::load(&paths);
                let result = vocabulary.add(&[term]).and_then(|_| vocabulary.save(&paths).map_err(|e| e.to_string()));
                self.lock().vocabulary_error = result.err();
                self.changed();
            }
            Intent::RemoveTerm { term } => {
                let paths = self.paths();
                let mut vocabulary = Vocabulary::load(&paths);
                vocabulary.remove(&[term]);
                let _ = vocabulary.save(&paths);
                self.lock().vocabulary_error = None;
                self.changed();
            }
            Intent::SetAssistantsEnabled { on } => {
                self.save_prefs(|p| p.mcp_enabled = on);
                self.sync_assistants();
                self.changed();
            }
            Intent::DisconnectClaudeDesktop => {
                if self.extension_state().is_installed() {
                    // Claude Desktop alone can uninstall its extension: open it and say where.
                    self.lock().claude_disconnecting = true;
                    self.host.open_url("claude://");
                } else {
                    // Connected through its settings file: Muesli wrote that entry, so it removes it.
                    self.save_prefs(|p| p.claude_desktop_config_entry = false);
                    self.sync_assistants();
                }
                self.changed();
            }
            Intent::ConnectClaudeDesktop => {
                self.lock().claude_disconnecting = false;
                if let Some(path) = self.host.extension_path() {
                    self.host.open(&path);
                }
            }
            Intent::DismissExtensionUpdate => {
                self.save_prefs(|p| p.extension_update_dismissed = Some(VERSION.into()));
                self.changed();
            }
            Intent::AddToClaudeConfig => {
                self.save_prefs(|p| p.claude_desktop_config_entry = true);
                self.sync_assistants();
                self.changed();
            }
            Intent::PickAssistant { name } => {
                if AssistantClient::ALL.iter().any(|c| c.name() == name) {
                    self.lock().assistant = name;
                    self.host.changed();
                }
            }
            Intent::CheckForUpdates => self.check_for_updates(true),
            Intent::RestartToUpdate => self.restart_to_update(),
            Intent::DismissUpdate => {
                self.lock().update.banner_hidden = true;
                self.host.changed();
            }
            Intent::OpenReleaseNotes { version } => self.open_release_notes(&version),
            Intent::SetAutoUpdateCheck { on } => {
                self.save_prefs(|p| p.auto_update_check = on);
                self.host.changed();
            }
            Intent::DownloadModelAgain => {
                self.download_model();
                self.changed();
            }
            Intent::RequestPermission { permission } => {
                let me = self.me();
                // The system's question can block; never on the caller's thread.
                std::thread::spawn(move || {
                    let permissions = &me.platform.permissions;
                    let asked = permissions.request(permission);
                    if asked.is_err() || !matches!(permissions.status(permission), Access::Allowed | Access::NotNeeded) {
                        let _ = permissions.open_settings(permission);
                    }
                    me.changed();
                });
            }
            Intent::OpenPermissionSettings { permission } => {
                let _ = self.platform.permissions.open_settings(permission);
            }

            Intent::SetupNext => {
                let (step, offer) = {
                    let mut s = self.lock();
                    s.setup_step = (s.setup_step + 1).min(SETUP_DONE);
                    let offer = s.setup_step == 3 && !s.assistants_offered;
                    s.assistants_offered |= offer;
                    (s.setup_step, offer)
                };
                if step == 1 {
                    self.download_model(); // "Get started": the model downloads during setup
                }
                if offer {
                    // The AI assistants step starts with the switch on; once, so Back and
                    // Continue keep what the user chose.
                    self.save_prefs(|p| p.mcp_enabled = true);
                    self.sync_assistants();
                }
                self.changed();
            }
            Intent::SetupBack => {
                let mut s = self.lock();
                s.setup_step = s.setup_step.saturating_sub(1);
                drop(s);
                self.host.changed();
            }
            Intent::FinishSetup => {
                let after_update = std::mem::take(&mut self.lock().update.permissions_after_update);
                if after_update {
                    // Only the permissions were asked again; Muesli was set up already.
                    self.lock().setup_step = 0;
                    self.host.close(Window::Setup);
                    self.changed();
                    return;
                }
                let version = self.running_version();
                self.save_prefs(|p| {
                    p.onboarding_done = true;
                    p.last_run_version = Some(version);
                });
                self.host.close(Window::Setup);
                self.host.show(Window::Main);
                self.download_model();
                self.changed();
            }
        }
    }

    fn tray(&self, id: &str) {
        match id {
            "update" => self.intent(Intent::RestartToUpdate),
            "start" => self.intent(Intent::StartTranscribing),
            "stop" => self.intent(Intent::StopAndSave),
            "open" => self.intent(Intent::OpenMuesli),
            "transcribe_file" => self.intent(Intent::ChooseAudioFiles),
            "settings" => self.intent(Intent::OpenSettings),
            "dismiss" => self.intent(Intent::DismissError),
            "quit" => self.intent(Intent::Quit),
            other => {
                if let Some(id) = other.strip_prefix("recent:") {
                    self.intent(Intent::OpenMeeting { id: id.to_string() });
                }
            }
        }
    }

    /// Before quitting: a pending delete is carried out, a transcription is stopped and saved.
    pub fn shutdown(&self) {
        let pending = {
            let mut s = self.lock();
            s.quitting = true;
            for item in &s.imports {
                item.cancel.store(true, Ordering::SeqCst);
            }
            s.pending_delete.take()
        };
        if let Some(d) = pending {
            self.commit_delete(&d);
        }
        if self.lock().activity == Activity::Transcribing {
            self.stop(StopReason::Asked);
        }
        if let Some(i) = self.lock().improving.as_ref() {
            i.cancel.store(true, Ordering::SeqCst); // the live text is saved as it is
        }
        // Wait (briefly) for the transcript to be saved.
        for _ in 0..600 {
            if !matches!(self.lock().activity, Activity::Saving | Activity::Starting | Activity::Improving) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }

    // -----------------------------------------------------------------------------------------
    // Views

    /// The tray menu (menu bar on macOS, notification area on Windows).
    pub fn menu(&self) -> TrayMenu {
        let update_items = self.update_menu_items();
        let update_dot = self.update_dot();
        let model = self.speech.status();
        let paths = self.paths();
        let recent: Vec<Meeting> = {
            let hidden = self.lock().pending_delete.as_ref().map(|d| d.id.clone());
            MeetingStore::new(&paths)
                .all()
                .into_iter()
                .filter(|m| m.state == MeetingState::Ready && Some(&m.id) != hidden.as_ref())
                .take(RECENT_IN_MENU)
                .collect()
        };
        let s = self.lock();
        let action = |id: &str, label: &str, enabled: bool| TrayItem::Action { id: id.into(), label: label.into(), enabled, detail: None };
        let now = self.now();
        let mut since = None;
        let label = |text: String| TrayItem::Label { label: text };
        let mut items = update_items;
        let ready = model.is_ready();
        let setup_done = s.prefs.onboarding_done;
        if !ready {
            items.push(label("Getting Ready".into()));
            items.push(label(match &model {
                ModelStatus::Downloading { .. } => {
                    format!("Downloading the speech model · {}%", (model.fraction().unwrap_or(0.0) * 100.0).round() as u32)
                }
                ModelStatus::Failed { .. } => "The speech model could not be downloaded".into(),
                _ => "The speech model is not downloaded yet".into(),
            }));
            items.push(TrayItem::Separator);
        }
        match &s.activity {
            Activity::Transcribing => {
                let title = s.session.as_ref().map(|x| x.meeting.lock().unwrap().title.clone()).unwrap_or_default();
                let started = s.session.as_ref().map(|x| x.meeting.lock().unwrap().started_at);
                since = started;
                items.push(label(format!("Transcribing · {}", shorten_title(&title, 40))));
                items.push(TrayItem::Action { id: "stop".into(), label: "Stop and Save".into(), enabled: true, detail: None });
            }
            Activity::Starting => items.push(label("Starting…".into())),
            Activity::Saving => items.push(label("Saving the transcript…".into())),
            Activity::Improving => {
                items.push(label(format!("Improving the transcript… {}", improving_percent(s.improving.as_ref()))));
                items.push(action("start", "Start Transcribing", ready && setup_done));
            }
            Activity::Error(message) => {
                items.push(label(shorten_title(message, 90)));
                items.push(action("dismiss", "Dismiss", true));
                items.push(action("start", "Start Transcribing", ready && setup_done));
            }
            Activity::Idle => items.push(action("start", "Start Transcribing", ready && setup_done)),
        }
        items.push(TrayItem::Separator);
        items.push(action("open", "Open Muesli", true));
        items.push(action("transcribe_file", "Transcribe Audio File…", ready));
        if !recent.is_empty() {
            items.push(TrayItem::Separator);
            items.push(label("Recent Transcripts".into()));
            for m in &recent {
                items.push(TrayItem::Action {
                    id: format!("recent:{}", m.id),
                    label: shorten_title(&m.title, 44),
                    enabled: true,
                    detail: Some(menu_subtitle_local(m.started_at, m.ended_at, now)),
                });
            }
        }
        items.push(TrayItem::Separator);
        items.push(action("settings", "Settings…", true));
        items.push(action("quit", "Quit Muesli", true));
        let transcribing = matches!(s.activity, Activity::Transcribing | Activity::Saving);
        TrayMenu {
            items,
            transcribing,
            tooltip: if transcribing {
                "Muesli · transcribing".into()
            } else if update_dot {
                "Muesli · update ready".into()
            } else {
                "Muesli".into()
            },
            transcribing_since: if transcribing { since } else { None },
            update_ready: update_dot && !transcribing,
        }
    }

    /// One window's view state as JSON, for the web UI.
    pub fn view(&self, window: Window) -> serde_json::Value {
        fn json<T: Serialize>(v: T) -> serde_json::Value {
            serde_json::to_value(v).unwrap_or(serde_json::Value::Null)
        }
        match window {
            Window::Main => json(self.main_view()),
            Window::Settings => json(self.settings_view()),
            Window::Setup => json(self.setup_view()),
            Window::Prompt => json(self.prompt_view()),
        }
    }

    pub fn prompt_view(&self) -> Option<PromptView> {
        let s = self.lock();
        let p = s.prompt.as_ref()?;
        Some(PromptView {
            words: self.platform.words.clone(),
            id: p.id,
            title: p.question.title.clone(),
            body: p.question.body.clone(),
            accept: p.question.accept.clone(),
            decline: p.question.decline.clone(),
        })
    }

    pub fn main_view(&self) -> MainView {
        let paths = self.paths();
        let ready = self.model_ready();
        let extension_update = (self.extension_update_ready() && self.preferences().extension_update_dismissed.as_deref() != Some(VERSION))
            .then(|| "A newer Muesli extension for Claude Desktop is ready.".to_string());
        let update = self.update_banner();
        let all = MeetingStore::new(&paths).all();
        let s = self.lock();
        let live_meeting = s
            .session
            .as_ref()
            .map(|x| x.meeting.lock().unwrap().clone())
            .or_else(|| s.improving.as_ref().map(|i| i.meeting.lock().unwrap().clone()));
        let improving = s.activity == Activity::Improving;
        let hidden = s.pending_delete.as_ref().map(|d| d.id.clone());
        let busy_ids: Vec<String> = live_meeting.iter().map(|m| m.id.clone()).collect();
        let listed: Vec<&Meeting> = all
            .iter()
            .filter(|m| Some(&m.id) != hidden.as_ref() && !busy_ids.contains(&m.id))
            // Meetings still being written (an import running, a recording being saved) are not listed yet.
            .filter(|m| matches!(m.state, MeetingState::Ready | MeetingState::Failed))
            .collect();
        let selected = s
            .selected
            .clone()
            .filter(|id| Some(id) != hidden.as_ref())
            .filter(|id| busy_ids.contains(id) || listed.iter().any(|m| &m.id == id))
            .or_else(|| busy_ids.first().cloned())
            .or_else(|| listed.first().map(|m| m.id.clone()));

        let live = live_meeting.as_ref().map(|m| LiveRow {
            id: m.id.clone(),
            title: m.title.clone(),
            started_at: iso(m.started_at),
            selected: selected.as_ref() == Some(&m.id),
            status: match s.activity {
                Activity::Saving => "Saving…".into(),
                Activity::Improving => format!("Improving… {}", improving_percent(s.improving.as_ref())),
                _ => "Transcribing".into(),
            },
        });
        let imports = s
            .imports
            .iter()
            .map(|i| ImportRow {
                id: i.id,
                name: i.name.clone(),
                status: match &i.state {
                    ImportState::Waiting if !ready => "Waiting for the speech model".into(),
                    ImportState::Waiting if live.is_some() => "Waiting for the meeting to end".into(),
                    ImportState::Waiting => "Waiting".into(),
                    ImportState::Running { .. } => "Transcribing".into(),
                    ImportState::Cancelling => "Cancelling…".into(),
                    ImportState::Failed(why) => why.clone(),
                },
                progress: match i.state {
                    ImportState::Running { progress } => progress,
                    _ => None,
                },
                running: matches!(i.state, ImportState::Running { .. }),
                cancelling: i.state == ImportState::Cancelling,
                failed: matches!(i.state, ImportState::Failed(_)),
            })
            .collect();
        let meetings = listed
            .iter()
            .map(|m| MeetingRow {
                id: m.id.clone(),
                title: m.title.clone(),
                started_at: iso(m.started_at),
                duration_minutes: minutes(m.started_at, m.ended_at),
                source: source_label(&m.source),
                selected: selected.as_ref() == Some(&m.id),
                partial: m.partial == Some(true),
                failed: m.state == MeetingState::Failed,
            })
            .collect();
        let reader = selected.as_ref().and_then(|id| {
            let live = live_meeting.as_ref().filter(|m| &m.id == id);
            let m = live.or_else(|| listed.iter().copied().find(|m| &m.id == id))?;
            // A stopped meeting whose text is being improved is read-only, but no longer live.
            let busy = live.is_some();
            let is_live = busy && !improving;
            Some(Reader {
                id: m.id.clone(),
                title: m.title.clone(),
                started_at: iso(m.started_at),
                duration_minutes: if busy {
                    Some(((self.now() - m.started_at).num_seconds() / 60).max(0))
                } else {
                    minutes(m.started_at, m.ended_at)
                },
                source: source_label(&m.source),
                attendees: m.attendees.clone(),
                lines: lines(m),
                live: is_live,
                badge: if improving && busy {
                    Some(format!("Improving the transcript · {}", improving_percent(s.improving.as_ref())))
                } else {
                    is_live.then(|| "Transcribing · live, read-only".to_string())
                },
                editable: !busy,
                empty_text: m.transcript.is_empty().then(|| {
                    if is_live {
                        "Text appears here a few seconds after each sentence while Muesli transcribes.".to_string()
                    } else {
                        "Nothing was said, or nothing could be transcribed.".to_string()
                    }
                }),
            })
        });
        let words = self.platform.words.clone();
        let empty = live.is_none() && s.imports.is_empty() && listed.is_empty();
        MainView {
            show_in_file_manager: format!("Show in {}", words.file_manager),
            words,
            live,
            imports,
            meetings,
            reader,
            toast: s.toast.as_ref().map(|t| Toast { text: t.text.clone(), undo: t.undo }),
            error: match &s.activity {
                Activity::Error(message) => Some(message.clone()),
                _ => None,
            },
            can_transcribe_file: ready,
            can_start: ready && s.prefs.onboarding_done && matches!(s.activity, Activity::Idle | Activity::Improving),
            transcribing: matches!(s.activity, Activity::Transcribing),
            empty_text: empty.then(|| "No transcripts yet. When a meeting starts, Muesli asks whether to transcribe it.".to_string()),
            extension_update,
            update,
        }
    }

    fn languages_view(&self) -> LanguagesView {
        let s = self.lock();
        let chosen = s.prefs.effective_languages(&self.options.system_languages);
        let chip = |code: &str| LanguageChip { code: code.to_string(), name: languages::english_name(code).to_string() };
        let mut available: Vec<LanguageChip> =
            languages::SUPPORTED.iter().filter(|c| !chosen.iter().any(|x| x == *c)).map(|c| chip(c)).collect();
        available.sort_by(|a, b| a.name.cmp(&b.name));
        LanguagesView {
            can_reset: chosen != self.options.system_languages,
            can_remove: chosen.len() > 1,
            chosen: chosen.iter().map(|c| chip(c)).collect(),
            available,
        }
    }

    fn model_view(&self) -> ModelView {
        let status = self.speech.status();
        let (text, error, url) = match &status {
            ModelStatus::Ready => ("Ready".to_string(), None, None),
            ModelStatus::Downloading { .. } => {
                (format!("Downloading · {}%", (status.fraction().unwrap_or(0.0) * 100.0).round() as u32), None, None)
            }
            ModelStatus::Failed { message, url } => ("Download failed".to_string(), Some(message.clone()), url.clone()),
            ModelStatus::Missing => ("Not downloaded".to_string(), None, None),
        };
        let computer = self.platform.words.computer;
        let detail = format!("Understands your languages, on this {computer}");
        let tone = match status {
            ModelStatus::Ready => "ok",
            ModelStatus::Failed { .. } => "warn",
            _ => "",
        };
        let mut features = vec![FeatureRow {
            id: "speech",
            title: "Speech recognition".into(),
            detail: detail.clone(),
            status: text.clone(),
            tone,
            progress: status.fraction(),
            action: match status {
                ModelStatus::Failed { .. } => Some("Try again"),
                ModelStatus::Missing => Some("Download"),
                _ => None,
            },
        }];
        if let Some(speed) = self.speech.speed_up() {
            features.push(speed_row(&speed));
        }
        features.push(if self.speech.echo_removal() {
            FeatureRow {
                id: "echo",
                title: "Works without headphones".into(),
                detail: "Removes the call's echo from your microphone".into(),
                status: "On".into(),
                tone: "ok",
                progress: None,
                action: None,
            }
        } else {
            FeatureRow {
                id: "echo",
                title: "Works without headphones".into(),
                detail: "Not available, so use headphones in calls".into(),
                status: "Off".into(),
                tone: "",
                progress: None,
                action: None,
            }
        });
        ModelView {
            title: "Speech model".into(),
            detail,
            about: format!("Whisper {} · {} MB", DEFAULT_MODEL.title, DEFAULT_MODEL.approximate_bytes / 1_000_000),
            progress: status.fraction(),
            status,
            text,
            error,
            url,
            features,
        }
    }

    /// The permissions this platform has, required ones first.
    pub fn permission_rows(&self) -> Vec<PermissionRow> {
        let words = &self.platform.words;
        let settings_button = if words.skin == "mac" { "Open System Settings…" } else { "Open Settings…" };
        Permission::ALL
            .into_iter()
            .filter_map(|p| {
                let access = self.platform.permissions.status(p);
                if access == Access::NotNeeded {
                    return None;
                }
                let (title, detail, required) = match p {
                    Permission::Microphone => ("Microphone", "Your side of the conversation", true),
                    Permission::SystemAudio => ("System audio", "The other side of the call. Never your screen or video.", true),
                    Permission::Calendar => ("Calendar", "Names meetings and notices them as they start", false),
                    Permission::Notifications => {
                        ("Notifications", "Asks before starting and stopping. Without them, a small window asks.", false)
                    }
                };
                Some(PermissionRow {
                    permission: p,
                    title: title.into(),
                    detail: detail.into(),
                    required,
                    access,
                    badge: match access {
                        Access::Allowed | Access::NotNeeded => "Allowed",
                        Access::Denied => "Not allowed",
                        Access::NotAsked => "Not asked yet",
                        Access::Unknown => "Unknown",
                    }
                    .into(),
                    granted: access.is_granted(),
                    button: match access {
                        Access::NotAsked => Some("Allow…".into()),
                        Access::Denied | Access::Unknown => Some(settings_button.into()),
                        _ => None,
                    },
                })
            })
            .collect()
    }

    pub fn setup_view(&self) -> SetupView {
        let words = self.platform.words.clone();
        let (step, after_update) = {
            let s = self.lock();
            (s.setup_step, s.update.permissions_after_update)
        };
        let permissions = self.permission_rows();
        let assistants = (step == 3 && !after_update).then(|| self.assistants_view());
        let required_ok = permissions.iter().filter(|p| p.required).all(|p| p.granted);
        let windows_note = self.platform.permissions.status(Permission::SystemAudio) == Access::NotNeeded;
        let (title, subtitle, primary, primary_enabled) = match step {
            0 => (
                "Meet Muesli".to_string(),
                Some("Granola’s raw, unbaked cousin. Three quick steps, and Muesli is ready for your next meeting.".to_string()),
                "Get started",
                true,
            ),
            1 => (
                "Which languages are spoken in your meetings?".to_string(),
                Some("Muesli listens for these.".to_string()),
                "Continue",
                true,
            ),
            3 => (
                "Use Muesli with your AI assistant".to_string(),
                Some("Ask Claude or another assistant about your meetings: summaries, decisions, action items.".to_string()),
                "Continue",
                true,
            ),
            2 => (
                "Allow Muesli to listen".to_string(),
                Some(if windows_note {
                    "Muesli needs the microphone. Notifications are optional.".to_string()
                } else {
                    "Muesli needs the microphone and the call audio. Calendar and notifications are optional.".to_string()
                }),
                "Continue",
                required_ok,
            ),
            _ => (
                format!("Muesli is in your {}", words.tray),
                Some(
                    "When a meeting starts, Muesli asks whether to transcribe it. Your transcripts appear in the Muesli window."
                        .to_string(),
                ),
                "Open Muesli",
                true,
            ),
        };
        let mut view = SetupView {
            counter: match step {
                0 => String::new(),
                SETUP_DONE => "All set".into(),
                n => format!("Step {n} of 3"),
            },
            finish: step == SETUP_DONE,
            step,
            steps: SETUP_DONE + 1,
            title,
            subtitle,
            promises: if step == 0 {
                vec![
                    format!("Runs entirely on this {}", words.computer),
                    "Keeps the text, never the audio".to_string(),
                    "No bot in your calls".to_string(),
                ]
            } else {
                Vec::new()
            },
            promise_details: if step == 0 {
                vec![
                    "Speech becomes text locally with Whisper. No account, no sign-up.".to_string(),
                    "Each meeting becomes a Markdown and a JSON file in a folder you choose.".to_string(),
                    "Nobody sees a transcriber join. Muesli asks before it starts.".to_string(),
                ]
            } else {
                Vec::new()
            },
            languages: self.languages_view(),
            hint: match step {
                1 => Some(format!(
                    "Taken from your {}'s settings. Add the others your meetings use; the fewer, the fewer mistakes.",
                    words.computer
                )),
                2 if words.skin == "mac" => Some("macOS asks you to confirm each one. You can change them later in Settings.".to_string()),
                _ => None,
            },
            permissions,
            permissions_note: windows_note
                .then(|| "Call audio needs no permission on Windows. The microphone switch is in Privacy & security.".to_string()),
            model: (step > 0).then(|| self.model_view()),
            assistants: assistants.clone(),
            claude_desktop_missing: (step == 3 && !self.host.claude_installed())
                .then(|| "Claude Desktop is not on this computer. Install it later and connect it in Settings.".to_string()),
            // Skip while assistants are on but Claude Desktop is not connected (off: Continue says it).
            can_skip: assistants.as_ref().is_some_and(|a| a.enabled && !a.claude_desktop.as_ref().is_some_and(|c| c.status == "Connected")),
            notice: (step == SETUP_DONE).then(|| {
                "Please tell the people you meet with that you are transcribing, and check your company's policy first.".to_string()
            }),
            primary: primary.into(),
            primary_enabled,
            can_go_back: step > 0 && step < SETUP_DONE,
            words,
        };
        if after_update {
            // Only the permissions screen, after an update made macOS ask again (UP7).
            view.counter = String::new();
            view.subtitle = Some("Muesli was updated, and macOS asks for these permissions again.".into());
            view.hint = None;
            view.model = None;
            view.can_go_back = false;
            view.can_skip = false;
            view.finish = true;
            view.steps = 1;
        }
        view
    }

    /// AI assistants, for Settings and the setup step: the Claude Desktop row (when it is
    /// installed and assistants are on) and the other apps' instructions.
    fn assistants_view(&self) -> AssistantsView {
        let claude_installed = self.host.claude_installed();
        let extension = self.extension_state();
        let helper = self.host.helper_path();
        let (prefs, assistant, claude_status, disconnecting) = {
            let s = self.lock();
            (s.prefs.clone(), s.assistant.clone(), s.claude_status.clone(), s.claude_disconnecting)
        };
        let through_file = !extension.is_installed()
            && prefs.claude_desktop_config_entry
            && helper
                .as_ref()
                .is_some_and(|h| ClaudeDesktopConfig::is_registered(h, &ClaudeDesktopConfig::default_directory(&*self.platform.paths)));
        let extension_path = self.host.extension_path().is_some();
        let client = AssistantClient::ALL.into_iter().find(|c| c.name() == assistant).unwrap_or(AssistantClient::ClaudeCode);
        // Terminal or PowerShell, and how to quote, follow the platform's words, not the build.
        let windows = self.platform.words.skin == "windows";
        AssistantsView {
            enabled: prefs.mcp_enabled,
            privacy: "Assistants read transcripts on this computer. The text an assistant reads is sent to its provider (Anthropic, for Claude) like anything else you share with it.".into(),
            claude_desktop: (prefs.mcp_enabled && claude_installed).then(|| claude_desktop_row(&extension, through_file, extension_path, claude_status.as_deref(), disconnecting)),
            assistants: AssistantClient::ALL
                .into_iter()
                .filter(|c| *c != AssistantClient::Other)
                .map(|c| AssistantChoice { name: c.name().into(), selected: c == client })
                .collect(),
            instructions: client.instructions_for(windows),
            command: helper.as_ref().map(|h| client.snippet_for(&h.to_string_lossy(), windows)),
            command_is_terminal: client.is_terminal_command(),
            can_add_to_claude_config: prefs.mcp_enabled
                && claude_installed
                && !prefs.claude_desktop_config_entry
                && !extension.is_installed(),
        }
    }

    pub fn settings_view(&self) -> SettingsView {
        let paths = self.paths();
        let vocabulary_error = self.lock().vocabulary_error.clone();
        let assistants = self.assistants_view();
        SettingsView {
            words: self.platform.words.clone(),
            version: self.running_version(),
            transcripts_folder: paths.transcripts.to_string_lossy().into_owned(),
            transcripts_folder_locked: paths.transcripts_set_by_environment(),
            start_at_login: self.platform.login_item.is_enabled(),
            languages: self.languages_view(),
            vocabulary: VocabularyView {
                terms: Vocabulary::load(&paths).terms,
                error: vocabulary_error,
                max_length: muesli_core::vocabulary::MAX_TERM_LENGTH,
            },
            assistants,
            model: self.model_view(),
            permissions: self.permission_rows(),
            updates: self.updates_view(),
        }
    }
}

/// Settings' "Extra speed" line.
fn speed_row(speed: &SpeedUp) -> FeatureRow {
    let uses = format!("Uses {}", speed.by);
    let (detail, status, tone, progress) = match speed.stage {
        SpeedUpStage::Waiting => (format!("{uses}, once speech recognition is downloaded"), "Waiting".to_string(), "", None),
        SpeedUpStage::Downloading(fraction) => {
            let status = match fraction {
                Some(f) => format!("Downloading · {}%", (f * 100.0).round() as u32),
                None => "Downloading".to_string(),
            };
            (uses, status, "", Some(fraction.unwrap_or(0.0)))
        }
        SpeedUpStage::Preparing => (format!("{uses}. Setting up, once, about a minute"), "Setting up".to_string(), "", None),
        SpeedUpStage::Ready => (uses, "Ready".to_string(), "ok", None),
        SpeedUpStage::Failed => (format!("{uses}. You can transcribe without it."), "Download failed".to_string(), "warn", None),
    };
    let action = matches!(speed.stage, SpeedUpStage::Failed).then_some("Try again");
    FeatureRow { id: "speed", title: "Extra speed".into(), detail, status, tone, progress, action }
}
