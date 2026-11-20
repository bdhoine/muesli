//! LocalVQE next to whisper.cpp in one process (two ggml copies), speed per hop, echo reduction,
//! and what Whisper hears from the raw and the cleaned microphone.
//! Env: LVQE_LIB (dylib), LVQE_MODEL (gguf), WHISPER_MODEL (ggml .bin), NEAR_WAV, FAR_WAV (16 kHz mono).
use libloading::{Library, Symbol};
use std::ffi::{c_char, c_int, c_void, CString};
use std::time::Instant;
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

const R: usize = 16_000;
const HOP: usize = 256;
type Ctx = *mut c_void;

struct Lvqe {
    _lib: Library,
    ctx: Ctx,
    frame: unsafe extern "C" fn(Ctx, *const f32, *const f32, c_int, *mut f32) -> c_int,
    reset: unsafe extern "C" fn(Ctx),
}

impl Lvqe {
    fn load(lib: &str, model: &str) -> Self {
        unsafe {
            let lib = Library::new(lib).expect("load dylib");
            let opts_new: Symbol<unsafe extern "C" fn() -> *mut c_void> = lib.get(b"localvqe_options_new").unwrap();
            let set_model: Symbol<unsafe extern "C" fn(*mut c_void, *const c_char) -> c_int> = lib.get(b"localvqe_options_set_model_path").unwrap();
            let set_threads: Symbol<unsafe extern "C" fn(*mut c_void, c_int) -> c_int> = lib.get(b"localvqe_options_set_threads").unwrap();
            let new_with: Symbol<unsafe extern "C" fn(*mut c_void) -> Ctx> = lib.get(b"localvqe_new_with_options").unwrap();
            let opts_free: Symbol<unsafe extern "C" fn(*mut c_void)> = lib.get(b"localvqe_options_free").unwrap();
            let o = opts_new();
            let m = CString::new(model).unwrap();
            set_model(o, m.as_ptr());
            set_threads(o, 1);
            let ctx = new_with(o);
            opts_free(o);
            assert!(!ctx.is_null(), "localvqe_new failed");
            let frame = *lib.get(b"localvqe_process_frame_f32").unwrap();
            let reset = *lib.get(b"localvqe_reset").unwrap();
            Self { _lib: lib, ctx, frame, reset }
        }
    }
    fn run(&self, mic: &[f32], refr: &[f32]) -> (Vec<f32>, Vec<f64>) {
        unsafe { (self.reset)(self.ctx) };
        let mut out = vec![0.0; mic.len()];
        let mut times = Vec::new();
        for i in 0..mic.len() / HOP {
            let s = i * HOP..(i + 1) * HOP;
            let t = Instant::now();
            let rc = unsafe { (self.frame)(self.ctx, mic[s.clone()].as_ptr(), refr[s.clone()].as_ptr(), HOP as c_int, out[s].as_mut_ptr()) };
            times.push(t.elapsed().as_secs_f64() * 1000.0);
            assert_eq!(rc, 0);
        }
        (out, times)
    }
}

fn wav(path: &str) -> Vec<f32> {
    let mut r = hound::WavReader::open(path).unwrap();
    assert_eq!(r.spec().sample_rate as usize, R);
    r.samples::<i16>().map(|s| s.unwrap() as f32 / 32768.0).collect()
}
fn echo(far: &[f32], delay_ms: usize, gain: f32) -> Vec<f32> {
    let d = delay_ms * R / 1000;
    let mut out = vec![0.0; far.len()];
    let taps = [(0usize, 1.0f32), (37, 0.5), (91, -0.3), (160, 0.2), (400, 0.1)];
    for i in 0..far.len() {
        for &(k, g) in &taps {
            if i >= d + k {
                out[i] += far[i - d - k] * g * gain;
            }
        }
    }
    out
}
/// Delay (ms) of `mic` behind `far`: peak of the cross-correlation of 10 ms energy envelopes.
fn estimate_delay(mic: &[f32], far: &[f32], max_ms: usize) -> usize {
    let env = |x: &[f32]| x.chunks(160).map(|c| c.iter().map(|v| v * v).sum::<f32>().sqrt()).collect::<Vec<f32>>();
    let (m, f) = (env(mic), env(far));
    let mean = |x: &[f32]| x.iter().sum::<f32>() / x.len() as f32;
    let (mm, fm) = (mean(&m), mean(&f));
    (0..=max_ms / 10)
        .max_by(|&a, &b| {
            let c = |lag: usize| (lag..m.len()).map(|i| (m[i] - mm) * (f[i - lag] - fm)).sum::<f32>();
            c(a).partial_cmp(&c(b)).unwrap()
        })
        .unwrap()
        * 10
}
fn energy(x: &[f32]) -> f64 { x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() + 1e-12 }
fn db(a: f64, b: f64) -> f64 { 10.0 * (a / b).log10() }
fn add(a: &[f32], b: &[f32]) -> Vec<f32> { a.iter().zip(b).map(|(x, y)| x + y).collect() }
fn pct(v: &mut [f64], p: f64) -> f64 { v.sort_by(|a, b| a.partial_cmp(b).unwrap()); v[((v.len() - 1) as f64 * p) as usize] }

fn transcribe(ctx: &WhisperContext, x: &[f32]) -> String {
    let mut st = ctx.create_state().unwrap();
    let mut p = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    p.set_language(Some("en"));
    p.set_no_context(true);
    p.set_print_progress(false);
    p.set_print_realtime(false);
    p.set_print_special(false);
    p.set_print_timestamps(false);
    st.full(p, x).unwrap();
    st.as_iter().map(|s| s.to_str_lossy().unwrap().to_string()).collect::<Vec<_>>().join("").trim().to_string()
}

fn main() {
    let env = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("{k}"));
    // whisper first, so its ggml (with Metal) is initialised before LocalVQE's copy is loaded
    let wctx = WhisperContext::new_with_params(&env("WHISPER_MODEL"), WhisperContextParameters::default()).unwrap();
    let lvqe = Lvqe::load(&env("LVQE_LIB"), &env("LVQE_MODEL"));
    let near = wav(&env("NEAR_WAV"));
    let far = wav(&env("FAR_WAV"));
    let n = near.len().min(far.len()) / HOP * HOP;
    let (near, far) = (&near[..n], &far[..n]);
    let secs = n as f64 / R as f64;
    println!("clip {secs:.1} s");

    // speed, single talk
    let e = echo(far, 150, 0.3);
    let (out, mut t) = lvqe.run(&e, far);
    let total: f64 = t.iter().sum();
    println!("speed: p50 {:.2} ms, p99 {:.2} ms per 16 ms hop; {:.1}% of one core", pct(&mut t, 0.5), pct(&mut t, 0.99), total / (secs * 1000.0) * 100.0);
    for &(delay, gain) in &[(40usize, 0.5f32), (150, 0.3), (280, 0.3), (500, 0.3)] {
        let e = echo(far, delay, gain);
        let (out, _) = lvqe.run(&e, far);
        let skip = R; // let it converge for a second
        println!("single talk, delay {delay} ms: ERLE {:.1} dB", db(energy(&e[skip..]), energy(&out[skip..])));
        // reference shifted by our own estimate, leaving 20 ms for LocalVQE to find
        let shift = delay.saturating_sub(20) * R / 1000;
        let shifted: Vec<f32> = (0..n).map(|i| if i >= shift { far[i - shift] } else { 0.0 }).collect();
        let (out, _) = lvqe.run(&e, &shifted);
        println!("  with the reference shifted by {} ms: ERLE {:.1} dB", delay.saturating_sub(20), db(energy(&e[skip..]), energy(&out[skip..])));
    }
    let lag = estimate_delay(&echo(far, 280, 0.3), far, 600);
    println!("cross-correlation estimate for a 280 ms echo: {lag} ms");
    // near alone, far silent; and headphones (far plays, mic hears none of it)
    let silent = vec![0.0; n];
    let (o1, _) = lvqe.run(near, &silent);
    let (o2, _) = lvqe.run(near, far);
    let lat = (0..2048usize)
        .max_by(|&a, &b| {
            let c = |l: usize| (l..n).map(|i| o1[i] as f64 * near[i - l] as f64).sum::<f64>();
            c(a).partial_cmp(&c(b)).unwrap()
        })
        .unwrap();
    println!("output latency of the per-hop API: {lat} samples");
    println!("near alone: level {:+.1} dB (far silent), {:+.1} dB (headphones)", db(energy(&o1), energy(near)), db(energy(&o2), energy(near)));
    // double talk
    let e = echo(far, 150, 0.3);
    let mic = add(near, &e);
    let (clean, _) = lvqe.run(&mic, far);
    let resid: Vec<f32> = clean[HOP..].iter().zip(near).map(|(c, x)| c - x).collect(); // one hop late
    println!("double talk: near/echo in {:.1} dB, near/(out-near) {:.1} dB", db(energy(near), energy(&e)), db(energy(&near[..n - HOP]), energy(&resid)));
    drop(out);
    // what Whisper hears (both libraries live in the process now)
    println!("\nWHISPER near (truth): {}", transcribe(&wctx, near));
    println!("WHISPER far (truth):  {}", transcribe(&wctx, far));
    println!("WHISPER echo only, raw:     {}", transcribe(&wctx, &e));
    let (eclean, _) = lvqe.run(&e, far);
    println!("WHISPER echo only, cleaned: {}", transcribe(&wctx, &eclean));
    println!("WHISPER double talk, raw:     {}", transcribe(&wctx, &mic));
    println!("WHISPER double talk, cleaned: {}", transcribe(&wctx, &clean));
}
