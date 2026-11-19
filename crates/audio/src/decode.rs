//! Reads an audio file (mp3, m4a/aac, flac, wav, ogg/vorbis, alac) and hands it over as 16 kHz mono f32
//! blocks, the format Whisper wants. Streams: an hour of audio is never held in memory as a whole.

use std::fs::File;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::errors::Error;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

use crate::{downmix_to_mono, Resampler, TARGET_RATE};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecodeError {
    /// The file could not be opened or is in a format that is not supported.
    Unsupported(String),
    /// The file has no audio track.
    NoAudio,
    /// Decoding failed part-way (corrupt file).
    Failed(String),
    Cancelled,
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(why) => write!(f, "This audio file cannot be read: {why}"),
            Self::NoAudio => write!(f, "The file contains no audio."),
            Self::Failed(why) => write!(f, "The audio file could not be decoded: {why}"),
            Self::Cancelled => write!(f, "Cancelled."),
        }
    }
}

/// What `decode_file` tells `on_block` besides the samples.
#[derive(Debug, Clone, Copy)]
pub struct Progress {
    /// Seconds of audio delivered so far (at 16 kHz).
    pub seconds: f64,
    /// Total length when the container says (most do), else `None`.
    pub total_seconds: Option<f64>,
}

/// Decodes `path`, calling `on_block(samples, progress)` with mono 16 kHz blocks in order.
/// Returns the number of seconds decoded. Stops with `Cancelled` when `cancel` is set.
pub fn decode_file(path: &Path, cancel: &AtomicBool, mut on_block: impl FnMut(&[f32], Progress)) -> Result<f64, DecodeError> {
    let file = File::open(path).map_err(|e| DecodeError::Unsupported(e.to_string()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut format = symphonia::default::get_probe()
        .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
        .map_err(|e| DecodeError::Unsupported(e.to_string()))?;

    let track = format.default_track(TrackType::Audio).ok_or(DecodeError::NoAudio)?;
    let track_id = track.id;
    let params = track.codec_params.as_ref().and_then(|p| p.audio()).ok_or(DecodeError::NoAudio)?;
    let total_seconds = match (track.num_frames, params.sample_rate) {
        (Some(frames), Some(rate)) if rate > 0 => Some(frames as f64 / rate as f64),
        _ => None,
    };
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(params, &AudioDecoderOptions::default())
        .map_err(|e| DecodeError::Unsupported(e.to_string()))?;

    let mut resampler: Option<(u32, Resampler)> = None;
    let mut interleaved: Vec<f32> = Vec::new();
    let mut delivered = 0usize;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(DecodeError::Cancelled);
        }
        let packet = match format.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            Err(Error::ResetRequired) => break, // chained streams: what was read is enough
            Err(e) => return Err(DecodeError::Failed(e.to_string())),
        };
        if packet.track_id != track_id {
            continue;
        }
        let buffer = match decoder.decode(&packet) {
            Ok(b) => b,
            Err(Error::IoError(_)) | Err(Error::DecodeError(_)) => continue, // skip a bad packet
            Err(e) => return Err(DecodeError::Failed(e.to_string())),
        };
        let spec = buffer.spec();
        let (rate, channels) = (spec.rate(), spec.channels().count().max(1));
        interleaved.resize(buffer.samples_interleaved(), 0.0);
        buffer.copy_to_slice_interleaved(&mut interleaved);
        let mono = downmix_to_mono(&interleaved, channels);
        if resampler.as_ref().map(|(r, _)| *r) != Some(rate) {
            resampler = Some((rate, Resampler::new(rate, TARGET_RATE)));
        }
        let out = resampler.as_mut().expect("set above").1.process(&mono);
        if !out.is_empty() {
            delivered += out.len();
            on_block(&out, Progress { seconds: delivered as f64 / TARGET_RATE as f64, total_seconds });
        }
    }
    Ok(delivered as f64 / TARGET_RATE as f64)
}
