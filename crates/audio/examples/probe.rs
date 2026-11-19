//! Manual check of capture on a real machine (macOS or Windows):
//!   cargo run -p muesli-audio --example probe -- 8
//! Speak, and play some audio, for the given number of seconds. It prints how much sound each
//! stream ("You" = microphone, "Others" = system audio) picked up.

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn main() {
    use std::sync::{Arc, Mutex};

    let seconds: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(8);
    let chunks = Arc::new(Mutex::new(Vec::new()));
    let sink = chunks.clone();
    let mut capture = muesli_audio::AudioCapture::new(move |c| sink.lock().unwrap().push(c), |e| eprintln!("stream error: {e}"));
    if let Err(e) = capture.start() {
        eprintln!("{e}");
        std::process::exit(1);
    }
    println!("Recording for {seconds} s: talk, and play some audio...");
    std::thread::sleep(std::time::Duration::from_secs(seconds));
    capture.stop();
    for c in chunks.lock().unwrap().iter() {
        let rms = (c.samples.iter().map(|v| v * v).sum::<f32>() / c.samples.len().max(1) as f32).sqrt();
        let peak = c.samples.iter().fold(0f32, |m, v| m.max(v.abs()));
        println!("{:7}: {:.1} s, rms {:.4}, peak {:.3}", c.speaker, c.samples.len() as f32 / 16_000.0, rms, peak);
    }
    println!("A stream with rms 0.0000 heard nothing (permission denied, or nothing was playing).");
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn main() {
    eprintln!("Capture is only available on macOS and Windows.");
}
