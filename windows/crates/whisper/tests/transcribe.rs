//! Needs real models and a 16 kHz mono 16-bit WAV with the word "meeting" in it, so these only
//! run on request:
//!   MUESLI_TEST_MODEL=~/Library/Application\ Support/Muesli/ggml-large-v3-turbo-q5_0.bin \
//!   MUESLI_TEST_VAD=~/Library/Application\ Support/Muesli/ggml-silero-v5.1.2.bin \
//!   MUESLI_TEST_WAV=/path/to/speech.wav cargo test -p muesli-whisper -- --ignored --nocapture
//! (`say -o s.aiff "This is a test of the meeting transcriber"; afconvert -f WAVE -d LEI16@16000 -c 1 s.aiff s.wav`)
#![cfg(feature = "transcribe")]

use std::path::PathBuf;

use muesli_core::AudioChunk;
use muesli_whisper::Transcriber;

const RATE: usize = 16_000;

fn env(name: &str) -> PathBuf {
    let value = std::env::var(name).unwrap_or_else(|_| panic!("{name} is not set"));
    PathBuf::from(value.replacen("~", &std::env::var("HOME").unwrap(), 1))
}

fn read_wav(path: &std::path::Path) -> Vec<f32> {
    let bytes = std::fs::read(path).unwrap();
    let data = bytes.windows(4).position(|w| w == b"data").expect("data chunk") + 8;
    bytes[data..].as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b) as f32 / 32768.0).collect()
}

fn transcriber(with_vad: bool) -> Transcriber {
    let vad = env("MUESLI_TEST_VAD");
    Transcriber::new(&env("MUESLI_TEST_MODEL"), with_vad.then_some(vad.as_path()), vec!["en".into(), "nl".into()])
        .expect("models load")
}

fn text(t: &Transcriber, samples: Vec<f32>, offset: f64) -> (String, Vec<(f64, f64)>) {
    let chunk = AudioChunk { speaker: "You".into(), offset_seconds: offset, samples };
    let segments = t.transcribe(&chunk).unwrap();
    let words = segments.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ");
    (words.to_lowercase(), segments.iter().map(|s| (s.start, s.end)).collect())
}

fn silence(seconds: usize) -> Vec<f32> {
    vec![0.0; seconds * RATE]
}

/// Quiet room noise: deterministic white noise at 2% of full scale.
fn noise(seconds: usize) -> Vec<f32> {
    let mut x = 0x2545_F491u32;
    (0..seconds * RATE)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            (x as f32 / u32::MAX as f32 - 0.5) * 0.04
        })
        .collect()
}

/// A 120 Hz mains hum with a little noise: fans and air conditioning sound like this.
fn hum(seconds: usize) -> Vec<f32> {
    noise(seconds)
        .into_iter()
        .enumerate()
        .map(|(i, n)| n + 0.05 * (2.0 * std::f32::consts::PI * 120.0 * i as f32 / RATE as f32).sin())
        .collect()
}

#[test]
#[ignore]
fn transcribes_real_speech_with_language_detection() {
    let t = transcriber(true);
    let (words, spans) = text(&t, read_wav(&env("MUESLI_TEST_WAV")), 30.0);
    println!("{words}");
    assert!(spans.iter().all(|(start, _)| *start >= 30.0));
    assert!(words.contains("meeting"), "got: {words}");
}

#[test]
#[ignore]
fn silence_and_noise_give_no_text() {
    let t = transcriber(true);
    for (name, audio) in [("silence", silence(30)), ("room noise", noise(30)), ("hum", hum(30))] {
        let (words, _) = text(&t, audio, 0.0);
        assert!(words.is_empty(), "{name} was transcribed as: {words:?}");
    }
}

#[test]
#[ignore]
fn speech_around_a_long_pause_keeps_both_sentences_in_place() {
    let speech = read_wav(&env("MUESLI_TEST_WAV"));
    let seconds = speech.len() as f64 / RATE as f64;
    let mut audio = speech.clone();
    audio.extend(silence(20));
    audio.extend(&speech);
    assert!(audio.len() <= 30 * RATE, "the test sentence is too long ({seconds:.1} s)");
    let (words, spans) = text(&transcriber(true), audio, 0.0);
    println!("{words} {spans:?}");
    assert_eq!(words.matches("meeting").count(), 2, "got: {words}");
    let second = spans.last().unwrap();
    assert!(second.0 >= seconds + 19.0, "the second sentence starts at {:.1} s, not after the pause", second.0);
}

/// Shows what Whisper does with the same audio when voice detection is off (no assertion).
#[test]
#[ignore]
fn without_voice_detection_for_comparison() {
    let t = transcriber(false);
    for (name, audio) in [("silence", silence(30)), ("room noise", noise(30)), ("hum", hum(30))] {
        let (words, _) = text(&t, audio, 0.0);
        println!("WITHOUT VAD, {name}: {words:?}");
    }
}

#[test]
fn missing_models_give_none() {
    let nothing = std::path::Path::new("/nonexistent/model.bin");
    assert!(Transcriber::new(nothing, None, vec!["en".into()]).is_none());
}
