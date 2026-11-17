//! Audio files to transcribe: which files count, the drop folder that is watched, what happens to
//! a file afterwards, and the record of what was done so a file that is kept is not transcribed twice.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use crate::json::{self, to_pretty_sorted};
use crate::paths::Paths;

/// File types Muesli can decode (lower case, without the dot).
pub const AUDIO_EXTENSIONS: [&str; 9] = ["mp3", "m4a", "mp4", "aac", "wav", "flac", "ogg", "oga", "caf"];

/// What happens to a file from the drop folder once it is transcribed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioCleanup {
    /// Leave it where it is (remembered, so it is not transcribed again).
    #[default]
    Keep,
    /// Move it to the Trash / Recycle Bin.
    Trash,
    /// Delete it for good.
    Delete,
}

pub fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| AUDIO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// Partial downloads, hidden and temporary files are never picked up.
fn is_temporary(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    name.starts_with('.') || name.starts_with('~') || name.ends_with(".part") || name.ends_with(".crdownload") || name.ends_with(".tmp")
}

/// Applies `mode` to a transcribed file.
pub fn clean_up(path: &Path, mode: AudioCleanup) -> Result<(), String> {
    match mode {
        AudioCleanup::Keep => Ok(()),
        AudioCleanup::Trash => trash::delete(path).map_err(|e| e.to_string()),
        AudioCleanup::Delete => std::fs::remove_file(path).map_err(|e| e.to_string()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Done {
    path: String,
    size: u64,
    modified_ms: u64,
}

fn stamp(path: &Path) -> Option<(u64, u64)> {
    let meta = std::fs::metadata(path).ok()?;
    let ms = meta.modified().ok()?.duration_since(SystemTime::UNIX_EPOCH).ok()?.as_millis() as u64;
    Some((meta.len(), ms))
}

/// Watches one folder. Call `poll` every few seconds: it returns each new audio file once, after
/// the file stopped changing (a file being copied in grows or changes its time between polls).
pub struct DropFolder {
    log_path: PathBuf,
    done: Vec<Done>,
    last_seen: HashMap<PathBuf, (u64, u64)>,
    handed_out: HashSet<PathBuf>,
}

impl DropFolder {
    pub fn new(paths: &Paths) -> Self {
        let log_path = paths.support.join("imported.json");
        let done = std::fs::read_to_string(&log_path).ok().and_then(|t| json::from_str(&t)).unwrap_or_default();
        Self { log_path, done, last_seen: HashMap::new(), handed_out: HashSet::new() }
    }

    fn was_done(&self, path: &Path, stamp: (u64, u64)) -> bool {
        let key = path.to_string_lossy();
        self.done.iter().any(|d| d.path == key && (d.size, d.modified_ms) == stamp)
    }

    /// New audio files in `folder` that are complete and not handled yet, oldest first.
    pub fn poll(&mut self, folder: &Path) -> Vec<PathBuf> {
        let mut found = Vec::new();
        let mut now_seen = HashMap::new();
        for entry in std::fs::read_dir(folder).into_iter().flatten().flatten() {
            let path = entry.path();
            if !path.is_file() || !is_audio(&path) || is_temporary(&path) || self.handed_out.contains(&path) {
                continue;
            }
            let Some(current) = stamp(&path) else { continue };
            if self.was_done(&path, current) {
                continue;
            }
            if self.last_seen.get(&path) == Some(&current) && current.0 > 0 {
                found.push((current.1, path.clone()));
            }
            now_seen.insert(path, current);
        }
        self.last_seen = now_seen;
        found.sort();
        let ready: Vec<PathBuf> = found.into_iter().map(|(_, p)| p).collect();
        self.handed_out.extend(ready.iter().cloned());
        ready
    }

    /// Remembers that `path` was handled (transcribed or failed), so it is not offered again, unless
    /// the file changes.
    pub fn mark_done(&mut self, path: &Path) {
        self.handed_out.remove(path);
        if let Some((size, modified_ms)) = stamp(path) {
            self.done.retain(|d| d.path != path.to_string_lossy());
            self.done.push(Done { path: path.to_string_lossy().into_owned(), size, modified_ms });
            let _ = json::write_atomic(&self.log_path, &to_pretty_sorted(&self.done));
        }
    }

    /// The file is gone (cleaned up) or the job was dropped: forget it was handed out.
    pub fn forget(&mut self, path: &Path) {
        self.handed_out.remove(path);
    }
}
