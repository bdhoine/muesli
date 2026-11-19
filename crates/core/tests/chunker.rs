use std::sync::{Arc, Mutex};

use muesli_core::{AudioChunk, AudioChunker, TranscriptSegment};

fn collect(chunk_seconds: f64, rate: f64) -> (AudioChunker, Arc<Mutex<Vec<AudioChunk>>>) {
    let out = Arc::new(Mutex::new(Vec::new()));
    let sink = out.clone();
    let chunker = AudioChunker::with_options(chunk_seconds, rate, &["You", "Others"], move |c| sink.lock().unwrap().push(c));
    (chunker, out)
}

#[test]
fn nothing_is_emitted_before_a_full_chunk() {
    let (c, out) = collect(2.0, 10.0);
    c.append("You", &[0.0; 19]);
    assert!(out.lock().unwrap().is_empty());
}

#[test]
fn emits_full_chunks_with_running_offsets() {
    let (c, out) = collect(2.0, 10.0);
    c.append("You", &[0.1; 45]);
    let chunks = out.lock().unwrap();
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].offset_seconds, 0.0);
    assert_eq!(chunks[1].offset_seconds, 2.0);
    assert!(chunks.iter().all(|c| c.samples.len() == 20));
}

#[test]
fn speakers_are_chunked_independently() {
    let (c, out) = collect(2.0, 10.0);
    c.append("You", &[0.0; 15]);
    c.append("Others", &[0.0; 20]);
    c.append("You", &[0.0; 5]);
    let chunks = out.lock().unwrap();
    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].speaker, "Others");
    assert_eq!(chunks[1].speaker, "You");
}

#[test]
fn finish_emits_the_tail_but_drops_under_one_second() {
    let (c, out) = collect(2.0, 10.0);
    c.append("You", &[0.0; 25]); // one full chunk and a 0.5 s tail
    c.append("Others", &[0.0; 15]); // a 1.5 s tail
    c.finish();
    let chunks = out.lock().unwrap();
    let you: Vec<_> = chunks.iter().filter(|c| c.speaker == "You").collect();
    let others: Vec<_> = chunks.iter().filter(|c| c.speaker == "Others").collect();
    assert_eq!(you.len(), 1);
    assert_eq!(others.len(), 1);
    assert_eq!(others[0].samples.len(), 15);
}

#[test]
fn defaults_are_thirty_seconds_at_sixteen_kilohertz() {
    let out = Arc::new(Mutex::new(Vec::new()));
    let sink = out.clone();
    let c = AudioChunker::new(move |c| sink.lock().unwrap().push(c));
    c.append("You", &vec![0.0; 16_000 * 30 - 1]);
    assert!(out.lock().unwrap().is_empty());
    c.append("You", &[0.0]);
    assert_eq!(out.lock().unwrap().len(), 1);
}

#[test]
fn concurrent_appends_lose_nothing() {
    let (c, out) = collect(1.0, 100.0);
    let c = Arc::new(c);
    let handles: Vec<_> = ["You", "Others"]
        .into_iter()
        .map(|speaker| {
            let c = c.clone();
            std::thread::spawn(move || {
                for _ in 0..100 {
                    c.append(speaker, &[0.0; 10]);
                }
            })
        })
        .collect();
    handles.into_iter().for_each(|h| h.join().unwrap());
    let total: usize = out.lock().unwrap().iter().map(|c| c.samples.len()).sum();
    assert_eq!(total, 2000);
}

#[test]
fn whisper_times_are_milliseconds_relative_to_the_chunk() {
    let s = TranscriptSegment::from_whisper("You", 30.0, 1500.0, 4200.0, "  Hello  ").unwrap();
    assert_eq!((s.start, s.end, s.text.as_str()), (31.5, 34.2, "Hello"));
}

#[test]
fn blank_and_empty_segments_are_dropped() {
    assert!(TranscriptSegment::from_whisper("You", 0.0, 0.0, 1.0, "[BLANK_AUDIO]").is_none());
    assert!(TranscriptSegment::from_whisper("You", 0.0, 0.0, 1.0, "  \n").is_none());
}
