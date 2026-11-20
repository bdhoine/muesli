//! One simulated computer: a temporary home folder, the fake adapters, a fake speech model and
//! the real engine. A scenario sets it up and then plays its steps.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, Weak};

use chrono::Duration;
use muesli_core::assistants::{ClaudeDesktopConfig, ClaudeDesktopExtension};
use muesli_core::ports::{Access, Clock, DetectionSample, MeetingDetector, Permission, PlatformWords};
use muesli_core::store::{MeetingStore, Store};
use muesli_core::{DetectedMeeting, Meeting, MeetingState, Paths, Preferences, TranscriptSegment};
use muesli_engine::fake::FakeSpeech;
use muesli_engine::{Engine, Host, Intent, Options, Window};
use muesli_platform_fake::{FakePermissions, FakePlatform};
use serde::Deserialize;
use serde_json::{json, Value};

/// What the simulated shell did, shown in the simulator's log.
pub struct SimHost {
    pub windows: Mutex<BTreeSet<&'static str>>,
    pub version: Arc<AtomicU64>,
    pub log: Mutex<Vec<String>>,
    pub engine: Mutex<Weak<Engine>>,
    pub home: PathBuf,
    pub fixtures: PathBuf,
    pub skin: &'static str,
}

impl SimHost {
    fn say(&self, line: String) {
        self.log.lock().unwrap().push(line);
        self.bump();
    }

    pub fn bump(&self) {
        self.version.fetch_add(1, Ordering::SeqCst);
    }

    fn engine(&self) -> Option<Arc<Engine>> {
        self.engine.lock().unwrap().upgrade()
    }
}

impl Host for SimHost {
    fn show(&self, window: Window) {
        self.windows.lock().unwrap().insert(window.label());
        self.bump();
    }

    fn close(&self, window: Window) {
        self.windows.lock().unwrap().remove(window.label());
        self.bump();
    }

    fn changed(&self) {
        self.bump();
    }

    /// No dialog in a browser: "picks" a copy of the stereo test tone.
    fn pick_audio_files(&self) {
        let file = self.home.join("Downloads/Customer interview.wav");
        let _ = std::fs::create_dir_all(file.parent().unwrap());
        let _ = std::fs::copy(self.fixtures.join("audio/tone-stereo-48k.wav"), &file);
        self.say(format!("File picker: chose {}", file.display()));
        if let Some(engine) = self.engine() {
            std::thread::spawn(move || engine.intent(Intent::TranscribeFiles { paths: vec![file] }));
        }
    }

    fn pick_transcripts_folder(&self) {
        let folder = self.home.join("Documents/Meeting notes");
        self.say(format!("Folder picker: chose {}", folder.display()));
        if let Some(engine) = self.engine() {
            std::thread::spawn(move || engine.intent(Intent::SetTranscriptsFolder { path: folder }));
        }
    }

    fn reveal(&self, path: &Path) {
        let manager = if self.skin == "mac" { "Finder" } else { "Explorer" };
        self.say(format!("{manager} shows {}", path.display()));
    }

    fn open(&self, path: &Path) {
        self.say(format!("Opened {}", path.display()));
    }

    fn quit(&self) {
        self.say("Quit Muesli (the simulator keeps running; reset to start again)".into());
    }

    fn helper_path(&self) -> Option<PathBuf> {
        Some(PathBuf::from(if self.skin == "mac" {
            "/Applications/Muesli.app/Contents/MacOS/muesli-mcp"
        } else {
            r"C:\Program Files\Muesli\muesli-mcp.exe"
        }))
    }

    fn extension_path(&self) -> Option<PathBuf> {
        Some(PathBuf::from("Muesli.mcpb"))
    }

    fn claude_installed(&self) -> bool {
        true
    }
}

/// A scenario file from `fixtures/scenarios/`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Scenario {
    pub name: String,
    pub description: String,
    /// Setup finished before Muesli starts (otherwise: first launch).
    pub setup_done: bool,
    /// "ready" (default), "missing", "downloading".
    pub model: Option<String>,
    /// "allowed" (default) or "notAsked": the permissions on a fresh computer.
    pub permissions: Option<String>,
    /// AI assistants switched on in Settings.
    pub assistants: bool,
    /// The version of the Muesli extension Claude Desktop has installed, if any.
    pub claude_extension: Option<String>,
    pub steps: Vec<Value>,
}

pub struct World {
    pub engine: Arc<Engine>,
    pub fakes: FakePlatform,
    pub speech: Arc<FakeSpeech>,
    pub host: Arc<SimHost>,
    pub skin: &'static str,
    _dir: tempfile::TempDir,
}

impl World {
    pub fn new(skin: &str, scenario: &Scenario, version: Arc<AtomicU64>, fixtures: PathBuf) -> Self {
        let dir = tempfile::tempdir().expect("a temporary folder");
        let (words, skin) = if skin == "windows" { (PlatformWords::WINDOWS, "windows") } else { (PlatformWords::MAC, "mac") };
        let mut fakes = FakePlatform::with_words(dir.path(), words);
        let windows_permissions = [(Permission::SystemAudio, Access::NotNeeded), (Permission::Calendar, Access::NotNeeded)];
        let permissions =
            if scenario.permissions.as_deref() == Some("notAsked") { FakePermissions::new() } else { FakePermissions::all_allowed() };
        if skin == "windows" {
            for (p, a) in windows_permissions {
                permissions.set(p, a);
            }
        }
        fakes.permissions = Arc::new(permissions);
        let paths = Paths::from_platform(&*fakes.paths);
        std::fs::create_dir_all(&paths.support).unwrap();
        if scenario.setup_done {
            Preferences { onboarding_done: true, mcp_enabled: scenario.assistants, ..Default::default() }.save(&paths).unwrap();
        }
        if let Some(version) = &scenario.claude_extension {
            let claude = ClaudeDesktopConfig::default_directory(&*fakes.paths);
            let folder = ClaudeDesktopExtension::extensions_directory(&claude).join("local.mcpb.muesli.muesli");
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(folder.join("manifest.json"), json!({ "name": "muesli", "version": version }).to_string()).unwrap();
        }
        let speech = Arc::new(match scenario.model.as_deref() {
            Some("missing") | Some("downloading") => FakeSpeech::missing(574_000_000),
            _ => FakeSpeech::ready(),
        });
        let host = Arc::new(SimHost {
            windows: Mutex::new(BTreeSet::new()),
            version,
            log: Mutex::new(Vec::new()),
            engine: Mutex::new(Weak::new()),
            home: dir.path().to_path_buf(),
            fixtures,
            skin,
        });
        let engine =
            Engine::new(fakes.platform(), speech.clone(), host.clone(), Options { system_languages: vec!["nl".into(), "en".into()] });
        *host.engine.lock().unwrap() = Arc::downgrade(&engine);
        engine.start();
        let world = Self { engine, fakes, speech, host, skin, _dir: dir };
        for step in &scenario.steps {
            if let Err(why) = world.act(step) {
                world.host.say(format!("Scenario step failed: {why}"));
            }
        }
        world.host.bump();
        world
    }

    /// One real second passed.
    pub fn tick(&self) {
        self.fakes.clock.advance(Duration::seconds(1));
        self.engine.tick();
    }

    fn advance(&self, seconds: i64) {
        for _ in 0..seconds {
            self.tick();
        }
    }

    /// What the simulator page shows besides Muesli's own windows.
    pub fn state(&self) -> Value {
        json!({
            "skin": self.skin,
            "windows": self.host.windows.lock().unwrap().iter().collect::<Vec<_>>(),
            "menu": self.engine.menu(),
            "question": self.fakes.notifier.pending_question(),
            "notice": self.fakes.notifier.pending_notice(),
            "notifications": self.fakes.notifier.enabled.load(Ordering::SeqCst),
            "clock": self.fakes.clock.now().to_rfc3339(),
            "call": self.fakes.detector.sample(self.fakes.clock.now()).mic_busy == Some(true),
            "model": muesli_engine::SpeechModel::status(&*self.speech),
            "log": self.host.log.lock().unwrap().iter().rev().take(8).cloned().collect::<Vec<_>>(),
        })
    }

    /// Plays one action: from a scenario step or a button on the simulator page.
    pub fn act(&self, step: &Value) -> Result<(), String> {
        let what = step.get("do").and_then(Value::as_str).ok_or("a step needs \"do\"")?;
        let num = |key: &str, default: f64| step.get(key).and_then(Value::as_f64).unwrap_or(default);
        let text = |key: &str, default: &str| step.get(key).and_then(Value::as_str).unwrap_or(default).to_string();
        match what {
            "call" => {
                let teams = step.get("teams").and_then(Value::as_bool).unwrap_or(true);
                self.fakes.detector.set(DetectionSample { calendar: None, mic_busy: Some(true), teams_running: teams });
                self.advance(num("seconds", 15.0) as i64);
            }
            "calendar" => {
                let meeting = DetectedMeeting::new(text("title", "Weekly sync"), "calendar");
                self.fakes.detector.set_calendar(Some((meeting, text("id", "event-1"))));
                self.advance(6);
            }
            "hangUp" => {
                self.fakes.detector.set(DetectionSample { calendar: None, mic_busy: Some(false), teams_running: false });
                self.advance(num("seconds", 0.0) as i64);
            }
            "talk" => {
                let seconds = num("seconds", 31.0);
                self.fakes.mic.feed_seconds(seconds);
                self.fakes.system_audio.feed_seconds(seconds);
                self.advance(seconds as i64);
            }
            "advance" => self.advance(num("seconds", 60.0) as i64),
            "answer" => {
                self.fakes.notifier.answer(step.get("accept").and_then(Value::as_bool).unwrap_or(true));
            }
            "clickNotice" => {
                self.fakes.notifier.click_action();
            }
            "dismissNotice" => self.fakes.notifier.dismiss_notice(),
            "notifications" => self.fakes.notifier.set_enabled(step.get("on").and_then(Value::as_bool).unwrap_or(true)),
            "model" => match text("state", "ready").as_str() {
                "ready" => self.speech.finish(),
                "progress" => self.speech.progress((num("fraction", 0.5) * 574_000_000.0) as u64),
                "failed" => self.speech.fail(
                    "Could not connect to huggingface.co (timed out after 15 s).",
                    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin",
                ),
                other => return Err(format!("unknown model state {other}")),
            },
            "permission" => {
                let permission: Permission =
                    serde_json::from_value(step.get("permission").cloned().unwrap_or_default()).map_err(|e| e.to_string())?;
                let access: Access =
                    serde_json::from_value(step.get("access").cloned().unwrap_or(json!("allowed"))).map_err(|e| e.to_string())?;
                self.fakes.permissions.set(permission, access);
            }
            "intent" => {
                let intent: Intent = serde_json::from_value(step.get("intent").cloned().unwrap_or_default()).map_err(|e| e.to_string())?;
                self.engine.intent(intent);
            }
            "wait" => std::thread::sleep(std::time::Duration::from_millis(num("ms", 200.0) as u64)),
            "meeting" => self.add_meeting(step),
            "dropFile" => {
                // An audio file dropped on Muesli's window.
                let name = text("name", "Voice memo.flac");
                let file = self.host.home.join("Desktop").join(&name);
                let _ = std::fs::create_dir_all(file.parent().unwrap());
                std::fs::copy(self.host.fixtures.join("audio/tone-mono-22k.flac"), &file).map_err(|e| e.to_string())?;
                self.engine.intent(Intent::TranscribeFiles { paths: vec![file] });
            }
            "closeWindow" => {
                let label = text("window", "main");
                self.host.windows.lock().unwrap().retain(|w| *w != label);
            }
            other => return Err(format!("unknown step {other}")),
        }
        self.host.bump();
        Ok(())
    }

    /// A finished meeting with a few lines, written as the app would have.
    fn add_meeting(&self, step: &Value) {
        let paths = self.engine.paths();
        let title = step.get("title").and_then(Value::as_str).unwrap_or("Weekly sync");
        let source = step.get("source").and_then(Value::as_str).unwrap_or("calendar");
        let minutes = step.get("minutes").and_then(Value::as_i64).unwrap_or(30);
        let ago = step.get("hoursAgo").and_then(Value::as_i64).unwrap_or(2);
        let mut m = Meeting::new(title, source);
        m.started_at = self.fakes.clock.now() - Duration::hours(ago);
        m.ended_at = Some(m.started_at + Duration::minutes(minutes));
        m.attendees = step.get("attendees").and_then(|a| serde_json::from_value(a.clone()).ok()).unwrap_or_default();
        m.state = MeetingState::Ready;
        let lines: Vec<(String, String)> = step
            .get("lines")
            .and_then(|l| serde_json::from_value(l.clone()).ok())
            .unwrap_or_else(|| vec![("You".into(), "Shall we start?".into()), ("Others".into(), "Yes, let's go through the list.".into())]);
        m.transcript = lines
            .into_iter()
            .enumerate()
            .map(|(i, (speaker, text))| TranscriptSegment { start: i as f64 * 37.0, end: i as f64 * 37.0 + 8.0, speaker, text })
            .collect();
        MeetingStore::new(&paths).save(&m);
        Store::save(&m, paths.transcripts_dir());
        self.host.bump();
    }
}
