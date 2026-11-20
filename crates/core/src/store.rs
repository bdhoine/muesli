use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveDateTime, SecondsFormat, Utc};

use crate::json::{self, to_pretty_sorted};
use crate::models::{Meeting, MeetingState, TranscriptSegment};
use crate::paths::Paths;

/// A transcript file for the "recent transcripts" list.
#[derive(Debug, Clone, PartialEq)]
pub struct RecentTranscript {
    pub path: PathBuf,
    pub title: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

impl RecentTranscript {
    pub fn duration_minutes(&self) -> Option<i64> {
        self.ended_at.map(|e| (e - self.started_at).num_minutes().max(0))
    }
}

/// Writes each finished meeting as a pair of files in the transcripts folder:
///   2026-11-10_1430_Weekly-sync.md    readable transcript
///   2026-11-10_1430_Weekly-sync.json  same data, machine-readable
pub struct Store;

impl Store {
    /// Returns the path of the Markdown file.
    pub fn save(meeting: &Meeting, folder: &Path) -> PathBuf {
        let _ = fs::create_dir_all(folder);
        let stem = folder.join(Self::file_stem(meeting));
        let md = stem.with_extension("md");
        let _ = fs::write(&md, Self::markdown(meeting));
        let _ = json::write_atomic(&stem.with_extension("json"), &to_pretty_sorted(meeting));
        md
    }

    /// Markdown files in the transcripts folder, newest first.
    pub fn recent(folder: &Path, limit: usize) -> Vec<PathBuf> {
        let mut files: Vec<(SystemTime, PathBuf)> = fs::read_dir(folder)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "md"))
            .map(|p| (modified(&p), p))
            .collect();
        files.sort_by_key(|f| std::cmp::Reverse(f.0));
        files.into_iter().take(limit).map(|(_, p)| p).collect()
    }

    pub fn recent_transcripts(folder: &Path, limit: usize) -> Vec<RecentTranscript> {
        if limit == 0 {
            return Vec::new();
        }
        Self::recent(folder, limit)
            .into_iter()
            .map(|md| {
                let meeting: Option<Meeting> = fs::read_to_string(md.with_extension("json")).ok().and_then(|t| json::from_str(&t));
                RecentTranscript {
                    title: meeting
                        .as_ref()
                        .map(|m| m.title.clone())
                        .unwrap_or_else(|| md.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()),
                    started_at: meeting.as_ref().map_or_else(|| DateTime::<Utc>::from(modified(&md)), |m| m.started_at),
                    ended_at: meeting.and_then(|m| m.ended_at),
                    path: md,
                }
            })
            .collect()
    }

    /// `2026-11-10_1430_Weekly-sync` (local time of the start, title reduced to letters and digits).
    pub fn file_stem(meeting: &Meeting) -> String {
        let when = meeting.started_at.with_timezone(&Local).format("%Y-%m-%d_%H%M");
        let slug: String = meeting
            .title
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '-' })
            .collect::<String>()
            .split('-')
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join("-");
        let slug: String = slug.chars().take(60).collect();
        format!("{when}_{}", if slug.is_empty() { "Meeting" } else { &slug })
    }

    pub fn markdown(meeting: &Meeting) -> String {
        format!("{}\n## Transcript\n\n{}", Self::header(meeting), Self::lines(&meeting.transcript))
    }

    pub fn header(m: &Meeting) -> String {
        let iso = |d: &DateTime<Utc>| d.to_rfc3339_opts(SecondsFormat::Secs, true);
        let mut out = format!("# {}\n\n- Started: {}\n", m.title, iso(&m.started_at));
        if let Some(end) = &m.ended_at {
            out += &format!("- Ended: {}\n", iso(end));
            out += &format!("- Duration: {} min\n", (*end - m.started_at).num_seconds() / 60);
        }
        out += &format!("- Source: {}\n", m.source);
        if !m.attendees.is_empty() {
            out += &format!("- Attendees: {}\n", m.attendees.join(", "));
        }
        if m.partial == Some(true) {
            out += "- Partial: recording was interrupted\n";
        }
        out
    }

    pub fn line(s: &TranscriptSegment) -> String {
        let t = s.start as i64;
        format!("[{:02}:{:02}] **{}**: {}\n\n", t / 60, t % 60, s.speaker, s.text)
    }

    pub fn lines(segments: &[TranscriptSegment]) -> String {
        segments.iter().map(Self::line).collect()
    }
}

/// Cuts a long title in the middle (`Weekly planning…Q3 review`). Menus grow with their text.
pub fn shorten_title(title: &str, limit: usize) -> String {
    let chars: Vec<char> = title.chars().collect();
    if limit < 3 || chars.len() <= limit {
        return title.to_string();
    }
    let head = (limit - 1) * 2 / 3;
    let tail = limit - 1 - head;
    let start: String = chars[..head].iter().collect();
    let end: String = chars[chars.len() - tail..].iter().collect();
    format!("{start}…{end}")
}

/// What the menu shows beside a transcript, in local time: "32 min · Today 14:30",
/// "<1 min · Yesterday", "Mon", "17 Nov" (no length while unknown).
pub fn menu_subtitle(started: NaiveDateTime, ended: Option<NaiveDateTime>, today: NaiveDate) -> String {
    let date = started.date();
    let days = (today - date).num_days();
    let when = if days == 0 {
        format!("Today {}", started.format("%H:%M"))
    } else if days == 1 {
        "Yesterday".to_string()
    } else if (2..7).contains(&days) {
        started.format("%a").to_string()
    } else if date.year() == today.year() {
        started.format("%-d %b").to_string()
    } else {
        started.format("%-d %b %Y").to_string()
    };
    match ended {
        Some(end) => {
            let minutes = (end - started).num_minutes().max(0);
            let length = if minutes < 1 { "<1 min".to_string() } else { format!("{minutes} min") };
            format!("{length} · {when}")
        }
        None => when,
    }
}

/// How long a recording has run: "0:42", "12:07", "1:02:07".
pub fn elapsed(seconds: i64) -> String {
    let s = seconds.max(0);
    let (h, m, s) = (s / 3600, s / 60 % 60, s % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// [`menu_subtitle`] for times in UTC, shown in the computer's time zone.
pub fn menu_subtitle_local(started: DateTime<Utc>, ended: Option<DateTime<Utc>>, now: DateTime<Utc>) -> String {
    let local = |t: DateTime<Utc>| t.with_timezone(&Local).naive_local();
    menu_subtitle(local(started), ended.map(local), local(now).date())
}

fn modified(path: &Path) -> SystemTime {
    fs::metadata(path).and_then(|m| m.modified()).unwrap_or(SystemTime::UNIX_EPOCH)
}

/// Canonical copy of every meeting, kept in the support folder so the MCP server never depends
/// on where the user's transcripts folder is. Rewritten during recording.
pub struct MeetingStore<'a> {
    pub paths: &'a Paths,
}

impl<'a> MeetingStore<'a> {
    pub fn new(paths: &'a Paths) -> Self {
        Self { paths }
    }

    /// `None` for ids that could escape the folder: only letters, digits and dashes, up to 64.
    pub fn url(&self, id: &str) -> Option<PathBuf> {
        let ok = !id.is_empty() && id.chars().count() <= 64 && id.chars().all(|c| c.is_alphanumeric() || c == '-');
        ok.then(|| self.paths.meetings().join(format!("{id}.json")))
    }

    pub fn save(&self, meeting: &Meeting) {
        if let Some(path) = self.url(&meeting.id) {
            let _ = json::write_atomic(&path, &to_pretty_sorted(meeting));
        }
    }

    pub fn load(&self, id: &str) -> Option<Meeting> {
        json::from_str(&fs::read_to_string(self.url(id)?).ok()?)
    }

    /// All meetings, newest first.
    pub fn all(&self) -> Vec<Meeting> {
        let mut meetings: Vec<Meeting> = fs::read_dir(self.paths.meetings())
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .filter_map(|p| json::from_str(&fs::read_to_string(p).ok()?))
            .collect();
        meetings.sort_by_key(|m| std::cmp::Reverse(m.started_at));
        meetings
    }

    /// A crash or forced quit mid-recording leaves a meeting in `recording` or `finalizing`.
    /// Keeps what was transcribed (marked partial, exported to the transcripts folder) and
    /// marks empty ones failed. Returns the meetings it changed.
    pub fn recover_interrupted(&self) -> Vec<Meeting> {
        let mut changed = Vec::new();
        for mut m in self.all() {
            if !matches!(m.state, MeetingState::Recording | MeetingState::Finalizing) {
                continue;
            }
            if m.transcript.is_empty() {
                m.state = MeetingState::Failed;
            } else {
                m.transcript.sort_by(|a, b| a.start.total_cmp(&b.start));
                let last_end = m.transcript.iter().map(|s| s.end).fold(0.0, f64::max);
                m.ended_at = m.ended_at.or(Some(m.started_at + chrono::Duration::milliseconds((last_end * 1000.0) as i64)));
                m.state = MeetingState::Ready;
                m.partial = Some(true);
                Store::save(&m, self.paths.transcripts_dir());
            }
            self.save(&m);
            changed.push(m);
        }
        changed
    }

    /// Copies transcripts found in a folder (older exports, or another machine's) into the store.
    pub fn import_transcripts(&self, folder: &Path) -> Vec<Meeting> {
        let mut imported = Vec::new();
        for entry in fs::read_dir(folder).into_iter().flatten().flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "json") {
                continue;
            }
            let Some(m) = fs::read_to_string(&path).ok().and_then(|t| json::from_str::<Meeting>(&t)) else {
                continue;
            };
            let Some(target) = self.url(&m.id) else { continue };
            if target.exists() {
                continue;
            }
            self.save(&m);
            imported.push(m);
        }
        imported
    }

    /// Adds freshly transcribed segments to a meeting that is being recorded and saves it, so the
    /// transcript survives a crash and can be read while the meeting is still going on.
    pub fn append(&self, meeting: &mut Meeting, segments: Vec<TranscriptSegment>) {
        if segments.is_empty() {
            return;
        }
        meeting.transcript.extend(segments);
        meeting.transcript.sort_by(|a, b| a.start.total_cmp(&b.start));
        // The two streams arrive in either order, so look again each time.
        crate::echo::drop_echoes(&mut meeting.transcript);
        self.save(meeting);
    }

    /// Ends a recording: marks the meeting ready, saves it and exports the transcript files.
    /// Returns the path of the Markdown file.
    pub fn finish(&self, meeting: &mut Meeting, now: DateTime<Utc>) -> PathBuf {
        meeting.ended_at = Some(now);
        meeting.transcript.sort_by(|a, b| a.start.total_cmp(&b.start));
        meeting.state = MeetingState::Ready;
        self.save(meeting);
        Store::save(meeting, self.paths.transcripts_dir())
    }

    pub fn latest_ready(&self) -> Option<Meeting> {
        self.all().into_iter().filter(|m| m.state == MeetingState::Ready).max_by_key(|m| m.ended_at.unwrap_or(m.started_at))
    }
}

#[cfg(test)]
mod menu_text_tests {
    use super::*;

    fn at(d: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(d, "%Y-%m-%d %H:%M").unwrap()
    }

    #[test]
    fn subtitles_give_the_length_then_the_day() {
        let today = NaiveDate::from_ymd_opt(2026, 11, 20).unwrap();
        assert_eq!(menu_subtitle(at("2026-11-20 14:30"), Some(at("2026-11-20 15:02")), today), "32 min · Today 14:30");
        assert_eq!(menu_subtitle(at("2026-11-19 09:00"), Some(at("2026-11-19 09:00")), today), "<1 min · Yesterday");
        assert_eq!(menu_subtitle(at("2026-11-16 10:00"), Some(at("2026-11-16 10:32")), today), "32 min · Mon");
        assert_eq!(menu_subtitle(at("2026-11-12 10:00"), None, today), "12 Nov");
        assert_eq!(menu_subtitle(at("2025-12-31 23:00"), None, today), "31 Dec 2025");
    }

    #[test]
    fn elapsed_is_a_clock() {
        assert_eq!(elapsed(42), "0:42");
        assert_eq!(elapsed(727), "12:07");
        assert_eq!(elapsed(3727), "1:02:07");
        assert_eq!(elapsed(-5), "0:00");
    }
}
