use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use muesli_core::whisper_model::{file_is_at_least, DEFAULT_MODEL, VAD_MODEL};
use muesli_core::{Paths, Preferences};

use crate::download::{download, DownloadError, DownloadSpec};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    /// Something has to be downloaded and nothing is running yet.
    Missing,
    /// `received` and `total` count bytes over everything that is being downloaded.
    Downloading { received: u64, total: u64 },
    /// The speech model to load.
    Ready(PathBuf),
    Failed(String),
}

/// One running download of everything that is missing. Each has its own cancel flag, so
/// cancelling one never stops the next.
struct Job {
    received: u64,
    total: u64,
    cancel: Arc<AtomicBool>,
}

#[derive(Default)]
struct Inner {
    settings: Preferences,
    job: Option<Job>,
    last_error: Option<String>,
}

/// Knows which files Muesli needs (the speech model, or the user's own file, and the voice
/// activity model), whether they are on disk, and downloads what is missing.
/// Recording is only allowed while `is_ready()`. Cheap to clone; clones share state.
#[derive(Clone)]
pub struct ModelManager {
    paths: Paths,
    inner: Arc<Mutex<Inner>>,
}

impl ModelManager {
    pub fn new(paths: Paths, settings: Preferences) -> Self {
        // A download that was cut off by quitting leaves its partial file behind.
        for entry in std::fs::read_dir(&paths.models).into_iter().flatten().flatten() {
            if entry.file_name().to_string_lossy().ends_with(".part") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        let inner = Inner { settings, ..Default::default() };
        Self { paths, inner: Arc::new(Mutex::new(inner)) }
    }

    pub fn apply(&self, settings: Preferences) {
        self.inner.lock().unwrap().settings = settings;
    }

    pub fn phase(&self) -> Phase {
        let inner = self.inner.lock().unwrap();
        let custom = inner.settings.custom_model_url();
        if let Some(custom) = custom.as_ref().filter(|c| !file_is_at_least(c, 1)) {
            return Phase::Failed(format!("The chosen model file was not found: {}", custom.display()));
        }
        if let Some(job) = &inner.job {
            return Phase::Downloading { received: job.received, total: job.total };
        }
        let model_ok = custom.is_some() || DEFAULT_MODEL.is_installed(&self.paths);
        if model_ok && VAD_MODEL.is_installed(&self.paths) {
            return Phase::Ready(inner.settings.model_path(&self.paths));
        }
        match &inner.last_error {
            Some(e) => Phase::Failed(e.clone()),
            None => Phase::Missing,
        }
    }

    pub fn is_ready(&self) -> bool {
        matches!(self.phase(), Phase::Ready(_))
    }

    pub fn is_downloading(&self) -> bool {
        self.inner.lock().unwrap().job.is_some()
    }

    /// What the tray and menu show while a download runs: text and a 0..1 fraction.
    pub fn activity(&self) -> Option<(String, Option<f64>)> {
        let inner = self.inner.lock().unwrap();
        let job = inner.job.as_ref()?;
        let fraction = (job.total > 0).then(|| (job.received as f64 / job.total as f64).min(1.0));
        Some((format!("Downloading the {} speech model", DEFAULT_MODEL.title), fraction))
    }

    /// The running download: bytes received, total bytes.
    pub fn download_progress(&self) -> Option<(u64, u64)> {
        self.inner.lock().unwrap().job.as_ref().map(|j| (j.received, j.total))
    }

    pub fn last_error(&self) -> Option<String> {
        self.inner.lock().unwrap().last_error.clone()
    }

    /// What is missing: the voice activity model, then the speech model unless the user chose a file.
    pub fn missing_downloads(&self) -> Vec<DownloadSpec> {
        let custom = self.inner.lock().unwrap().settings.custom_model_url().is_some();
        let mut specs = Vec::new();
        for (model, needed) in [(VAD_MODEL, true), (DEFAULT_MODEL, !custom)] {
            if needed && !model.is_installed(&self.paths) {
                specs.push(DownloadSpec {
                    url: model.url.to_string(),
                    dest: model.path(&self.paths),
                    min_bytes: model.minimum_valid_bytes(),
                    approximate_bytes: model.approximate_bytes,
                });
            }
        }
        specs
    }

    /// Downloads whatever is missing on a background thread (nothing, when all is there or a
    /// download already runs). `on_change` runs on progress and when it ends.
    pub fn ensure_ready(&self, on_change: impl Fn() + Send + 'static) {
        self.download_all(self.missing_downloads(), on_change);
    }

    /// `ensure_ready` for an explicit list of files (tests point it at a local server).
    pub fn download_all(&self, specs: Vec<DownloadSpec>, on_change: impl Fn() + Send + 'static) {
        if specs.is_empty() {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let total: u64 = specs.iter().map(|s| s.approximate_bytes).sum();
        {
            let mut inner = self.inner.lock().unwrap();
            if inner.job.is_some() {
                return; // already on its way
            }
            inner.last_error = None;
            inner.job = Some(Job { received: 0, total, cancel: cancel.clone() });
        }
        let me = self.clone();
        std::thread::spawn(move || {
            // Only the job that owns this cancel flag may touch the shared state.
            let mine = |inner: &Inner| inner.job.as_ref().is_some_and(|j| Arc::ptr_eq(&j.cancel, &cancel));
            let mut done_before = 0u64;
            let mut result = Ok(());
            for spec in &specs {
                result = download(
                    spec,
                    &mut |received, _| {
                        let mut inner = me.inner.lock().unwrap();
                        if mine(&inner) {
                            if let Some(job) = inner.job.as_mut() {
                                job.received = done_before + received;
                            }
                            drop(inner);
                            on_change();
                        }
                    },
                    &cancel,
                );
                if result.is_err() {
                    break;
                }
                done_before += spec.approximate_bytes;
            }
            {
                let mut inner = me.inner.lock().unwrap();
                if !mine(&inner) {
                    return; // cancelled and possibly replaced: the newer job reports for itself
                }
                inner.job = None;
                inner.last_error = match result {
                    Ok(()) | Err(DownloadError::Cancelled) => None,
                    Err(e) => Some(e.to_string()),
                };
            }
            on_change();
        });
    }

    /// Cancels the running download, if any. Takes effect at once for `phase()`.
    pub fn cancel(&self) {
        let mut inner = self.inner.lock().unwrap();
        if let Some(job) = inner.job.take() {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Old model files (earlier versions let the user pick Small, Medium or Large) and the space
    /// removing them frees.
    pub fn legacy_models(&self) -> (Vec<PathBuf>, u64) {
        muesli_core::whisper_model::legacy_models(&self.paths)
    }

    /// Deletes the old model files. Returns how many were removed.
    pub fn remove_legacy_models(&self) -> usize {
        muesli_core::whisper_model::remove_legacy_models(&self.paths)
    }

    /// Blocks until a running download ends (tests and shutdown).
    pub fn wait(&self) {
        while self.is_downloading() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}
