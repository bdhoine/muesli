use std::sync::mpsc;
use std::sync::Arc;
use std::thread::JoinHandle;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{ErrorKind, FromSample, Sample, SampleFormat, Stream};
use muesli_core::{AudioChunk, AudioChunker};

use crate::{downmix_to_mono, Resampler, TARGET_RATE};

#[derive(Debug)]
pub enum CaptureError {
    NoMicrophone,
    NoOutputDevice,
    Device(String),
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoMicrophone => write!(f, "No microphone was found."),
            Self::NoOutputDevice => write!(f, "No audio output device was found to record the other side from."),
            Self::Device(why) => write!(f, "Audio capture failed: {why}"),
        }
    }
}

impl std::error::Error for CaptureError {}

fn device_error(e: cpal::Error) -> CaptureError {
    CaptureError::Device(e.to_string())
}

type OnError = Arc<dyn Fn(String) + Send + Sync>;
type OnSamples = Box<dyn FnMut(Vec<f32>) + Send>;

#[derive(Clone, Copy)]
enum Source {
    Microphone,
    /// What the default output device plays: WASAPI loopback on Windows, a Core Audio process
    /// tap on macOS.
    SystemAudio,
}

/// Captures the microphone ("You") and system audio ("Others") and emits ~30 s chunks, so no
/// meeting bot has to join the call.
pub struct AudioCapture {
    chunker: Arc<AudioChunker>,
    on_error: OnError,
    streams: Vec<Stream>,
    workers: Vec<JoinHandle<()>>,
}

impl AudioCapture {
    /// `on_error` is called (from an audio thread) when a stream breaks for good.
    pub fn new(emit: impl Fn(AudioChunk) + Send + Sync + 'static, on_error: impl Fn(String) + Send + Sync + 'static) -> Self {
        Self {
            chunker: Arc::new(AudioChunker::new(emit)),
            on_error: Arc::new(on_error),
            streams: Vec::new(),
            workers: Vec::new(),
        }
    }

    /// Starts both streams. If either fails, nothing is left running.
    pub fn start(&mut self) -> Result<(), CaptureError> {
        let result = self.add(Source::Microphone, "You").and_then(|()| self.add(Source::SystemAudio, "Others"));
        if result.is_err() {
            self.shut_down();
        }
        result
    }

    /// Stops capturing and emits what is left of each stream.
    pub fn stop(&mut self) {
        self.shut_down();
        self.chunker.finish();
    }

    fn shut_down(&mut self) {
        self.streams.clear(); // dropping a stream stops it and closes its channel
        for worker in self.workers.drain(..) {
            let _ = worker.join(); // resamples what was still queued
        }
    }

    fn add(&mut self, source: Source, speaker: &'static str) -> Result<(), CaptureError> {
        // The audio callback must stay light, so it only hands blocks to a worker that resamples.
        let (tx, rx) = mpsc::channel::<Vec<f32>>();
        let chunker = self.chunker.clone();
        let opened = open(source, Box::new(move |block| drop(tx.send(block))), self.on_error.clone())?;
        self.workers.push(std::thread::spawn(move || {
            let mut resampler = Resampler::new(opened.rate, TARGET_RATE);
            for interleaved in rx {
                let out = resampler.process(&downmix_to_mono(&interleaved, opened.channels));
                if !out.is_empty() {
                    chunker.append(speaker, &out);
                }
            }
        }));
        self.streams.push(opened.stream);
        Ok(())
    }
}

struct Opened {
    stream: Stream,
    rate: u32,
    channels: usize,
}

/// Opens and starts a stream. `on_samples` gets interleaved f32 samples.
fn open(source: Source, mut on_samples: OnSamples, on_error: OnError) -> Result<Opened, CaptureError> {
    let host = cpal::default_host();
    let (device, config) = match source {
        Source::Microphone => {
            let d = host.default_input_device().ok_or(CaptureError::NoMicrophone)?;
            let c = d.default_input_config().map_err(device_error)?;
            (d, c)
        }
        Source::SystemAudio => {
            let d = host.default_output_device().ok_or(CaptureError::NoOutputDevice)?;
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
        other => return Err(CaptureError::Device(format!("unsupported sample format {other}"))),
    }
    .map_err(device_error)?;
    stream.play().map_err(device_error)?;
    Ok(Opened { stream, rate, channels })
}

/// Opens a stream briefly and closes it again, so the OS asks for permission at a moment the
/// app chooses.
fn probe(source: Source) -> Result<(), CaptureError> {
    let _opened = open(source, Box::new(|_| {}), Arc::new(|_| {}))?;
    std::thread::sleep(std::time::Duration::from_millis(400));
    Ok(())
}

pub fn probe_microphone() -> Result<(), CaptureError> {
    probe(Source::Microphone)
}

/// macOS asks for "System Audio Recording"; Windows loopback needs no permission.
pub fn probe_system_audio() -> Result<(), CaptureError> {
    probe(Source::SystemAudio)
}
