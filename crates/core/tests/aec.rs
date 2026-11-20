//! The echo stage with the stand-in canceller: alignment, latency, passthrough and giving up.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use muesli_core::aec::{EchoCanceller, EchoStage, StandInCanceller, BUDGET_HOPS, HOP};

const R: usize = 16_000;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0
    }
}

/// Speech-like: harmonics on a wandering pitch, noise, and syllables with pauses between them.
fn talk(seed: u64, seconds: f64, pitch: f32) -> Vec<f32> {
    let mut rng = Rng(seed);
    let (mut phase, mut lp) = (0.0f32, 0.0f32);
    (0..(seconds * R as f64) as usize)
        .map(|i| {
            let t = i as f32 / R as f32;
            phase += pitch * (1.0 + 0.15 * (t * 1.3 + seed as f32).sin()) / R as f32;
            let voiced: f32 = (1..8).map(|h| (std::f32::consts::TAU * phase * h as f32).sin() / h as f32).sum();
            lp = lp * 0.9 + rng.next() * 0.1;
            // Syllables of uneven length, with each voice's own rhythm, so the envelope has one
            // clear best lag and two voices do not look like echoes of each other.
            let (fast, slow) = (2.3 + 0.37 * seed as f32, 0.5 + 0.13 * seed as f32);
            let env = ((t * fast + seed as f32).sin() * (t * slow).cos()).max(0.0);
            (voiced * 0.1 + lp * 0.2) * env
        })
        .collect()
}

fn delayed(x: &[f32], ms: usize, gain: f32) -> Vec<f32> {
    let d = ms * R / 1000;
    (0..x.len()).map(|i| if i >= d { x[i - d] * gain } else { 0.0 }).collect()
}

fn add(a: &[f32], b: &[f32]) -> Vec<f32> {
    a.iter().zip(b).map(|(x, y)| x + y).collect()
}

fn energy(x: &[f32]) -> f64 {
    x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() + 1e-12
}

fn db(a: f64, b: f64) -> f64 {
    10.0 * (a / b).log10()
}

/// Feeds both streams in interleaved callbacks at the same rate but out of step, as capture
/// does, and returns the cleaned microphone.
fn run(stage: &mut EchoStage, mic: &[f32], reference: &[f32]) -> Vec<f32> {
    let mut out = Vec::new();
    let (mut m, mut r) = (0, 0);
    while m < mic.len() || r < reference.len() {
        let rn = (if r == 0 { 200 } else { 441 }).min(reference.len() - r);
        stage.push_reference(&reference[r..r + rn]);
        r += rn;
        let mn = 441.min(mic.len() - m);
        stage.push_mic(&mic[m..m + mn]);
        m += mn;
        out.extend(stage.take());
    }
    out.extend(stage.finish());
    out
}

/// A stage with the stand-in and a clock that stands still, so a slow test machine never makes
/// the stage give up (that has its own test).
fn stage(latency: usize) -> EchoStage {
    EchoStage::with_clock(Some(Box::new(StandInCanceller::new(latency))), Box::new(|| 0.0))
}

#[test]
fn as_many_samples_come_out_as_went_in() {
    let far = talk(1, 3.3, 180.0);
    let mic = add(&delayed(&far, 100, 0.4), &talk(2, 3.3, 120.0));
    for latency in [0, 100, HOP] {
        for len in [0, 1, 255, 256, 257, 16_001, mic.len()] {
            let out = run(&mut stage(latency), &mic[..len], &far[..len]);
            assert_eq!(out.len(), len, "latency {latency}");
        }
    }
    assert_eq!(run(&mut EchoStage::new(None), &mic, &far).len(), mic.len());
}

#[test]
fn without_a_canceller_the_microphone_passes_untouched() {
    let far = talk(1, 3.0, 180.0);
    let mic = talk(2, 3.0, 120.0);
    assert_eq!(run(&mut EchoStage::new(None), &mic, &far), mic);
}

/// Counts the hops it is asked to clean.
struct Counting(StandInCanceller, Arc<AtomicUsize>);
impl EchoCanceller for Counting {
    fn latency(&self) -> usize {
        self.0.latency()
    }
    fn process(&mut self, mic: &[f32], reference: &[f32], out: &mut [f32]) -> Result<(), String> {
        self.1.fetch_add(1, Ordering::SeqCst);
        self.0.process(mic, reference, out)
    }
    fn reset(&mut self) {
        self.0.reset();
    }
}

#[test]
fn while_the_far_end_is_silent_the_microphone_passes_untouched_and_unprocessed() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut stage = EchoStage::new(Some(Box::new(Counting(StandInCanceller::new(HOP), calls.clone()))));
    let mic = talk(2, 4.0, 120.0);
    let out = run(&mut stage, &mic, &vec![0.0; mic.len()]);
    assert_eq!(out, mic);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn a_delayed_echo_is_found_and_removed() {
    for (delay_ms, latency) in [(40, HOP), (150, HOP), (280, 0), (500, HOP)] {
        let far = talk(1, 12.0, 180.0);
        let echo = delayed(&far, delay_ms, 0.4);
        let mut stage = stage(latency);
        let out = run(&mut stage, &echo, &far);
        let settled = 6 * R..out.len();
        let erle = db(energy(&echo[settled.clone()]), energy(&out[settled]));
        assert!(erle > 30.0, "{delay_ms} ms: only {erle:.1} dB removed");
        assert!((stage.delay_seconds() - delay_ms as f64 / 1000.0).abs() <= 0.02, "{delay_ms} ms: found {}", stage.delay_seconds());
    }
}

#[test]
fn in_double_talk_the_near_speaker_stays() {
    // The far end alone for 6 s (the canceller converges), then both talk.
    let far = talk(1, 12.0, 180.0);
    let near = [vec![0.0; 6 * R], talk(7, 6.0, 120.0)].concat();
    let echo = delayed(&far, 150, 0.3);
    let out = run(&mut stage(HOP), &add(&near, &echo), &far);
    let settled = 6 * R..out.len();
    let left: Vec<f32> = out[settled.clone()].iter().zip(&near[settled.clone()]).map(|(o, n)| o - n).collect();
    let before = db(energy(&near[settled.clone()]), energy(&echo[settled.clone()]));
    let after = db(energy(&near[settled]), energy(&left));
    assert!(after > before + 20.0, "near to echo: {before:.1} dB in, {after:.1} dB out");
}

#[test]
fn a_reference_that_stalls_lines_up_again_after_the_gap() {
    // Windows loopback delivers nothing while nothing plays: 3 s of reference never arrive.
    let far = [talk(1, 8.0, 180.0), vec![0.0; 3 * R], talk(3, 8.0, 200.0)].concat();
    let echo = delayed(&far, 120, 0.4);
    let delivered: Vec<f32> = [&far[..8 * R], &far[11 * R..]].concat();
    let mut stage = stage(HOP);
    let mut out = Vec::new();
    let (mut m, mut r) = (0, 0);
    while m < echo.len() {
        // The reference keeps pace with the microphone except during the gap.
        let in_gap = (8 * R..11 * R).contains(&m);
        if !in_gap && r < delivered.len() {
            let n = 480.min(delivered.len() - r);
            stage.push_reference(&delivered[r..r + n]);
            r += n;
        }
        let n = 480.min(echo.len() - m);
        stage.push_mic(&echo[m..m + n]);
        m += n;
        out.extend(stage.take());
    }
    out.extend(stage.finish());
    assert_eq!(out.len(), echo.len());
    let after_gap = 15 * R..out.len();
    let erle = db(energy(&echo[after_gap.clone()]), energy(&out[after_gap]));
    assert!(erle > 25.0, "after the gap only {erle:.1} dB removed");
}

/// Fails on its tenth hop.
struct Failing(usize);
impl EchoCanceller for Failing {
    fn process(&mut self, mic: &[f32], _: &[f32], out: &mut [f32]) -> Result<(), String> {
        self.0 += 1;
        if self.0 >= 10 {
            return Err("broken".into());
        }
        out.copy_from_slice(mic);
        Ok(())
    }
    fn reset(&mut self) {}
}

#[test]
fn a_failing_canceller_is_dropped_for_the_rest_of_the_meeting() {
    let far = talk(1, 3.0, 180.0);
    let mic = talk(2, 3.0, 120.0);
    let mut stage = EchoStage::new(Some(Box::new(Failing(0))));
    let out = run(&mut stage, &mic, &far);
    assert!(!stage.active());
    assert_eq!(out, mic, "the raw microphone, sample for sample");
}

#[test]
fn a_canceller_that_cannot_keep_up_is_dropped() {
    // Every call takes 12 ms of a 16 ms hop (the clock is read twice per call): over the budget.
    let ticks = Arc::new(AtomicUsize::new(0));
    let clock_ticks = ticks.clone();
    let clock = Box::new(move || clock_ticks.fetch_add(1, Ordering::SeqCst) as f64 * 0.012);
    let mut stage = EchoStage::with_clock(Some(Box::new(StandInCanceller::new(HOP))), clock);
    let far = talk(1, 3.0, 180.0);
    let mic = delayed(&far, 40, 0.4);
    let out = run(&mut stage, &mic, &far);
    assert!(!stage.active());
    assert_eq!(out.len(), mic.len());
    // Given up after about a second: from then on the microphone is raw.
    let later = (BUDGET_HOPS + 4) * HOP..mic.len();
    assert_eq!(out[later.clone()], mic[later]);
}

#[test]
fn passthrough_and_cancelling_join_without_a_seam() {
    // Far end plays, falls silent long enough to pass through, then plays again.
    let far = [talk(1, 6.0, 180.0), vec![0.0; 4 * R], talk(3, 6.0, 200.0)].concat();
    let near = [vec![0.0; 5 * R], talk(7, 11.0, 120.0)].concat();
    let echo = delayed(&far, 60, 0.4);
    let out = run(&mut stage(HOP), &add(&near, &echo), &far);
    assert_eq!(out.len(), near.len());
    // In the silent stretch (after the 2 s hangover) the output is the microphone itself.
    let quiet = 9 * R..10 * R;
    assert_eq!(out[quiet.clone()], add(&near, &echo)[quiet]);
    // Once the far end plays again, the near speaker is what is left, without a jump in time.
    let back = 12 * R..16 * R;
    let left: Vec<f32> = out[back.clone()].iter().zip(&near[back.clone()]).map(|(o, n)| o - n).collect();
    assert!(db(energy(&near[back]), energy(&left)) > 25.0);
}

#[test]
fn a_microphone_that_stops_does_not_let_the_reference_pile_up() {
    let mut stage = stage(HOP);
    stage.push_mic(&talk(2, 1.0, 120.0));
    for _ in 0..600 {
        stage.push_reference(&vec![0.1; R / 10]); // a minute of system audio, no microphone
    }
    let out = stage.finish();
    assert_eq!(out.len(), R);
}
