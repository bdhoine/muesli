//! Conversion to what Whisper wants: one channel at 16 kHz. The resampling itself is `rubato`'s
//! FFT resampler (fixed ratio, anti-aliased); this file only adapts it to a stream of arbitrary
//! sized blocks.

use audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler as _};

/// Input frames handed to rubato per call.
const CHUNK: usize = 1024;

/// Averages interleaved frames into one channel.
pub fn downmix_to_mono(interleaved: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    interleaved.chunks_exact(channels).map(|f| f.iter().sum::<f32>() / channels as f32).collect()
}

/// Streaming mono resampler. Feed blocks of any size; output comes out as whole chunks are ready.
/// The output lags the input by a few milliseconds (the filter's delay), which is irrelevant for
/// 30-second transcription chunks.
pub struct Resampler {
    /// `None` when the rates already match.
    inner: Option<Fft<f32>>,
    pending: Vec<f32>,
}

impl Resampler {
    pub fn new(input_rate: u32, output_rate: u32) -> Self {
        assert!(input_rate > 0 && output_rate > 0);
        let inner = (input_rate != output_rate).then(|| {
            Fft::new(input_rate as usize, output_rate as usize, CHUNK, 1, FixedSync::Input)
                .expect("positive sample rates and one channel are always valid")
        });
        Self { inner, pending: Vec::new() }
    }

    /// Feeds mono input; returns whatever output samples are ready.
    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        let Some(resampler) = self.inner.as_mut() else { return input.to_vec() };
        self.pending.extend_from_slice(input);
        let mut out = Vec::new();
        let mut used = 0;
        loop {
            let need = resampler.input_frames_next();
            if self.pending.len() - used < need {
                break;
            }
            let block = &self.pending[used..used + need];
            used += need;
            let Ok(adapter) = InterleavedSlice::new(block, 1, need) else { break };
            match resampler.process(&adapter, None) {
                Ok(chunk) => out.extend(chunk.take_data()),
                Err(_) => break,
            }
        }
        self.pending.drain(..used);
        out
    }
}
