use std::sync::{Arc, Mutex};

use muesli_core::{AudioChunk, AudioChunker, Pauses, TranscriptSegment};

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

// ---- cut at pauses (live transcription) ----

/// 1 kHz, so a 20 ms frame is 20 samples and a second is 1000 samples.
fn live() -> (AudioChunker, Arc<Mutex<Vec<AudioChunk>>>) {
    let out = Arc::new(Mutex::new(Vec::new()));
    let sink = out.clone();
    let pauses = Pauses { pause_seconds: 0.8, min_seconds: 2.0, max_seconds: 10.0, silence_rms: 0.01 };
    (AudioChunker::with_pauses(pauses, 1000.0, &["You", "Others"], move |c| sink.lock().unwrap().push(c)), out)
}

fn speech(seconds: f64) -> Vec<f32> {
    vec![0.3; (seconds * 1000.0) as usize]
}

fn silence(seconds: f64) -> Vec<f32> {
    vec![0.0; (seconds * 1000.0) as usize]
}

#[test]
fn a_sentence_is_sent_when_the_speaker_pauses() {
    let (c, out) = live();
    c.append("You", &speech(3.0));
    assert!(out.lock().unwrap().is_empty(), "still talking");
    c.append("You", &silence(0.5));
    assert!(out.lock().unwrap().is_empty(), "a short pause does not end it");
    c.append("You", &silence(0.4));
    let chunks = out.lock().unwrap();
    assert_eq!(chunks.len(), 1, "after 0.8 s of silence");
    assert_eq!(chunks[0].offset_seconds, 0.0);
    assert_eq!(chunks[0].samples.len(), 3900);
}

#[test]
fn a_short_pause_inside_a_sentence_does_not_split_it() {
    let (c, out) = live();
    c.append("You", &speech(2.0));
    c.append("You", &silence(0.5));
    c.append("You", &speech(2.0));
    assert!(out.lock().unwrap().is_empty());
}

#[test]
fn a_chunk_needs_two_seconds_before_a_pause_ends_it() {
    let (c, out) = live();
    c.append("You", &speech(0.8));
    c.append("You", &silence(0.9));
    assert!(out.lock().unwrap().is_empty(), "1.7 s is too short to send yet");
    c.append("You", &silence(0.4)); // the quiet goes on: now it is long enough
    assert_eq!(out.lock().unwrap().len(), 1);
}

#[test]
fn silence_is_not_sent_and_the_next_chunk_keeps_its_place_in_time() {
    let (c, out) = live();
    c.append("Others", &silence(20.0));
    assert!(out.lock().unwrap().is_empty());
    c.append("Others", &speech(3.0));
    c.append("Others", &silence(1.0));
    let chunks = out.lock().unwrap();
    assert_eq!(chunks.len(), 1);
    // 20 s of silence, minus the 0.3 s kept in front of the speech
    assert!((chunks[0].offset_seconds - 19.7).abs() < 0.05, "offset {}", chunks[0].offset_seconds);
}

#[test]
fn endless_speech_is_cut_at_ten_seconds_at_its_quietest_moment() {
    let (c, out) = live();
    let mut audio = speech(8.0);
    audio.extend(vec![0.02; 100]); // a dip of 0.1 s at 8 s, still above the silence level
    audio.extend(speech(2.5));
    c.append("You", &audio);
    let chunks = out.lock().unwrap();
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].samples.len(), 8100, "cut right after the dip, not mid-word at 10 s");
    drop(chunks);
    // the rest carries on from there
    c.append("You", &speech(0.5));
    c.append("You", &silence(1.0));
    let chunks = out.lock().unwrap();
    assert_eq!(chunks.len(), 2);
    assert!((chunks[1].offset_seconds - 8.1).abs() < 0.001);
}

#[test]
fn finish_sends_the_last_words_but_not_silence_or_a_blip() {
    let (c, out) = live();
    c.append("You", &speech(1.0));
    c.append("Others", &silence(2.0));
    c.finish();
    let chunks = out.lock().unwrap();
    assert_eq!(chunks.len(), 1);
    assert_eq!(chunks[0].speaker, "You");

    let (c, out) = live();
    c.append("You", &speech(0.3));
    c.finish();
    assert!(out.lock().unwrap().is_empty(), "under half a second is dropped");
}
