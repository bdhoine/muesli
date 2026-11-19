//! Port adapters shared by `platform-macos` and `platform-windows`: the parts of the operating
//! system that portable libraries already cover. Nothing here decides anything.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use muesli_core::ports::{Clock, PlatformPaths, Trash};

#[cfg(all(feature = "capture", any(target_os = "macos", target_os = "windows")))]
pub mod cpal_capture;
pub mod teams;

/// The system clock.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// The platform's folders through the `dirs` crate, and the process environment.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemPaths;

fn or_dot(dir: Option<PathBuf>) -> PathBuf {
    dir.unwrap_or_else(|| PathBuf::from("."))
}

impl PlatformPaths for SystemPaths {
    fn data_dir(&self) -> PathBuf {
        or_dot(dirs::data_dir())
    }

    fn local_data_dir(&self) -> PathBuf {
        or_dot(dirs::data_local_dir().or_else(dirs::data_dir))
    }

    fn documents_dir(&self) -> PathBuf {
        or_dot(dirs::document_dir().or_else(dirs::home_dir))
    }

    fn home_dir(&self) -> PathBuf {
        or_dot(dirs::home_dir())
    }

    fn env(&self, name: &str) -> Option<String> {
        std::env::var(name).ok()
    }
}

/// The Trash / Recycle Bin through the `trash` crate.
#[derive(Debug, Clone, Copy)]
pub struct SystemTrash {
    pub name: &'static str,
}

impl Trash for SystemTrash {
    fn trash(&self, path: &Path) -> Result<(), String> {
        trash::delete(path).map_err(|e| e.to_string())
    }

    fn name(&self) -> &'static str {
        self.name
    }
}
