//! Needs a real model and a 16 kHz mono 16-bit WAV, so it only runs on request:
//!   MUESLI_TEST_MODEL=~/Library/Application\ Support/Muesli/ggml-small.bin \
//!   MUESLI_TEST_WAV=/path/to/speech.wav cargo test -p muesli-whisper -- --ignored
#![cfg(feature = "transcribe")]

use muesli_core::AudioChunk;
use muesli_whisper::Transcriber;

fn read_wav(path: &str) -> Vec<f32> {
    let bytes = std::fs::read(path).unwrap();
    let data = bytes.windows(4).position(|w| w == b"data").expect("data chunk") + 8;
    bytes[data..].as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b) as f32 / 32768.0).collect()
}

#[test]
#[ignore]
fn transcribes_real_speech_with_language_detection() {
    let model = std::env::var("MUESLI_TEST_MODEL").expect("MUESLI_TEST_MODEL");
    let wav = std::env::var("MUESLI_TEST_WAV").expect("MUESLI_TEST_WAV");
    let transcriber = Transcriber::new(std::path::Path::new(&model), vec!["en".into(), "nl".into()]).expect("model loads");
    let chunk = AudioChunk { speaker: "You".into(), offset_seconds: 30.0, samples: read_wav(&wav) };
    let segments = transcriber.transcribe(&chunk).unwrap();
    let text: String = segments.iter().map(|s| s.text.as_str()).collect::<Vec<_>>().join(" ").to_lowercase();
    println!("{text}");
    assert!(segments.iter().all(|s| s.start >= 30.0 && s.speaker == "You"));
    assert!(text.contains("meeting"), "got: {text}");
}

#[test]
fn missing_model_gives_none() {
    assert!(Transcriber::new(std::path::Path::new("/nonexistent/model.bin"), vec!["en".into()]).is_none());
}
