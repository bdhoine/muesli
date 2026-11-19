//! Audio as Whisper wants it (16 kHz mono f32), without the OS: resampling, mixing and decoding
//! audio files. Capturing devices is a port; the `cpal` adapters live in `platform-common`.

pub mod decode;
pub mod resample;

pub use decode::{decode_file, DecodeError, Progress};
pub use resample::{downmix_to_mono, Resampler};

/// The format Whisper wants.
pub const TARGET_RATE: u32 = 16_000;
