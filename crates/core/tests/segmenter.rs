//! Pause-based blocks: scripted speech and silence through the energy detector.

use std::collections::HashMap;

use muesli_core::languages;
use muesli_core::segmenter::{
    EnergyDetector, Pacer, Segmenter, SpeechDetector, FRAME, MAX_SECONDS, MAX_SECONDS_BEHIND, PAD_SECONDS, SAMPLE_RATE,
};
use muesli_core::AudioChunk;

const R: f64 = SAMPLE_RATE as f64;

fn speech(seconds: f64) -> Vec<f32> {
    (0..(seconds * R) as usize).map(|i| 0.1 * (i as f32 * 0.05).sin()).collect()
}

/// Speech whose loudness dips to `quiet` for one frame at `dip` seconds.
fn speech_with_dip(seconds: f64, dip: f64) -> Vec<f32> {
    let mut s = speech(seconds);
    let at = (dip * R) as usize / FRAME * FRAME;
    for x in &mut s[at..at + FRAME] {
        *x *= 0.2; // still speech, but the quietest frame
    }
    s
}

fn silence(seconds: f64) -> Vec<f32> {
    vec![0.0; (seconds * R) as usize]
}

fn segmenter() -> Segmenter {
    Segmenter::new("You", Box::new(EnergyDetector::default()))
}

/// Feeds the stream in small callbacks, as capture does, and returns every block, finish included.
fn run(seg: &mut Segmenter, parts: &[Vec<f32>]) -> Vec<AudioChunk> {
    let mut out = Vec::new();
    for part in parts {
        for piece in part.chunks(441) {
            out.extend(seg.push(piece));
        }
    }
    out.extend(seg.finish());
    out
}

fn seconds(c: &AudioChunk) -> f64 {
    c.samples.len() as f64 / R
}

fn end(c: &AudioChunk) -> f64 {
    c.offset_seconds + seconds(c)
}

#[test]
fn silence_gives_no_blocks() {
    let out = run(&mut segmenter(), &[silence(1800.0)]);
    assert!(out.is_empty());
}

#[test]
fn a_pause_after_speech_sends_the_sentence() {
    let mut seg = segmenter();
    let mut out = Vec::new();
    for piece in [silence(2.0), speech(3.0), silence(1.6)].concat().chunks(441) {
        out.extend(seg.push(piece));
    }
    assert_eq!(out.len(), 1, "sent before stop, after the pause");
    let block = &out[0];
    assert!((block.offset_seconds - (2.0 - PAD_SECONDS)).abs() < 0.04, "starts with the padding: {}", block.offset_seconds);
    assert!((end(block) - (5.0 + PAD_SECONDS)).abs() < 0.04, "ends with the padding: {}", end(block));
    assert!(seg.finish().is_empty());
}

#[test]
fn offsets_are_exact_on_the_stream_timeline() {
    let out = run(&mut segmenter(), &[silence(7.0), speech(2.0), silence(3.0), speech(4.0), silence(2.0)]);
    assert_eq!(out.len(), 2);
    // Each block's samples are the stream's samples at its offset.
    let stream = [silence(7.0), speech(2.0), silence(3.0), speech(4.0), silence(2.0)].concat();
    for block in &out {
        let at = (block.offset_seconds * R).round() as usize;
        assert_eq!(&stream[at..at + block.samples.len()], block.samples.as_slice());
    }
    assert!((out[1].offset_seconds - (12.0 - PAD_SECONDS)).abs() < 0.04);
}

#[test]
fn continuous_speech_is_cut_at_most_every_ten_seconds_at_the_quietest_moment() {
    let out = run(&mut segmenter(), &[speech_with_dip(25.0, 9.0)]);
    assert!(out.iter().all(|b| seconds(b) <= MAX_SECONDS + 0.01), "{:?}", out.iter().map(seconds).collect::<Vec<_>>());
    assert!((end(&out[0]) - (9.0 + FRAME as f64 / R)).abs() < 0.04, "cut after the dip: {}", end(&out[0]));
    // Blocks follow each other without gaps or overlap.
    for pair in out.windows(2) {
        assert!((end(&pair[0]) - pair[1].offset_seconds).abs() < 1e-9);
    }
    let total: f64 = out.iter().map(seconds).sum();
    assert!((total - 25.0).abs() < 0.04, "nothing lost: {total}");
}

#[test]
fn short_speech_is_held_and_joined_to_the_next_block() {
    let out = run(&mut segmenter(), &[speech(0.5), silence(2.0), speech(3.0), silence(2.0)]);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].offset_seconds, 0.0);
    assert!(end(&out[0]) > 5.5);
}

#[test]
fn a_short_utterance_alone_is_sent_once_the_silence_goes_on() {
    let out = run(&mut segmenter(), &[silence(1.0), speech(0.6), silence(8.0)]);
    assert_eq!(out.len(), 1, "\"Yes.\" on its own");
    assert!((out[0].offset_seconds - (1.0 - PAD_SECONDS)).abs() < 0.04);
    assert!(seconds(&out[0]) < 1.2);
}

#[test]
fn stop_sends_the_rest_whatever_the_pause() {
    let mut seg = segmenter();
    let mut out = Vec::new();
    for piece in [silence(1.0), speech(4.3)].concat().chunks(441) {
        out.extend(seg.push(piece));
    }
    assert!(out.is_empty(), "no pause yet");
    let tail = seg.finish();
    assert_eq!(tail.len(), 1);
    assert!((end(&tail[0]) - 5.3).abs() < 0.001, "every sample, partial frame included: {}", end(&tail[0]));
}

#[test]
fn tails_of_a_second_or_less_are_dropped_on_stop() {
    let out = run(&mut segmenter(), &[silence(3.0), speech(0.7)]);
    assert!(out.is_empty());
}

#[test]
fn speakers_are_independent() {
    let mut you = Segmenter::new("You", Box::new(EnergyDetector::default()));
    let mut others = Segmenter::new("Others", Box::new(EnergyDetector::default()));
    let a = run(&mut you, &[speech(2.0), silence(2.0)]);
    let b = run(&mut others, &[silence(5.0), speech(3.0), silence(2.0)]);
    assert_eq!(a.len(), 1);
    assert_eq!(b.len(), 1);
    assert_eq!(a[0].speaker, "You");
    assert_eq!(b[0].speaker, "Others");
    assert!(b[0].offset_seconds > 4.0);
}

#[test]
fn when_behind_blocks_grow_and_pauses_wait_for_the_longer_block() {
    let mut seg = segmenter();
    seg.set_max_seconds(MAX_SECONDS_BEHIND);
    let parts: Vec<Vec<f32>> = (0..8).flat_map(|_| [speech(3.0), silence(1.5)]).collect();
    let out = run(&mut seg, &parts);
    assert!(out[0].samples.len() as f64 / R >= 20.0, "first block {}", seconds(&out[0]));
    assert!(out.iter().all(|b| seconds(b) <= MAX_SECONDS_BEHIND + 0.01));
}

/// A detector that never answers: everything is treated as speech, so Whisper's own voice
/// detection still decides, as before pause-based blocks.
struct Broken;
impl SpeechDetector for Broken {
    fn probabilities(&mut self, _: &[f32]) -> Result<Vec<f32>, String> {
        Err("no model".into())
    }
}

#[test]
fn a_failing_detector_sends_everything_in_maximum_blocks() {
    let out = run(&mut Segmenter::new("You", Box::new(Broken)), &[silence(25.0)]);
    assert_eq!(out.len(), 3);
    assert!(out.iter().all(|b| seconds(b) <= MAX_SECONDS + 0.01));
}

/// The detector sees warm-up audio in front of each new step, and the window is whole frames.
struct Recording(std::sync::Arc<std::sync::Mutex<Vec<usize>>>);
impl SpeechDetector for Recording {
    fn probabilities(&mut self, window: &[f32]) -> Result<Vec<f32>, String> {
        assert_eq!(window.len() % FRAME, 0);
        self.0.lock().unwrap().push(window.len() / FRAME);
        Ok(vec![0.0; window.len() / FRAME])
    }
}

#[test]
fn the_detector_gets_steps_with_warm_up_in_front() {
    let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut seg = Segmenter::new("You", Box::new(Recording(calls.clone())));
    run(&mut seg, &[silence(4.0)]);
    let calls = calls.lock().unwrap();
    assert_eq!(calls[0], 16, "the first step has nothing in front");
    assert_eq!(calls[1], 32);
    assert!(calls[2..calls.len() - 1].iter().all(|n| *n == 48), "{calls:?}");
}

#[test]
fn the_pacer_lengthens_blocks_when_whisper_is_slow_and_shortens_them_again() {
    let mut p = Pacer::default();
    assert_eq!(p.max_seconds(), MAX_SECONDS);
    for _ in 0..10 {
        p.record(10.0, 12.0);
    }
    assert_eq!(p.max_seconds(), MAX_SECONDS_BEHIND);
    for _ in 0..20 {
        p.record(30.0, 3.0);
    }
    assert_eq!(p.max_seconds(), MAX_SECONDS);
    p.record(10.0, 6.0); // in between: stays
    assert_eq!(p.max_seconds(), MAX_SECONDS);
}

fn probs(pairs: &[(&str, f32)]) -> HashMap<String, f32> {
    pairs.iter().map(|(c, p)| (c.to_string(), *p)).collect()
}

#[test]
fn a_short_block_keeps_the_previous_language_unless_detection_is_confident() {
    let allowed = vec!["en".to_string(), "nl".to_string()];
    let unsure = probs(&[("en", 0.5), ("nl", 0.3)]);
    let sure = probs(&[("en", 0.9), ("nl", 0.05)]);
    assert_eq!(languages::choose(&unsure, &allowed, Some("nl"), 1.5).as_deref(), Some("nl"));
    assert_eq!(languages::choose(&sure, &allowed, Some("nl"), 1.5).as_deref(), Some("en"));
    assert_eq!(languages::choose(&unsure, &allowed, Some("nl"), 8.0).as_deref(), Some("en"), "long blocks decide afresh");
    assert_eq!(languages::choose(&unsure, &allowed, None, 1.5).as_deref(), Some("en"));
    assert_eq!(languages::choose(&unsure, &allowed, Some("fr"), 1.5).as_deref(), Some("en"), "a language no longer allowed");
}
