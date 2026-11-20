use sonora::config::EchoCanceller;
use sonora::{AudioProcessing, Config, StreamConfig};
use std::time::Instant;

const R: usize = 16_000;
struct Rng(u64);
impl Rng { fn f(&mut self) -> f32 { self.0 ^= self.0 << 13; self.0 ^= self.0 >> 7; self.0 ^= self.0 << 17; (self.0 >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0 } }

/// Speech-like: harmonics of a wandering pitch plus breath noise, switched by a syllable envelope with pauses.
fn talk(seed: u64, secs: usize, pitch: f32, active: impl Fn(f32) -> bool) -> Vec<f32> {
    let mut r = Rng(seed); let mut ph = 0.0f32; let mut lp = 0.0f32;
    (0..secs * R).map(|i| {
        let t = i as f32 / R as f32;
        let f0 = pitch * (1.0 + 0.15 * (t * 1.3 + seed as f32).sin());
        ph += f0 / R as f32;
        let mut v = 0.0; for h in 1..12 { v += (std::f32::consts::TAU * ph * h as f32).sin() / h as f32 * (1.0 + (t * 3.0 + h as f32).sin()) * 0.5; }
        lp = lp * 0.9 + r.f() * 0.1;
        let env = ((t * 4.0 * std::f32::consts::PI + seed as f32).sin()).max(0.0).powf(0.5);
        if active(t) { (v * 0.15 + lp * 0.3) * env } else { 0.0 }
    }).collect()
}

fn echo(far: &[f32], delay_ms: usize, gain: f32) -> Vec<f32> {
    let d = delay_ms * R / 1000; let mut out = vec![0.0; far.len()];
    let taps = [(0usize, 1.0f32), (37, 0.5), (91, -0.3), (160, 0.2), (400, 0.1)];
    for i in 0..far.len() { for &(k, g) in &taps { if i >= d + k { out[i] += far[i - d - k] * g * gain; } } }
    out
}
fn energy(x: &[f32]) -> f64 { x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() + 1e-12 }
fn db(a: f64, b: f64) -> f64 { 10.0 * (a / b).log10() }

fn run(far: &[f32], mic: &[f32], hmm: bool) -> (Vec<f32>, f64, AudioProcessing) {
    let mut ec = EchoCanceller::default();
    if hmm { ec.transparent_mode = sonora::config::TransparentModeType::Hmm; }
    let cfg = Config { echo_canceller: Some(ec), ..Default::default() };
    let mut apm = AudioProcessing::builder().config(cfg).capture_config(StreamConfig::new(16000, 1)).render_config(StreamConfig::new(16000, 1)).build();
    let n = 160; let mut out = vec![0.0; mic.len()]; let mut rd = vec![0.0; n];
    let t0 = Instant::now();
    for (i, (f, m)) in far.chunks_exact(n).zip(mic.chunks_exact(n)).enumerate() {
        apm.process_render_f32(&[f], &mut [&mut rd]).unwrap();
        let o = &mut out[i * n..(i + 1) * n];
        apm.process_capture_f32(&[m], &mut [o]).unwrap();
    }
    let secs = t0.elapsed().as_secs_f64();
    (out, secs, apm)
}

fn main() {
    let secs = 40;
    let mut nz = Rng(99); let noise: Vec<f32> = (0..secs * R).map(|_| nz.f() * 0.001).collect();
    let far = talk(1, secs, 180.0, |_| true);
    let near_dt = talk(7, secs, 120.0, |t| t > 20.0);     // near speaks only in the second half
    let tail = 30 * R..secs * R;
    for &(delay, gain) in &[(40usize, 0.5f32), (150, 0.3), (280, 0.3)] {
        for hmm in [false, true] {
            let e = echo(&far, delay, gain);
            // single talk: echo + noise only, first 20 s; measure 10..20 s
            let mic: Vec<f32> = e.iter().zip(&noise).map(|(a, b)| a + b).collect();
            let (out, t, apm) = run(&far, &mic, hmm);
            let st = 10 * R..20 * R;
            let erle = db(energy(&mic[st.clone()]), energy(&out[st.clone()]));
            // double talk: echo + near in 20..40 s
            let mic2: Vec<f32> = mic.iter().zip(&near_dt).map(|(a, b)| a + b).collect();
            let (out2, _, _) = run(&far, &mic2, hmm);
            let ser_in = db(energy(&near_dt[tail.clone()]), energy(&e[tail.clone()]));
            let silent = vec![0.0f32; secs * R];
            let near_mic: Vec<f32> = near_dt.iter().zip(&noise).map(|(a, b)| a + b).collect();
            let (near_ref, _, _) = run(&silent, &near_mic, hmm);
            let resid: Vec<f32> = out2[tail.clone()].iter().zip(&near_ref[tail.clone()]).map(|(o, n)| o - n).collect();
            let ser_out = db(energy(&near_ref[tail.clone()]), energy(&resid));
            let near_kept = db(energy(&out2[tail.clone()]), energy(&mic2[tail.clone()].iter().zip(&e[tail.clone()]).map(|(m, e)| m - e).collect::<Vec<_>>()));
            let s = apm.statistics();
            println!("delay {delay:>3} ms gain {gain}: hmm={hmm} single-talk ERLE {erle:5.1} dB | double-talk near/echo in {ser_in:5.1} dB -> near/(out-near) {ser_out:5.1} dB, output level vs clean near {near_kept:+5.1} dB | est delay {:?} ms | {:.2} ms CPU per s audio",
                s.delay_ms, t / secs as f64 * 1000.0);
        }
    }
    // headphones: far plays, mic hears only near speech (from 20 s) + noise. Output must equal input.
    for hmm in [false, true] {
        let mic: Vec<f32> = near_dt.iter().zip(&noise).map(|(a, b)| a + b).collect();
        let (out, _, _) = run(&far, &mic, hmm);
        println!("headphones hmm={hmm}: near level change {:+.2} dB (20..30 s) {:+.2} dB (30..40 s)", db(energy(&out[20*R..30*R]), energy(&mic[20*R..30*R])), db(energy(&out[30*R..]), energy(&mic[30*R..])));
    }
    // silence on far end: near alone must pass
    let silent = vec![0.0f32; secs * R];
    let mic: Vec<f32> = near_dt.iter().zip(&noise).map(|(a, b)| a + b).collect();
    let (out, _, _) = run(&silent, &mic, false);
    println!("far silent: near level change {:+.2} dB", db(energy(&out[20*R..]), energy(&mic[20*R..])));
}
