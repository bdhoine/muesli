//! Fakes for what the engine needs besides the ports: a speech model that downloads when told
//! and "transcribes" with scripted sentences, and a host that records what it was asked to do.
//! Used by the tests and by `muesli-sim`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use muesli_core::{AudioChunk, TranscriptSegment};
use muesli_import::SpeechToText;

use crate::host::{Host, Window};
use crate::speech::{ModelStatus, SpeechModel, Transcriber};

type OnChange = Box<dyn Fn() + Send + Sync>;

/// A speech model whose download moves only when told to (`progress`, `finish`, `fail`), or at
/// once with `FakeSpeech::ready()`.
pub struct FakeSpeech {
    status: Mutex<ModelStatus>,
    on_change: Mutex<Option<OnChange>>,
    total: u64,
    downloads: AtomicUsize,
    /// When set, loading the model fails with this message.
    pub load_error: Mutex<Option<String>>,
}

impl FakeSpeech {
    /// Not downloaded yet; `download` starts a download of `total` bytes.
    pub fn missing(total: u64) -> Self {
        Self {
            status: Mutex::new(ModelStatus::Missing),
            on_change: Mutex::new(None),
            total,
            downloads: AtomicUsize::new(0),
            load_error: Mutex::new(None),
        }
    }

    /// Already downloaded.
    pub fn ready() -> Self {
        let s = Self::missing(1);
        *s.status.lock().unwrap() = ModelStatus::Ready;
        s
    }

    fn set(&self, status: ModelStatus) {
        *self.status.lock().unwrap() = status;
        if let Some(f) = self.on_change.lock().unwrap().as_ref() {
            f();
        }
    }

    /// The running download reached `received` bytes.
    pub fn progress(&self, received: u64) {
        if matches!(self.status(), ModelStatus::Downloading { .. }) {
            self.set(ModelStatus::Downloading { received: received.min(self.total), total: self.total });
        }
    }

    pub fn finish(&self) {
        self.set(ModelStatus::Ready);
    }

    pub fn fail(&self, message: &str, url: &str) {
        self.set(ModelStatus::Failed { message: message.into(), url: Some(url.into()) });
    }

    /// How often a download was started.
    pub fn downloads(&self) -> usize {
        self.downloads.load(Ordering::SeqCst)
    }
}

impl SpeechModel for FakeSpeech {
    fn status(&self) -> ModelStatus {
        self.status.lock().unwrap().clone()
    }

    fn download(&self, on_change: Box<dyn Fn() + Send + Sync>) {
        *self.on_change.lock().unwrap() = Some(on_change);
        if matches!(self.status(), ModelStatus::Missing | ModelStatus::Failed { .. }) {
            self.downloads.fetch_add(1, Ordering::SeqCst);
            self.set(ModelStatus::Downloading { received: 0, total: self.total });
        }
    }

    fn transcriber(&self, _languages: Vec<String>) -> Result<Transcriber, String> {
        if let Some(why) = self.load_error.lock().unwrap().clone() {
            return Err(why);
        }
        Ok(Box::new(ScriptedTranscriber::default()))
    }
}

const YOU: [&str; 6] = [
    "Morning all, shall we start with the roadmap?",
    "I think we can ship the importer this week.",
    "Let's keep the scope small and see how people use it.",
    "I'll write the release notes tonight.",
    "Any objections to moving the review to Thursday?",
    "Great, thanks everyone.",
];
const OTHERS: [&str; 6] = [
    "Sounds good. The design review went well yesterday.",
    "We still need someone to test it on Windows.",
    "I can take that, I have a PC at home.",
    "Do we have numbers on the download size?",
    "About five hundred megabytes for the model, once.",
    "Thursday works for me.",
];

/// "Transcribes" a chunk that has sound into one sentence per speaker, in turn. Silence gives
/// nothing, as with the real voice activity model.
#[derive(Default)]
pub struct ScriptedTranscriber {
    you: AtomicUsize,
    others: AtomicUsize,
}

impl SpeechToText for ScriptedTranscriber {
    fn transcribe(&self, chunk: &AudioChunk) -> Result<Vec<TranscriptSegment>, String> {
        let loud = chunk.samples.iter().any(|s| s.abs() > 0.01);
        if !loud || chunk.samples.is_empty() {
            return Ok(Vec::new());
        }
        let seconds = chunk.samples.len() as f64 / 16_000.0;
        let text = match chunk.speaker.as_str() {
            "You" => YOU[self.you.fetch_add(1, Ordering::SeqCst) % YOU.len()],
            _ => OTHERS[self.others.fetch_add(1, Ordering::SeqCst) % OTHERS.len()],
        };
        Ok(vec![TranscriptSegment {
            start: chunk.offset_seconds,
            end: chunk.offset_seconds + seconds.min(8.0),
            speaker: chunk.speaker.clone(),
            text: text.to_string(),
        }])
    }
}

/// One thing the engine asked the host to do.
#[derive(Debug, Clone, PartialEq)]
pub enum HostCall {
    Show(Window),
    Close(Window),
    PickAudioFiles,
    PickTranscriptsFolder,
    Reveal(PathBuf),
    Open(PathBuf),
    Quit,
}

/// Records what it is asked; `changes` counts `changed()` calls.
#[derive(Default)]
pub struct RecordingHost {
    calls: Mutex<Vec<HostCall>>,
    changes: AtomicUsize,
    /// What `helper_path` answers.
    pub helper: Mutex<Option<PathBuf>>,
    pub extension: Mutex<Option<PathBuf>>,
    pub claude: Mutex<bool>,
}

impl RecordingHost {
    pub fn calls(&self) -> Vec<HostCall> {
        self.calls.lock().unwrap().clone()
    }

    pub fn take_calls(&self) -> Vec<HostCall> {
        std::mem::take(&mut *self.calls.lock().unwrap())
    }

    pub fn changes(&self) -> usize {
        self.changes.load(Ordering::SeqCst)
    }

    /// Whether a window is open, judging by the last show/close.
    pub fn is_open(&self, window: Window) -> bool {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find_map(|c| match c {
                HostCall::Show(w) if *w == window => Some(true),
                HostCall::Close(w) if *w == window => Some(false),
                _ => None,
            })
            .unwrap_or(false)
    }

    fn record(&self, call: HostCall) {
        self.calls.lock().unwrap().push(call);
    }
}

impl Host for RecordingHost {
    fn show(&self, window: Window) {
        self.record(HostCall::Show(window));
    }

    fn close(&self, window: Window) {
        self.record(HostCall::Close(window));
    }

    fn changed(&self) {
        self.changes.fetch_add(1, Ordering::SeqCst);
    }

    fn pick_audio_files(&self) {
        self.record(HostCall::PickAudioFiles);
    }

    fn pick_transcripts_folder(&self) {
        self.record(HostCall::PickTranscriptsFolder);
    }

    fn reveal(&self, path: &Path) {
        self.record(HostCall::Reveal(path.to_path_buf()));
    }

    fn open(&self, path: &Path) {
        self.record(HostCall::Open(path.to_path_buf()));
    }

    fn quit(&self) {
        self.record(HostCall::Quit);
    }

    fn helper_path(&self) -> Option<PathBuf> {
        self.helper.lock().unwrap().clone()
    }

    fn extension_path(&self) -> Option<PathBuf> {
        self.extension.lock().unwrap().clone()
    }

    fn claude_installed(&self) -> bool {
        *self.claude.lock().unwrap()
    }
}

/// The host is shared with the engine as an `Arc`.
pub fn recording_host() -> Arc<RecordingHost> {
    Arc::new(RecordingHost::default())
}
