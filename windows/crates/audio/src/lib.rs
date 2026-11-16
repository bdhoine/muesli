//! Microphone ("You") and system audio ("Others") as 16 kHz mono f32, chunked for Whisper.
//!
//! Resampling and mixing are plain Rust and tested everywhere. The device code uses `cpal`:
//! the microphone on macOS and Windows, and system audio through WASAPI loopback on Windows and
//! a Core Audio process tap on macOS (the "System Audio Recording Only" permission).

pub mod resample;

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod capture;
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub use capture::{probe_microphone, probe_system_audio, AudioCapture, CaptureError};

pub use resample::{downmix_to_mono, Resampler};

/// The format Whisper wants.
pub const TARGET_RATE: u32 = 16_000;
