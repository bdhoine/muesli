use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

use muesli_core::store::MeetingStore;
use muesli_core::{AudioChunk, MeetingState, Paths, TranscriptSegment};
use muesli_import::{import_file, title_for, ImportError, SpeechToText};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures/audio").join(name)
}

fn paths() -> (tempfile::TempDir, Paths) {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(dir.path().join("support"), dir.path().join("transcripts"));
    std::fs::create_dir_all(&paths.support).unwrap();
    std::fs::create_dir_all(paths.transcripts_dir()).unwrap();
    (dir, paths)
}

/// Says "heard N" for every chunk, with the chunk's offset, and remembers what it was given.
struct Stub {
    seen: Mutex<Vec<(f64, usize)>>,
    fail: bool,
}

impl SpeechToText for Stub {
    fn transcribe(&self, chunk: &AudioChunk) -> Result<Vec<TranscriptSegment>, String> {
        if self.fail {
            return Err("model exploded".into());
        }
        let mut seen = self.seen.lock().unwrap();
        seen.push((chunk.offset_seconds, chunk.samples.len()));
        Ok(vec![TranscriptSegment {
            start: chunk.offset_seconds,
            end: chunk.offset_seconds + 1.0,
            speaker: chunk.speaker.clone(),
            text: format!("heard {}", seen.len()),
        }])
    }
}

fn stub() -> Stub {
    Stub { seen: Mutex::new(Vec::new()), fail: false }
}

#[test]
fn a_file_becomes_a_ready_meeting_with_its_transcript_files() {
    let (_dir, paths) = paths();
    let stt = stub();
    let mut last = None;
    let meeting = import_file(&fixture("tone-mono-44k.mp3"), &stt, &paths, &AtomicBool::new(false), |p| last = p).unwrap();
    assert_eq!(meeting.title, "tone-mono-44k");
    assert_eq!(meeting.source, "file");
    assert_eq!(meeting.state, MeetingState::Ready);
    assert_eq!(meeting.transcript.len(), 1); // one second: a single short chunk
    assert_eq!(meeting.transcript[0].speaker, "Speaker");
    assert_eq!(last, Some(1.0));
    let stored = MeetingStore::new(&paths).load(&meeting.id).unwrap();
    assert_eq!(stored.state, MeetingState::Ready);
    let files: Vec<_> = std::fs::read_dir(paths.transcripts_dir()).unwrap().flatten().collect();
    assert_eq!(files.len(), 2, "a .md and a .json");
    // the meeting is dated by the file, and lasts as long as the audio
    let seconds = (meeting.ended_at.unwrap() - meeting.started_at).num_milliseconds() as f64 / 1000.0;
    assert!((0.9..=1.1).contains(&seconds), "{seconds}");
}

#[test]
fn a_chunk_that_is_too_short_to_matter_is_skipped_but_half_a_second_counts() {
    let (_dir, paths) = paths();
    let stt = stub();
    import_file(&fixture("tone-stereo-48k.wav"), &stt, &paths, &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(stt.seen.lock().unwrap().len(), 1);
}

#[test]
fn a_failing_transcriber_leaves_a_failed_meeting() {
    let (_dir, paths) = paths();
    let stt = Stub { seen: Mutex::new(Vec::new()), fail: true };
    let err = import_file(&fixture("tone-mono-44k.mp3"), &stt, &paths, &AtomicBool::new(false), |_| {}).unwrap_err();
    assert!(matches!(err, ImportError::Transcribe(ref why) if why == "model exploded"));
    let all = MeetingStore::new(&paths).all();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].state, MeetingState::Failed);
}

#[test]
fn cancelling_leaves_nothing_when_nothing_was_transcribed() {
    let (_dir, paths) = paths();
    let cancel = AtomicBool::new(true);
    let err = import_file(&fixture("tone-mono-44k.mp3"), &stub(), &paths, &cancel, |_| {}).unwrap_err();
    assert_eq!(err, ImportError::Cancelled);
    assert!(MeetingStore::new(&paths).all().is_empty());
    assert!(std::fs::read_dir(paths.transcripts_dir()).unwrap().next().is_none());
}

#[test]
fn a_file_that_is_not_audio_gives_a_failed_meeting_and_a_clear_error() {
    let (dir, paths) = paths();
    let path = dir.path().join("notes.mp3");
    std::fs::write(&path, b"nope").unwrap();
    let err = import_file(&path, &stub(), &paths, &AtomicBool::new(false), |_| {}).unwrap_err();
    assert!(matches!(err, ImportError::Decode(_)));
    assert!(err.to_string().contains("cannot be read"));
}

#[test]
fn titles_come_from_the_file_name() {
    assert_eq!(title_for(&PathBuf::from("/x/Weekly sync 2024.m4a")), "Weekly sync 2024");
    assert_eq!(title_for(&PathBuf::from("/x/.mp3")), ".mp3");
}

/// A 16 kHz mono 16-bit WAV of silence.
fn silence_wav(path: &std::path::Path, seconds: usize) {
    let data = vec![0u8; seconds * 16_000 * 2];
    let mut out = Vec::new();
    out.extend(b"RIFF");
    out.extend((36 + data.len() as u32).to_le_bytes());
    out.extend(b"WAVEfmt ");
    out.extend(16u32.to_le_bytes());
    out.extend(1u16.to_le_bytes()); // PCM
    out.extend(1u16.to_le_bytes()); // mono
    out.extend(16_000u32.to_le_bytes());
    out.extend(32_000u32.to_le_bytes());
    out.extend(2u16.to_le_bytes());
    out.extend(16u16.to_le_bytes());
    out.extend(b"data");
    out.extend((data.len() as u32).to_le_bytes());
    out.extend(data);
    std::fs::write(path, out).unwrap();
}

#[test]
fn a_long_file_is_cut_into_30_second_chunks_with_running_offsets() {
    let (dir, paths) = paths();
    let wav = dir.path().join("long.wav");
    silence_wav(&wav, 70);
    let stt = stub();
    let mut steps = Vec::new();
    let meeting = import_file(&wav, &stt, &paths, &AtomicBool::new(false), |p| steps.push(p)).unwrap();
    let seen = stt.seen.lock().unwrap().clone();
    assert_eq!(seen.iter().map(|(offset, _)| *offset).collect::<Vec<_>>(), [0.0, 30.0, 60.0]);
    assert_eq!(seen[0].1, 30 * 16_000);
    assert!(seen[2].1 >= 9 * 16_000 && seen[2].1 <= 10 * 16_000 + 16_000); // the 10 s tail
    assert_eq!(meeting.transcript.iter().map(|s| s.start).collect::<Vec<_>>(), [0.0, 30.0, 60.0]);
    // progress only moves forward and ends at 1
    let fractions: Vec<f64> = steps.iter().flatten().copied().collect();
    assert!(fractions.windows(2).all(|w| w[0] <= w[1]) && *fractions.last().unwrap() == 1.0);
}

#[test]
fn cancelling_part_way_keeps_what_was_transcribed() {
    let (dir, paths) = paths();
    let wav = dir.path().join("long.wav");
    silence_wav(&wav, 70);
    let cancel = AtomicBool::new(false);
    let err = import_file(&wav, &stub(), &paths, &cancel, |_| cancel.store(true, std::sync::atomic::Ordering::Relaxed)).unwrap_err();
    assert_eq!(err, ImportError::Cancelled);
    let all = MeetingStore::new(&paths).all();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].partial, Some(true));
    assert!(!all[0].transcript.is_empty());
}
