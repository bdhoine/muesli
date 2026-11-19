//! Audio files: the drop folder, what happens to a file afterwards, and (with the `whisper`
//! feature) transcribing one. The logic is `muesli_core::import` and `muesli-import`.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use muesli_core::import::{self, AudioCleanup};

use crate::paths::PathSet;

#[uniffi::remote(Enum)]
pub enum AudioCleanup {
    Keep,
    Trash,
    Delete,
}

/// File types Muesli can read (lower case, no dot), for the file picker.
#[uniffi::export]
pub fn audio_extensions() -> Vec<String> {
    import::AUDIO_EXTENSIONS.iter().map(|e| e.to_string()).collect()
}

#[uniffi::export]
pub fn is_audio_file(path: String) -> bool {
    import::is_audio(Path::new(&path))
}

/// Applies the cleanup setting to a transcribed file. `None` when it worked, else why not.
#[uniffi::export]
pub fn audio_clean_up(path: String, mode: AudioCleanup) -> Option<String> {
    import::clean_up(Path::new(&path), mode).err()
}

/// Watches the drop folder: call `poll` every few seconds.
#[derive(uniffi::Object)]
pub struct DropFolder {
    inner: Mutex<muesli_core::DropFolder>,
}

#[uniffi::export]
impl DropFolder {
    #[uniffi::constructor]
    pub fn new(paths: PathSet) -> Self {
        Self { inner: Mutex::new(muesli_core::DropFolder::new(&paths)) }
    }

    /// New, complete audio files in the folder, oldest first; each is returned once.
    pub fn poll(&self, folder: String) -> Vec<String> {
        let found = self.inner.lock().unwrap().poll(Path::new(&folder));
        found.into_iter().map(|p| p.to_string_lossy().into_owned()).collect()
    }

    /// Remember the file as handled (so it is not offered again unless it changes).
    pub fn mark_done(&self, path: String) {
        self.inner.lock().unwrap().mark_done(&PathBuf::from(path));
    }

    /// The file is gone or the job was dropped: allow it to be offered again.
    pub fn forget(&self, path: String) {
        self.inner.lock().unwrap().forget(&PathBuf::from(path));
    }
}

#[cfg(feature = "whisper")]
mod transcribing {
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;

    use muesli_core::Meeting;
    use muesli_import::ImportError;

    use crate::paths::PathSet;
    use crate::transcriber::Transcriber;

    /// Why an import did not finish.
    #[derive(Debug, thiserror::Error, uniffi::Error)]
    pub enum ImportFailure {
        #[error("Cancelled.")]
        Cancelled,
        #[error("{message}")]
        Failed { message: String },
    }

    impl From<ImportError> for ImportFailure {
        fn from(e: ImportError) -> Self {
            match e {
                ImportError::Cancelled => Self::Cancelled,
                other => Self::Failed { message: other.to_string() },
            }
        }
    }

    /// Told how far along a file is. Called from the import thread, so hop to the main actor
    /// before touching the UI.
    #[uniffi::export(with_foreign)]
    pub trait ImportObserver: Send + Sync {
        /// 0 to 1, or a negative number while the file's length is unknown.
        fn progress(&self, fraction: f64);
    }

    /// One audio file being transcribed. Make one per file; `cancel` may be called from any thread.
    #[derive(uniffi::Object, Default)]
    pub struct FileImporter {
        cancel: AtomicBool,
    }

    #[uniffi::export]
    impl FileImporter {
        #[uniffi::constructor]
        pub fn new() -> Self {
            Self::default()
        }

        pub fn cancel(&self) {
            self.cancel.store(true, Ordering::Relaxed);
        }

        /// Decodes and transcribes `path` and stores it as a meeting. Blocks until done, so call it
        /// off the main thread.
        pub fn import(
            &self,
            transcriber: Arc<Transcriber>,
            path: String,
            paths: PathSet,
            observer: Arc<dyn ImportObserver>,
        ) -> Result<Meeting, ImportFailure> {
            muesli_import::import_file(Path::new(&path), transcriber.inner(), &paths, &self.cancel, |p| {
                observer.progress(p.unwrap_or(-1.0))
            })
            .map_err(Into::into)
        }
    }
}
