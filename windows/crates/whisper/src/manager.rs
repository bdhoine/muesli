use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use muesli_core::whisper_model::file_is_at_least;
use muesli_core::{Paths, Preferences, WhisperModel};

use crate::download::{download, DownloadSpec};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Phase {
    Missing(WhisperModel),
    Downloading { model: WhisperModel, received: u64, total: u64 },
    Ready(PathBuf),
    Failed(String),
}

/// One running download. Each has its own cancel flag, so cancelling one never stops the next.
struct Job {
    model: WhisperModel,
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

/// Knows which Whisper model Muesli should use, whether it is on disk, and downloads it.
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
        if let Some(custom) = inner.settings.custom_model_url() {
            return if file_is_at_least(&custom, 1) {
                Phase::Ready(custom)
            } else {
                Phase::Failed(format!("The chosen model file was not found: {}", custom.display()))
            };
        }
        let model = inner.settings.model;
        if let Some(job) = inner.job.as_ref().filter(|j| j.model == model) {
            return Phase::Downloading { model, received: job.received, total: job.total };
        }
        if model.is_installed(&self.paths) {
            return Phase::Ready(model.path(&self.paths));
        }
        match &inner.last_error {
            Some(e) => Phase::Failed(e.clone()),
            None => Phase::Missing(model),
        }
    }

    pub fn is_ready(&self) -> bool {
        matches!(self.phase(), Phase::Ready(_))
    }

    pub fn is_downloading(&self) -> bool {
        self.inner.lock().unwrap().job.is_some()
    }

    pub fn is_installed(&self, model: WhisperModel) -> bool {
        model.is_installed(&self.paths)
    }

    /// What the tray and onboarding show while a download runs: text and a 0..1 fraction.
    pub fn activity(&self) -> Option<(String, Option<f64>)> {
        let inner = self.inner.lock().unwrap();
        let (model, received, total) = inner.job.as_ref().map(|j| (j.model, j.received, j.total))?;
        let fraction = (total > 0).then(|| (received as f64 / total as f64).min(1.0));
        Some((format!("Downloading the {} Whisper model", model.title()), fraction))
    }

    /// Downloads the selected model unless it is there already.
    pub fn download_selected_if_needed(&self, on_change: impl Fn() + Send + 'static) {
        let (model, custom) = {
            let i = self.inner.lock().unwrap();
            (i.settings.model, i.settings.custom_model_url().is_some())
        };
        if !custom && !self.is_installed(model) {
            self.download(model, on_change);
        }
    }

    /// Starts a download on a background thread. `on_change` runs on progress and when it ends.
    pub fn download(&self, model: WhisperModel, on_change: impl Fn() + Send + 'static) {
        let spec = DownloadSpec {
            url: model.download_url(),
            dest: model.path(&self.paths),
            min_bytes: model.minimum_valid_bytes(),
            approximate_bytes: model.approximate_bytes(),
        };
        self.download_spec(model, spec, on_change);
    }

    /// Starts a download, replacing any download of another model that is still running.
    pub fn download_spec(&self, model: WhisperModel, spec: DownloadSpec, on_change: impl Fn() + Send + 'static) {
        let cancel = Arc::new(AtomicBool::new(false));
        {
            let mut inner = self.inner.lock().unwrap();
            match &inner.job {
                Some(job) if job.model == model => return, // already on its way
                Some(job) => job.cancel.store(true, Ordering::Relaxed),
                None => {}
            }
            inner.last_error = None;
            inner.job = Some(Job { model, received: 0, total: spec.approximate_bytes, cancel: cancel.clone() });
        }
        let me = self.clone();
        std::thread::spawn(move || {
            // Only the job that owns this cancel flag may touch the shared state.
            let mine = |inner: &Inner| inner.job.as_ref().is_some_and(|j| Arc::ptr_eq(&j.cancel, &cancel));
            let result = download(
                &spec,
                &mut |received, total| {
                    let mut inner = me.inner.lock().unwrap();
                    if mine(&inner) {
                        if let Some(job) = inner.job.as_mut() {
                            job.received = received;
                            job.total = total;
                        }
                        drop(inner);
                        on_change();
                    }
                },
                &cancel,
            );
            {
                let mut inner = me.inner.lock().unwrap();
                if !mine(&inner) {
                    return; // replaced by a newer download, which reports for itself
                }
                inner.job = None;
                inner.last_error = match result {
                    Ok(()) | Err(crate::DownloadError::Cancelled) => None,
                    Err(e) => Some(e.to_string()),
                };
            }
            on_change();
        });
    }

    /// Cancels the running download, if any. Takes effect at once for `phase()`.
    pub fn cancel(&self) {
        if let Some(job) = self.inner.lock().unwrap().job.take() {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }

    pub fn delete(&self, model: WhisperModel) {
        let _ = std::fs::remove_file(model.path(&self.paths));
    }

    /// Blocks until a running download ends (tests and shutdown).
    pub fn wait(&self) {
        while self.is_downloading() {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
}
