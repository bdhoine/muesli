//! Speech model management and transcription.

pub mod download;
pub mod manager;
#[cfg(feature = "transcribe")]
pub mod transcriber;
pub mod unpack;

pub use download::{check_reachable, download, DownloadError, DownloadSpec, Reach};
pub use manager::{Extra, ModelManager, Phase};
#[cfg(feature = "transcribe")]
pub use transcriber::{SileroDetector, Transcriber};
