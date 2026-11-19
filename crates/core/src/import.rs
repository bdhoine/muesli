//! Audio files to transcribe: which files count. Files are picked in the menu or dropped on the
//! tray icon; Muesli never moves or deletes them.

use std::path::Path;

/// File types Muesli can decode (lower case, without the dot).
pub const AUDIO_EXTENSIONS: [&str; 9] = ["mp3", "m4a", "mp4", "aac", "wav", "flac", "ogg", "oga", "caf"];

/// Whether Muesli can transcribe this file, judged by its extension.
pub fn is_audio(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| AUDIO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}
