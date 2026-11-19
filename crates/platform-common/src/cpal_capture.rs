//! Microphone and system audio through `cpal`: the microphone on macOS and Windows, system audio
//! through WASAPI loopback (Windows) or a Core Audio process tap (macOS, the "System Audio
//! Recording Only" permission). Each stream is resampled to 16 kHz mono on a worker thread, so
//! the audio callback stays light.

use std::sync::mpsc;
use std::sync::Arc;
use std::thread::JoinHandle;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{ErrorKind, FromSample, Sample, SampleFormat, Stream};
use muesli_audio::{downmix_to_mono, Resampler, TARGET_RATE};
use muesli_core::ports::{AudioStream, MicCapture, OnError, OnSamples, SystemAudioCapture};

#[derive(Clone, Copy)]
enum Source {
    Microphone,
    /// What the default output device plays.
    SystemAudio,
}

fn device_error(e: cpal::Error) -> String {
    format!("Audio capture failed: {e}")
}

/// The default microphone.
#[derive(Default)]
pub struct CpalMic;

/// What the default output device plays.
#[derive(Default)]
pub struct CpalSystemAudio;

impl MicCapture for CpalMic {
    fn start(&self, on_samples: OnSamples, on_error: OnError) -> Result<Box<dyn AudioStream>, String> {
        CpalStream::open(Source::Microphone, on_samples, on_error).map(|s| Box::new(s) as Box<dyn AudioStream>)
    }
}

impl SystemAudioCapture for CpalSystemAudio {
    fn start(&self, on_samples: OnSamples, on_error: OnError) -> Result<Box<dyn AudioStream>, String> {
        CpalStream::open(Source::SystemAudio, on_samples, on_error).map(|s| Box::new(s) as Box<dyn AudioStream>)
    }
}

/// A running stream and the worker that resamples it.
pub struct CpalStream {
    stream: Option<Stream>,
    worker: Option<JoinHandle<()>>,
}

impl AudioStream for CpalStream {
    fn stop(&mut self) {
        self.stream = None; // dropping the stream stops it and closes the channel
        if let Some(worker) = self.worker.take() {
            let _ = worker.join(); // resamples what was still queued
        }
    }
}

impl Drop for CpalStream {
    fn drop(&mut self) {
        self.stop();
    }
}

impl CpalStream {
    fn open(source: Source, mut on_samples: OnSamples, on_error: OnError) -> Result<Self, String> {
        let (tx, rx) = mpsc::channel::<Vec<f32>>();
        let opened = open(source, Box::new(move |block| drop(tx.send(block))), on_error)?;
        let (rate, channels) = (opened.rate, opened.channels);
        let worker = std::thread::spawn(move || {
            let mut resampler = Resampler::new(rate, TARGET_RATE);
            for interleaved in rx {
                let out = resampler.process(&downmix_to_mono(&interleaved, channels));
                if !out.is_empty() {
                    on_samples(&out);
                }
            }
        });
        Ok(Self { stream: Some(opened.stream), worker: Some(worker) })
    }
}

struct Opened {
    stream: Stream,
    rate: u32,
    channels: usize,
}

type RawSamples = Box<dyn FnMut(Vec<f32>) + Send>;

/// Opens and starts a stream. `on_samples` gets interleaved f32 samples.
fn open(source: Source, mut on_samples: RawSamples, on_error: OnError) -> Result<Opened, String> {
    let host = cpal::default_host();
    let (device, config) = match source {
        Source::Microphone => {
            let d = host.default_input_device().ok_or("No microphone was found.")?;
            let c = d.default_input_config().map_err(device_error)?;
            (d, c)
        }
        Source::SystemAudio => {
            let d = host.default_output_device().ok_or("No audio output device was found to record the other side from.")?;
            let c = d.default_output_config().map_err(device_error)?;
            (d, c)
        }
    };
    let channels = config.channels() as usize;
    let rate = config.sample_rate();
    let format = config.sample_format();
    let config: cpal::StreamConfig = config.into();

    let errors = move |e: cpal::Error| match e.kind() {
        // Glitches and device switches the backend recovers from by itself.
        ErrorKind::Xrun | ErrorKind::RealtimeDenied | ErrorKind::DeviceChanged => {}
        _ => on_error(e.to_string()),
    };
    fn floats<T: Sample>(data: &[T]) -> Vec<f32>
    where
        f32: FromSample<T>,
    {
        data.iter().map(|s| f32::from_sample(*s)).collect()
    }
    let stream = match format {
        SampleFormat::F32 => device.build_input_stream(config, move |d: &[f32], _: &_| on_samples(floats(d)), errors, None),
        SampleFormat::I16 => device.build_input_stream(config, move |d: &[i16], _: &_| on_samples(floats(d)), errors, None),
        SampleFormat::I32 => device.build_input_stream(config, move |d: &[i32], _: &_| on_samples(floats(d)), errors, None),
        SampleFormat::U16 => device.build_input_stream(config, move |d: &[u16], _: &_| on_samples(floats(d)), errors, None),
        other => return Err(format!("Audio capture failed: unsupported sample format {other}")),
    }
    .map_err(device_error)?;
    stream.play().map_err(device_error)?;
    Ok(Opened { stream, rate, channels })
}

/// Opens a stream briefly and closes it again, so the OS asks for permission at a moment the
/// app chooses (the "Allow…" button in setup and Settings).
fn probe(source: Source) -> Result<(), String> {
    let _opened = open(source, Box::new(|_| {}), Arc::new(|_| {}))?;
    std::thread::sleep(std::time::Duration::from_millis(400));
    Ok(())
}

pub fn probe_microphone() -> Result<(), String> {
    probe(Source::Microphone)
}

/// macOS asks for "System Audio Recording"; Windows loopback needs no permission.
pub fn probe_system_audio() -> Result<(), String> {
    probe(Source::SystemAudio)
}
