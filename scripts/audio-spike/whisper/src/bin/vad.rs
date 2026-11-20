use std::time::Instant;
use whisper_rs::{WhisperVadContext, WhisperVadContextParams};
fn main() {
    let path = std::env::args().nth(1).unwrap();
    whisper_rs::install_logging_hooks();
    for threads in [1, 4] {
        let mut p = WhisperVadContextParams::new();
        p.set_n_threads(threads);
        p.set_use_gpu(false);
        let mut ctx = WhisperVadContext::new(&path, p).expect("load");
        let mut x = 1u32;
        let audio: Vec<f32> = (0..16000 * 60).map(|_| { x ^= x << 13; x ^= x >> 17; x ^= x << 5; (x as f32 / u32::MAX as f32 - 0.5) * 0.2 }).collect();
        for secs in [0.512f32, 2.048, 4.096, 30.0] {
            let n = (secs * 16000.0) as usize;
            let reps = (60.0 / secs) as usize;
            let t = Instant::now();
            for r in 0..reps { let s = &audio[(r * n) % (audio.len() - n)..][..n]; ctx.detect_speech(s).unwrap(); }
            let per = t.elapsed().as_secs_f64() / reps as f64 * 1000.0;
            println!("threads {threads}: window {secs:6.3} s -> {per:7.3} ms per call, {:.4} ms per 32 ms frame, probs {}", per / (n as f64 / 512.0), ctx.probabilities().len());
        }
    }
}
