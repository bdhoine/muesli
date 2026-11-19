//! Speech model management and transcription.

pub mod download;
pub mod manager;
#[cfg(feature = "transcribe")]
pub mod transcriber;

pub use download::{check_reachable, download, DownloadError, DownloadSpec, Reach};
pub use manager::{ModelManager, Phase};
#[cfg(feature = "transcribe")]
pub use transcriber::Transcriber;
