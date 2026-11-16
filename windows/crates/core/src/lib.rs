//! Platform-neutral core of Muesli: a Rust port of the Swift `MuesliCore` library.
//!
//! The on-disk formats (`config.json`, `state.json`, `meetings/*.json`, transcript Markdown and
//! JSON) are shared with the Swift app and must stay compatible. Nothing here touches audio,
//! windows or the network; those live behind traits in other crates.

pub mod assistants;
pub mod chunker;
pub mod detection;
pub mod editor;
pub mod json;
pub mod languages;
pub mod models;
pub mod paths;
pub mod preferences;
pub mod status;
pub mod store;
pub mod whisper_model;

pub use chunker::AudioChunker;
pub use detection::{DetectionEvent, DetectionLogic};
pub use models::{AudioChunk, DetectedMeeting, Meeting, MeetingState, TranscriptSegment};
pub use paths::Paths;
pub use preferences::Preferences;
pub use status::AppStatus;
pub use whisper_model::WhisperModel;

/// The version of the Windows app: the Cargo workspace version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Where deleted transcripts go, as the user sees it.
pub fn trash_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "macOS Trash"
    } else if cfg!(windows) {
        "Recycle Bin"
    } else {
        "Trash"
    }
}
