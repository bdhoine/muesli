//! The vocabulary: names and terms Whisper should spell right ("LocalVQE", "Barry d'Hoine").
//!
//! It lives in its own file, `vocabulary.json` in the support folder, read whenever it is
//! needed, because two processes change it: the app (Settings) and `muesli-mcp` (AI assistants,
//! `update_vocabulary`). `config.json` is not reread by the app, so it cannot hold it.
//!
//! Whisper gets it as its initial prompt ([`whisper_prompt`]), which biases spelling without
//! forcing it.

use serde::{Deserialize, Serialize};

use crate::json;
use crate::paths::Paths;

pub const MAX_TERMS: usize = 200;
pub const MAX_TERM_LENGTH: usize = 60;
const SCHEMA_VERSION: u32 = 1;

/// Titles that say nothing about the meeting, so they are left out of the prompt.
const GENERIC_TITLES: [&str; 5] = ["meeting", "call", "teams call", "untitled", "audio"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Vocabulary {
    pub schema_version: u32,
    /// Oldest first; a new term goes to the end.
    pub terms: Vec<String>,
}

impl Default for Vocabulary {
    fn default() -> Self {
        Self { schema_version: SCHEMA_VERSION, terms: Vec::new() }
    }
}

/// One line, trimmed, inner whitespace collapsed.
fn clean(term: &str) -> String {
    term.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl Vocabulary {
    /// The vocabulary on disk; empty when the file is missing or unreadable.
    pub fn load(paths: &Paths) -> Self {
        std::fs::read_to_string(paths.vocabulary()).ok().and_then(|text| json::from_str(&text)).unwrap_or_default()
    }

    pub fn save(&self, paths: &Paths) -> std::io::Result<()> {
        json::write_atomic(&paths.vocabulary(), &json::to_pretty_sorted(self))
    }

    /// Adds `terms` (each trimmed to one line; empty ones and ones already there, whatever their
    /// case, are skipped). Nothing changes when one is too long or the list would grow past
    /// [`MAX_TERMS`].
    pub fn add(&mut self, terms: &[String]) -> Result<(), String> {
        let mut new: Vec<String> = Vec::new();
        for term in terms.iter().map(|t| clean(t)).filter(|t| !t.is_empty()) {
            if term.chars().count() > MAX_TERM_LENGTH {
                return Err(format!("\"{term}\" is longer than {MAX_TERM_LENGTH} characters"));
            }
            let known = |t: &String| t.to_lowercase() == term.to_lowercase();
            if !self.terms.iter().any(known) && !new.iter().any(known) {
                new.push(term);
            }
        }
        if self.terms.len() + new.len() > MAX_TERMS {
            return Err(format!("The vocabulary holds at most {MAX_TERMS} terms; remove some first"));
        }
        self.terms.extend(new);
        Ok(())
    }

    /// Removes `terms`, whatever their case. Terms that are not there are ignored.
    pub fn remove(&mut self, terms: &[String]) {
        let gone: Vec<String> = terms.iter().map(|t| clean(t).to_lowercase()).collect();
        self.terms.retain(|t| !gone.contains(&t.to_lowercase()));
    }
}

/// Whisper's initial prompt for a meeting: the vocabulary, then the meeting's own title and
/// attendee names. whisper.cpp keeps the end of a prompt that is too long, so the newest terms
/// and this meeting's names are what survive. Empty when there is nothing to say.
pub fn whisper_prompt(vocabulary: &Vocabulary, title: &str, attendees: &[String]) -> String {
    let mut words: Vec<String> = vocabulary.terms.clone();
    let title = clean(title);
    if !title.is_empty() && !GENERIC_TITLES.contains(&title.to_lowercase().as_str()) {
        words.push(title);
    }
    words.extend(attendees.iter().map(|a| clean(a)).filter(|a| !a.is_empty()));
    let mut seen: Vec<String> = Vec::new();
    words.retain(|w| {
        let key = w.to_lowercase();
        let fresh = !seen.contains(&key);
        seen.push(key);
        fresh
    });
    if words.is_empty() {
        return String::new();
    }
    format!("Glossary: {}.", words.join(", "))
}
