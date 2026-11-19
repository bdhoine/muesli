use muesli_audio::{downmix_to_mono, Resampler};

fn sine(rate: u32, hz: f64, seconds: f64) -> Vec<f32> {
    (0..(rate as f64 * seconds) as usize).map(|i| (2.0 * std::f64::consts::PI * hz * i as f64 / rate as f64).sin() as f32 * 0.5).collect()
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

#[test]
fn downmix_averages_channels() {
    assert_eq!(downmix_to_mono(&[1.0, 3.0, 2.0, 4.0], 2), vec![2.0, 3.0]);
    assert_eq!(downmix_to_mono(&[1.0, 2.0], 1), vec![1.0, 2.0]);
}

#[test]
fn output_length_follows_the_ratio() {
    for (from, to) in [(48_000, 16_000), (44_100, 16_000), (16_000, 16_000), (8_000, 16_000)] {
        let mut r = Resampler::new(from, to);
        let out = r.process(&sine(from, 440.0, 2.0));
        let expected = (2.0 * to as f64) as i64;
        // Up to one unprocessed input chunk (1024 frames) may still be waiting inside.
        let slack = 1024 * to as i64 / from as i64 + 16;
        assert!(out.len() as i64 <= expected && expected - out.len() as i64 <= slack, "{from}->{to}: {} vs {expected}", out.len());
    }
}

#[test]
fn speech_band_tones_keep_their_level() {
    for (from, hz) in [(48_000, 440.0), (44_100, 1000.0), (48_000, 3000.0), (8_000, 1000.0)] {
        let mut r = Resampler::new(from, 16_000);
        let out = r.process(&sine(from, hz, 1.0));
        let level = rms(&out[2000..out.len() - 200]);
        assert!((level - 0.3535).abs() < 0.02, "{from} {hz}: {level}");
    }
}

#[test]
fn tones_above_the_new_nyquist_are_suppressed() {
    let mut r = Resampler::new(48_000, 16_000);
    let out = r.process(&sine(48_000, 12_000.0, 1.0)); // would alias to 4 kHz
    assert!(rms(&out[2000..out.len() - 200]) < 0.005);
}

#[test]
fn chunked_input_matches_one_shot_input() {
    let input = sine(44_100, 700.0, 1.0);
    let whole = Resampler::new(44_100, 16_000).process(&input);
    let mut r = Resampler::new(44_100, 16_000);
    let mut pieces = Vec::new();
    for chunk in input.chunks(441) {
        pieces.extend(r.process(chunk));
    }
    assert_eq!(whole.len(), pieces.len());
    assert!(whole.iter().zip(&pieces).all(|(a, b)| (a - b).abs() < 1e-6));
}

#[test]
fn silence_stays_silent_and_equal_rates_pass_through() {
    let out = Resampler::new(48_000, 16_000).process(&vec![0.0; 48_000]);
    assert!(out.iter().all(|v| v.abs() < 1e-6));
    let same = sine(16_000, 300.0, 0.1);
    assert_eq!(Resampler::new(16_000, 16_000).process(&same), same);
}
