//! Runs a 16-bit PCM WAV through the same downmix and resampler the app uses and writes a
//! 16 kHz mono WAV, to check by ear or with Whisper that the conversion is sound:
//!   cargo run -p muesli-audio --example resample_wav -- in.wav out.wav

use muesli_audio::{downmix_to_mono, Resampler, TARGET_RATE};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (input, output) = (&args[1], &args[2]);
    let bytes = std::fs::read(input).expect("readable input");
    let channels = u16::from_le_bytes([bytes[22], bytes[23]]) as usize;
    let rate = u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]);
    let data = bytes.windows(4).position(|w| w == b"data").expect("data chunk") + 8;
    let samples: Vec<f32> = bytes[data..].as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b) as f32 / 32768.0).collect();

    let mut resampler = Resampler::new(rate, TARGET_RATE);
    let mut out = Vec::new();
    for block in downmix_to_mono(&samples, channels).chunks(480) {
        out.extend(resampler.process(block)); // small blocks, like an audio callback
    }

    let pcm: Vec<u8> = out.iter().flat_map(|s| ((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()).collect();
    let mut wav = Vec::new();
    wav.extend(b"RIFF");
    wav.extend((36 + pcm.len() as u32).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16u32.to_le_bytes());
    wav.extend(1u16.to_le_bytes()); // PCM
    wav.extend(1u16.to_le_bytes()); // mono
    wav.extend(TARGET_RATE.to_le_bytes());
    wav.extend((TARGET_RATE * 2).to_le_bytes());
    wav.extend(2u16.to_le_bytes());
    wav.extend(16u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend((pcm.len() as u32).to_le_bytes());
    wav.extend(pcm);
    std::fs::write(output, wav).expect("writable output");
    println!("{rate} Hz, {channels} ch -> {} samples at {TARGET_RATE} Hz", out.len());
}
