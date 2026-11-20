//! The live audio path between capture and Whisper.
//!
//! Capture callbacks run on real-time audio threads, so they only hand their samples over a
//! channel. A worker owns one [`Segmenter`] per speaker (voice activity, pauses, blocks) and sends
//! finished blocks on to the transcribing thread. With an echo canceller, an [`EchoStage`] sits
//! in front of the microphone's segmenter: the system audio ("Others") is its reference, and
//! the microphone ("You") reaches its segmenter cleaned.

use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use muesli_core::ports::OnSamples;
use muesli_core::{AudioChunk, EchoCanceller, EchoStage, Pacer, Segmenter, SpeechDetector};

/// The microphone's speaker, and the system audio's, the echo stage's reference.
const MIC: &str = "You";
const SYSTEM: &str = "Others";

enum Audio {
    Samples(&'static str, Vec<f32>),
    Finish,
}

pub(crate) struct Live {
    tx: Sender<Audio>,
    worker: JoinHandle<()>,
}

impl Live {
    /// Starts the worker. `echo` cleans the microphone of the call (none: it goes as it is);
    /// `pacer` sets the longest block (the transcribing thread updates it); blocks go to
    /// `blocks`, which is dropped when the worker ends.
    pub fn start(
        speakers: Vec<(&'static str, Box<dyn SpeechDetector>)>,
        echo: Option<Box<dyn EchoCanceller>>,
        pacer: Arc<Mutex<Pacer>>,
        blocks: Sender<AudioChunk>,
    ) -> Self {
        let (tx, rx) = mpsc::channel::<Audio>();
        let worker = std::thread::spawn(move || {
            let mut segmenters: Vec<(&'static str, Segmenter)> =
                speakers.into_iter().map(|(speaker, detector)| (speaker, Segmenter::new(speaker, detector))).collect();
            let mut stage = echo.map(|canceller| EchoStage::new(Some(canceller)));
            let mut segment = |speaker: &str, samples: &[f32]| {
                let max = pacer.lock().unwrap().max_seconds();
                if let Some((_, segmenter)) = segmenters.iter_mut().find(|(s, _)| *s == speaker) {
                    segmenter.set_max_seconds(max);
                    for block in segmenter.push(samples) {
                        let _ = blocks.send(block);
                    }
                }
            };
            // Ends on Finish, or when every sender is gone (capture never started).
            for message in rx {
                match message {
                    Audio::Samples(speaker, samples) => match stage.as_mut() {
                        Some(stage) if speaker == MIC => {
                            stage.push_mic(&samples);
                            segment(MIC, &stage.take());
                        }
                        Some(stage) if speaker == SYSTEM => {
                            stage.push_reference(&samples);
                            segment(SYSTEM, &samples);
                            segment(MIC, &stage.take());
                        }
                        _ => segment(speaker, &samples),
                    },
                    Audio::Finish => break,
                }
            }
            if let Some(stage) = stage.as_mut() {
                segment(MIC, &stage.finish());
            }
            for (_, segmenter) in segmenters.iter_mut() {
                for block in segmenter.finish() {
                    let _ = blocks.send(block);
                }
            }
        });
        Self { tx, worker }
    }

    /// The capture callback for one speaker: copies the samples and passes them on, nothing more.
    pub fn feed(&self, speaker: &'static str) -> OnSamples {
        let tx = self.tx.clone();
        Box::new(move |samples: &[f32]| drop(tx.send(Audio::Samples(speaker, samples.to_vec()))))
    }

    /// Sends what is left of each stream, whatever the pause, and waits for the worker.
    pub fn finish(self) {
        let _ = self.tx.send(Audio::Finish);
        let _ = self.worker.join();
    }
}
