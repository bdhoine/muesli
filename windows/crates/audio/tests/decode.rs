//! Every supported container comes out as one second of 16 kHz mono audio, whatever its rate and channels.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use muesli_audio::{decode_file, DecodeError};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../fixtures/audio").join(name)
}

fn decode(name: &str) -> (Vec<f32>, f64) {
    let mut samples = Vec::new();
    let seconds = decode_file(&fixture(name), &AtomicBool::new(false), |block, _| samples.extend_from_slice(block)).unwrap();
    (samples, seconds)
}

#[test]
fn every_format_becomes_one_second_of_16k_mono() {
    for name in ["tone-stereo-48k.wav", "tone-mono-44k.mp3", "tone-stereo-44k.m4a", "tone-mono-22k.flac", "tone-mono-48k.ogg"] {
        let (samples, seconds) = decode(name);
        assert!((0.9..=1.1).contains(&seconds), "{name}: {seconds} s");
        assert_eq!(samples.len() as f64 / 16_000.0, seconds, "{name}");
        // ffmpeg's sine has amplitude 1/8 (RMS 0.088), 3 dB less when it fills two channels
        let rms = (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt();
        assert!((0.05..=0.10).contains(&rms), "{name}: rms {rms}");
    }
}

#[test]
fn the_tone_keeps_its_pitch_after_resampling() {
    let (samples, _) = decode("tone-stereo-44k.m4a");
    // count zero crossings in the middle half second: 440 Hz gives about 880 per second (440 in the half second)
    let middle = &samples[4_000..12_000];
    let crossings = middle.windows(2).filter(|w| (w[0] < 0.0) != (w[1] < 0.0)).count();
    assert!((420.0..=460.0).contains(&(crossings as f64)), "{crossings} crossings in 0.5 s");
}

#[test]
fn blocks_arrive_in_order_with_progress() {
    let mut last = 0.0;
    let mut total = None;
    decode_file(&fixture("tone-mono-44k.mp3"), &AtomicBool::new(false), |_, p| {
        assert!(p.seconds >= last);
        last = p.seconds;
        total = total.or(p.total_seconds);
    })
    .unwrap();
    assert!(last > 0.9);
}

#[test]
fn cancelling_stops_early() {
    let cancel = AtomicBool::new(false);
    let err = decode_file(&fixture("tone-mono-44k.mp3"), &cancel, |_, _| cancel.store(true, Ordering::Relaxed)).unwrap_err();
    assert_eq!(err, DecodeError::Cancelled);
}

#[test]
fn files_that_are_not_audio_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.mp3");
    std::fs::write(&path, b"this is not audio at all").unwrap();
    assert!(matches!(decode_file(&path, &AtomicBool::new(false), |_, _| {}), Err(DecodeError::Unsupported(_))));
    assert!(matches!(decode_file(&dir.path().join("missing.wav"), &AtomicBool::new(false), |_, _| {}), Err(DecodeError::Unsupported(_))));
}
