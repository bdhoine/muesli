//! Shared application state and the data sent to the UI.

use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use muesli_core::status::{Activity, Current, Detected, LastReady};
use muesli_core::{DetectedMeeting, Meeting, Paths, Preferences};
use muesli_whisper::ModelManager;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Status {
    Idle,
    Recording { title: String },
    Saving,
    Error { message: String },
}

/// What the small prompt window shows.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Prompt {
    /// Tells this prompt from an older one whose notification is answered late.
    #[serde(skip)]
    pub id: u64,
    /// "start" (a meeting was detected) or "stop" (the call seems to be over)
    pub kind: String,
    pub title: String,
    pub body: String,
}

pub struct Session {
    pub meeting: Arc<Mutex<Meeting>>,
    /// Tells the audio thread to stop; the thread then drops the capture and closes the chunk channel.
    pub stop_audio: Sender<()>,
    pub consumer: Option<JoinHandle<()>>,
    pub audio: Option<JoinHandle<()>>,
}

pub struct App {
    /// Paths from the environment; `paths()` adds the user's transcripts folder.
    base: Paths,
    pub prefs: Mutex<Preferences>,
    pub model: ModelManager,
    pub status: Mutex<Status>,
    pub session: Mutex<Option<Session>>,
    pub detected: Mutex<Option<(DetectedMeeting, chrono::DateTime<chrono::Utc>)>>,
    pub last_ready: Mutex<Option<LastReady>>,
    pub prompt: Mutex<Option<Prompt>>,
    pub pending: Mutex<Option<DetectedMeeting>>,
    pub claude_status: Mutex<String>,
    /// Set when a stream broke mid-recording; shown once the transcript so far is saved.
    pub capture_error: Mutex<Option<String>>,
}

impl App {
    pub fn new(paths: Paths) -> Arc<Self> {
        let prefs = Preferences::load(&paths);
        Arc::new(Self {
            model: ModelManager::new(paths.clone(), prefs.clone()),
            base: paths,
            prefs: Mutex::new(prefs),
            status: Mutex::new(Status::Idle),
            session: Mutex::new(None),
            detected: Mutex::new(None),
            last_ready: Mutex::new(None),
            prompt: Mutex::new(None),
            pending: Mutex::new(None),
            claude_status: Mutex::new(String::new()),
            capture_error: Mutex::new(None),
        })
    }

    /// Where things are right now. The transcripts folder follows the preferences.
    pub fn paths(&self) -> Paths {
        self.base.clone().with_preferences(&self.prefs())
    }

    pub fn prefs(&self) -> Preferences {
        self.prefs.lock().unwrap().clone()
    }

    pub fn status(&self) -> Status {
        self.status.lock().unwrap().clone()
    }

    pub fn is_idle(&self) -> bool {
        matches!(self.status(), Status::Idle)
    }

    pub fn is_recording(&self) -> bool {
        matches!(self.status(), Status::Recording { .. })
    }

    pub fn activity(&self) -> Activity {
        match self.status() {
            Status::Idle => Activity::Idle { preparing: self.model.is_downloading() },
            Status::Recording { .. } => Activity::Recording,
            Status::Saving => Activity::Finalizing,
            Status::Error { message } => Activity::Error(message),
        }
    }

    /// The `state.json` other processes (the MCP server) read.
    pub fn app_status(&self) -> muesli_core::AppStatus {
        let mut out = muesli_core::AppStatus::for_activity(&self.activity());
        out.transcripts_folder = Some(self.paths().transcripts.to_string_lossy().into_owned());
        out.last_ready = self.last_ready.lock().unwrap().clone();
        if let Some((d, since)) = self.detected.lock().unwrap().clone() {
            out.detected = Some(Detected { title: d.title, source: d.source, since });
        }
        if let Some(s) = self.session.lock().unwrap().as_ref() {
            let m = s.meeting.lock().unwrap();
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
}
