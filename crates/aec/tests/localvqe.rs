//! LocalVQE through Muesli's echo stage.
//!
//! The ignored tests need the `localvqe` feature, the model (`python3 scripts/fetch-aec-model.py`)
//! and two clips of speech, 16 kHz mono 16-bit WAV, of different voices:
//!
//!     MUESLI_AEC_NEAR_WAV=near.wav MUESLI_AEC_FAR_WAV=far.wav \
//!         cargo test -p muesli-aec --features localvqe -- --ignored
//!
//! On a Mac, `say -o near.wav --data-format=LEI16@16000 "…"` makes one. The far end is played
//! through a synthetic room (a delay and four reflections) to make the microphone's echo.

use std::path::{Path, PathBuf};

use muesli_aec::{built_library_dir, LocalVqe, LIBRARY, MODEL};
use muesli_core::aec::EchoStage;

const R: usize = 16_000;

#[test]
fn a_missing_library_or_model_is_an_error_not_a_crash() {
    let dir = tempfile_dir();
    assert!(LocalVqe::load_from(&dir).is_err());
    assert!(LocalVqe::load(&dir.join(LIBRARY), Path::new("/nowhere/model.gguf")).is_err());
}

fn tempfile_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("muesli-aec-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn model() -> PathBuf {
    let dir =
        std::env::var("MUESLI_AEC_MODEL_DIR").map(PathBuf::from).unwrap_or_else(|_| Path::new(env!("CARGO_MANIFEST_DIR")).join("model"));
    dir.join(MODEL)
}

fn canceller() -> LocalVqe {
    let dir = built_library_dir().expect("build with --features localvqe");
    LocalVqe::load(&dir.join(LIBRARY), &model()).expect("LocalVQE loads (python3 scripts/fetch-aec-model.py)")
}

fn wav(var: &str) -> Vec<f32> {
    let path = std::env::var(var).unwrap_or_else(|_| panic!("set {var} to a 16 kHz mono WAV"));
    let mut reader = hound::WavReader::open(&path).unwrap();
    assert_eq!(reader.spec().sample_rate as usize, R, "{path}: 16 kHz");
    assert_eq!(reader.spec().channels, 1, "{path}: mono");
    reader.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect()
}

/// The far end as the microphone hears it from the speakers.
fn room(far: &[f32], delay_ms: usize, gain: f32) -> Vec<f32> {
    let d = delay_ms * R / 1000;
    let taps = [(0usize, 1.0f32), (37, 0.5), (91, -0.3), (160, 0.2), (400, 0.1)];
    let mut out = vec![0.0; far.len()];
    for (i, o) in out.iter_mut().enumerate() {
        for &(k, g) in &taps {
            if i >= d + k {
                *o += far[i - d - k] * g * gain;
            }
        }
    }
    out
}

fn energy(x: &[f32]) -> f64 {
    x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() + 1e-12
}

fn db(a: f64, b: f64) -> f64 {
    10.0 * (a / b).log10()
}

/// Both streams in 10 ms callbacks, as capture delivers them.
fn run(stage: &mut EchoStage, mic: &[f32], reference: &[f32]) -> Vec<f32> {
    let mut out = Vec::new();
    for (m, r) in mic.chunks(160).zip(reference.chunks(160)) {
        stage.push_reference(r);
        stage.push_mic(m);
        out.extend(stage.take());
    }
    out.extend(stage.finish());
    out
}

/// The far end repeated to `seconds`, so the delay estimate has time to settle.
fn long(clip: &[f32], seconds: usize) -> Vec<f32> {
    clip.iter().copied().cycle().take(seconds * R).collect()
}

#[test]
#[ignore = "needs the localvqe feature, the model and two speech clips"]
fn the_echo_of_the_call_is_removed_at_any_delay() {
    let far = long(&wav("MUESLI_AEC_FAR_WAV"), 20);
    for delay_ms in [40, 150, 280, 500] {
        let echo = room(&far, delay_ms, 0.3);
        let mut stage = EchoStage::new(Some(Box::new(canceller())));
        let out = run(&mut stage, &echo, &far);
        assert!(stage.active(), "{delay_ms} ms: gave up");
        let settled = 8 * R..out.len();
        let erle = db(energy(&echo[settled.clone()]), energy(&out[settled]));
        eprintln!("{delay_ms} ms: echo reduced by {erle:.1} dB, delay found {:.3} s", stage.delay_seconds());
        assert!(erle >= 20.0, "{delay_ms} ms: only {erle:.1} dB");
    }
}

#[test]
#[ignore = "needs the localvqe feature, the model and two speech clips"]
fn the_near_speaker_is_kept_while_both_talk() {
    let far = long(&wav("MUESLI_AEC_FAR_WAV"), 20);
    let near_clip = wav("MUESLI_AEC_NEAR_WAV");
    // The far end alone for 10 s, then both.
    let mut near = vec![0.0; 10 * R];
    near.extend(near_clip.iter().copied().cycle().take(10 * R));
    let echo = room(&far, 150, 0.3);
    let mic: Vec<f32> = near.iter().zip(&echo).map(|(n, e)| n + e).collect();
    let mut stage = EchoStage::new(Some(Box::new(canceller())));
    let out = run(&mut stage, &mic, &far);
    let both = 10 * R..20 * R;
    let left: Vec<f32> = out[both.clone()].iter().zip(&near[both.clone()]).map(|(o, n)| o - n).collect();
    let before = db(energy(&near[both.clone()]), energy(&echo[both.clone()]));
    let after = db(energy(&near[both.clone()]), energy(&left));
    let kept = db(energy(&out[both.clone()]), energy(&near[both]));
    eprintln!("double talk: near to echo {before:.1} dB in, {after:.1} dB out; level {kept:+.1} dB");
    assert!(after > before + 3.0, "no better: {before:.1} -> {after:.1} dB");
    assert!(kept.abs() < 3.0, "the near speaker changed level by {kept:+.1} dB");
}

#[test]
#[ignore = "needs the localvqe feature and the model"]
fn with_headphones_the_microphone_is_left_alone() {
    let far = long(&wav("MUESLI_AEC_FAR_WAV"), 10);
    let near: Vec<f32> = wav("MUESLI_AEC_NEAR_WAV").into_iter().cycle().take(10 * R).collect();
    let mut stage = EchoStage::new(Some(Box::new(canceller())));
    let out = run(&mut stage, &near, &far);
    let level = db(energy(&out), energy(&near));
    eprintln!("headphones: level {level:+.2} dB");
    // Measured on an M1: -0.96 dB (the far end plays, so LocalVQE runs; it barely touches the voice).
    assert!(level.abs() < 1.5, "level changed by {level:+.2} dB");
}
