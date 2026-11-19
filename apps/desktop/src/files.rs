//! Audio files to transcribe: the ones picked from the tray menu and the ones found in the drop
//! folder. A queue, one file at a time; a recording always goes first. The work itself (decoding,
//! chunking, Whisper, saving) is `muesli-import`, shared with the macOS app.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use muesli_core::import::clean_up;
use muesli_core::status::{FileJob, LastReady};
use muesli_core::{languages, AudioChunk, AudioCleanup, DropFolder, TranscriptSegment};
use muesli_import::{import_file, ImportError, SpeechToText};
use muesli_whisper::Transcriber;
use serde::Serialize;

use crate::ctx::Ctx;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FileState {
    Waiting,
    Running { progress: Option<f64> },
    Done,
    Failed { message: String },
    Cancelled,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileItem {
    pub id: u64,
    pub name: String,
    #[serde(skip)]
    pub path: PathBuf,
    /// Picked from the menu (shown in the small window), not found in the drop folder.
    pub manual: bool,
    pub state: FileState,
}

impl FileItem {
    fn finished(&self) -> bool {
        !matches!(self.state, FileState::Waiting | FileState::Running { .. })
    }
}

#[derive(Default)]
pub struct Files {
    items: Mutex<Vec<FileItem>>,
    next_id: AtomicU64,
    worker: AtomicBool,
    cancel: Mutex<Option<(u64, Arc<AtomicBool>)>>,
    watch: Mutex<Option<(PathBuf, DropFolder)>>,
    /// The last problem with a file from the drop folder, shown until dismissed.
    pub error: Mutex<Option<String>>,
}

impl Files {
    pub fn items(&self) -> Vec<FileItem> {
        self.items.lock().unwrap().clone()
    }

    pub fn running(&self) -> Option<FileItem> {
        self.items.lock().unwrap().iter().find(|i| matches!(i.state, FileState::Running { .. })).cloned()
    }

    pub fn waiting(&self) -> usize {
        self.items.lock().unwrap().iter().filter(|i| i.state == FileState::Waiting).count()
    }

    /// What `state.json` says about the running file.
    pub fn job(&self) -> Option<FileJob> {
        let running = self.running()?;
        let progress = match running.state {
            FileState::Running { progress } => progress,
            _ => None,
        };
        Some(FileJob { name: running.name, progress, waiting: self.waiting() as u32 })
    }

    fn set(&self, id: u64, state: FileState) {
        if let Some(item) = self.items.lock().unwrap().iter_mut().find(|i| i.id == id) {
            item.state = state;
        }
    }

    /// Finished files from the drop folder leave no trace in the lists.
    fn prune(&self) {
        self.items.lock().unwrap().retain(|i| i.manual || !i.finished());
    }
}

pub fn enqueue(ctx: &Ctx, paths: Vec<PathBuf>, manual: bool) {
    {
        let mut items = ctx.app.files.items.lock().unwrap();
        if manual {
            items.retain(|i| !(i.manual && i.finished()));
        }
        for path in paths {
            let id = ctx.app.files.next_id.fetch_add(1, Ordering::Relaxed);
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            items.push(FileItem { id, name, path, manual, state: FileState::Waiting });
        }
    }
    ctx.changed();
    start_worker(ctx);
}

pub fn cancel(ctx: &Ctx, id: u64) {
    if let Some((running, flag)) = ctx.app.files.cancel.lock().unwrap().as_ref() {
        if *running == id {
            flag.store(true, Ordering::Relaxed);
            return;
        }
    }
    let waiting = ctx.app.files.items().iter().any(|i| i.id == id && i.state == FileState::Waiting);
    if waiting {
        ctx.app.files.set(id, FileState::Cancelled);
        ctx.app.files.prune();
        ctx.changed();
    }
}

pub fn cancel_all_manual(ctx: &Ctx) {
    for item in ctx.app.files.items().into_iter().filter(|i| i.manual && !i.finished()) {
        cancel(ctx, item.id);
    }
}

pub fn clear_finished(ctx: &Ctx) {
    ctx.app.files.items.lock().unwrap().retain(|i| !(i.manual && i.finished()));
    ctx.changed();
}

pub fn dismiss_error(ctx: &Ctx) {
    *ctx.app.files.error.lock().unwrap() = None;
    ctx.changed();
}

/// Looks in the drop folder (called every few seconds) and queues what is new.
pub fn poll_folder(ctx: &Ctx) {
    let Some(folder) = ctx.app.prefs().drop_folder.map(PathBuf::from) else {
        *ctx.app.files.watch.lock().unwrap() = None;
        return;
    };
    let found = {
        let mut watch = ctx.app.files.watch.lock().unwrap();
        if watch.as_ref().map(|(f, _)| f) != Some(&folder) {
            *watch = Some((folder.clone(), DropFolder::new(&ctx.app.paths())));
        }
        watch.as_mut().map(|(_, w)| w.poll(&folder)).unwrap_or_default()
    };
    if !found.is_empty() {
        enqueue(ctx, found, false);
    }
}

fn start_worker(ctx: &Ctx) {
    if ctx.app.files.worker.swap(true, Ordering::SeqCst) {
        return;
    }
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        work(&ctx);
        ctx.app.files.worker.store(false, Ordering::SeqCst);
        ctx.changed();
        // something may have been queued between the last look and the flag going down
        if ctx.app.files.waiting() > 0 {
            start_worker(&ctx);
        }
    });
}

/// Waits for a recording to end before each chunk, so a live transcript is never slowed down.
struct Polite<'a> {
    inner: &'a Transcriber,
    ctx: &'a Ctx,
    cancel: &'a AtomicBool,
}

impl SpeechToText for Polite<'_> {
    fn transcribe(&self, chunk: &AudioChunk) -> Result<Vec<TranscriptSegment>, String> {
        while self.ctx.app.is_recording() && !self.cancel.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(500));
        }
        SpeechToText::transcribe(self.inner, chunk)
    }
}

fn work(ctx: &Ctx) {
    let mut transcriber: Option<Transcriber> = None;
    loop {
        let Some(next) = ctx.app.files.items().into_iter().find(|i| i.state == FileState::Waiting) else { return };
        // Not while recording or while the model is not ready.
        if !ctx.app.is_idle() || !ctx.app.model.is_ready() {
            std::thread::sleep(Duration::from_secs(1));
            continue;
        }
        if transcriber.is_none() {
            let prefs = ctx.app.prefs();
            let paths = ctx.app.paths();
            transcriber = Transcriber::new(
                &prefs.model_path(&paths),
                Some(&prefs.vad_model_path(&paths)),
                prefs.effective_languages(&languages::system_defaults()),
            );
        }
        let Some(model) = transcriber.as_ref() else {
            finish_failed(ctx, &next, "The speech model could not be loaded.".into());
            continue;
        };
        let cancel = Arc::new(AtomicBool::new(false));
        *ctx.app.files.cancel.lock().unwrap() = Some((next.id, cancel.clone()));
        ctx.app.files.set(next.id, FileState::Running { progress: None });
        ctx.changed();

        let polite = Polite { inner: model, ctx, cancel: &cancel };
        let result = import_file(&next.path, &polite, &ctx.app.paths(), &cancel, |p| {
            ctx.app.files.set(next.id, FileState::Running { progress: p });
            ctx.changed();
        });
        *ctx.app.files.cancel.lock().unwrap() = None;

        match result {
            Ok(meeting) => {
                *ctx.app.last_ready.lock().unwrap() = Some(LastReady { id: meeting.id, title: meeting.title, ended_at: meeting.ended_at });
                ctx.app.files.set(next.id, FileState::Done);
                if !next.manual {
                    after_folder_file(ctx, &next.path);
                }
            }
            Err(ImportError::Cancelled) => {
                ctx.app.files.set(next.id, FileState::Cancelled);
                if !next.manual {
                    mark_done(ctx, &next.path);
                }
            }
            Err(e) => finish_failed(ctx, &next, e.to_string()),
        }
        ctx.app.files.prune();
        ctx.changed();
    }
}

fn finish_failed(ctx: &Ctx, item: &FileItem, message: String) {
    if !item.manual {
        mark_done(ctx, &item.path); // do not try the same file again
        *ctx.app.files.error.lock().unwrap() = Some(format!("{}: {message}", item.name));
    }
    ctx.app.files.set(item.id, FileState::Failed { message });
    ctx.app.files.prune();
    ctx.changed();
}

fn mark_done(ctx: &Ctx, path: &Path) {
    if let Some((_, watch)) = ctx.app.files.watch.lock().unwrap().as_mut() {
        watch.mark_done(path);
    }
}

/// A folder file was transcribed: remember it, then apply the cleanup setting.
fn after_folder_file(ctx: &Ctx, path: &Path) {
    mark_done(ctx, path);
    let mode = ctx.app.prefs().drop_folder_cleanup;
    if mode == AudioCleanup::Keep {
        return;
    }
    if let Err(problem) = clean_up(path, mode) {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        *ctx.app.files.error.lock().unwrap() = Some(format!("{name} was transcribed, but could not be removed: {problem}"));
    }
    if let Some((_, watch)) = ctx.app.files.watch.lock().unwrap().as_mut() {
        watch.forget(path);
    }
}
