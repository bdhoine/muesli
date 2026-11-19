//! Manual check of capture on a real machine (macOS or Windows):
//!   cargo run -p muesli-platform-common --features capture --example probe -- 8
//! Speak, and play some audio, for the given number of seconds. It prints how much sound each
//! stream ("You" = microphone, "Others" = system audio) picked up.

#[cfg(all(feature = "capture", any(target_os = "macos", target_os = "windows")))]
fn main() {
    use std::sync::{Arc, Mutex};

    use muesli_core::ports::{MicCapture, SystemAudioCapture};
    use muesli_platform_common::cpal_capture::{CpalMic, CpalSystemAudio};

    let seconds: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(8);
    let you = Arc::new(Mutex::new(Vec::<f32>::new()));
    let others = Arc::new(Mutex::new(Vec::<f32>::new()));
    let (y, o) = (you.clone(), others.clone());
    let on_error: muesli_core::ports::OnError = Arc::new(|e| eprintln!("stream error: {e}"));
    let mic = CpalMic.start(Box::new(move |s| y.lock().unwrap().extend_from_slice(s)), on_error.clone());
    let system = CpalSystemAudio.start(Box::new(move |s| o.lock().unwrap().extend_from_slice(s)), on_error);
    let (Ok(mut mic), Ok(mut system)) = (mic, system) else {
        eprintln!("A stream did not start (no device, or no permission).");
        std::process::exit(1);
    };
    println!("Recording for {seconds} s: talk, and play some audio...");
    std::thread::sleep(std::time::Duration::from_secs(seconds));
    mic.stop();
    system.stop();
    for (speaker, samples) in [("You", you), ("Others", others)] {
        let samples = samples.lock().unwrap();
        let rms = (samples.iter().map(|v| v * v).sum::<f32>() / samples.len().max(1) as f32).sqrt();
        let peak = samples.iter().fold(0f32, |m, v| m.max(v.abs()));
        println!("{speaker:7}: {:.1} s, rms {rms:.4}, peak {peak:.3}", samples.len() as f32 / 16_000.0);
    }
    println!("A stream with rms 0.0000 heard nothing (permission denied, or nothing was playing).");
}

#[cfg(not(all(feature = "capture", any(target_os = "macos", target_os = "windows"))))]
fn main() {
    eprintln!("Capture is only available on macOS and Windows, with --features capture.");
}
