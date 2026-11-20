//! The live audio path between capture and Whisper.
//!
//! Capture callbacks run on real-time audio threads, so they only hand their samples over a
//! channel. A worker owns one [`Segmenter`] per speaker (voice activity, pauses, blocks) and sends
//! finished blocks on to the transcribing thread. Echo cancellation will sit here too, since the
//! worker sees both streams.

use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use muesli_core::ports::OnSamples;
use muesli_core::{AudioChunk, Pacer, Segmenter, SpeechDetector};

enum Audio {
    Samples(&'static str, Vec<f32>),
    Finish,
}

pub(crate) struct Live {
    tx: Sender<Audio>,
    worker: JoinHandle<()>,
}

impl Live {
    /// Starts the worker. `pacer` sets the longest block (the transcribing thread updates it);
    /// blocks go to `blocks`, which is dropped when the worker ends.
    pub fn start(speakers: Vec<(&'static str, Box<dyn SpeechDetector>)>, pacer: Arc<Mutex<Pacer>>, blocks: Sender<AudioChunk>) -> Self {
        let (tx, rx) = mpsc::channel::<Audio>();
        let worker = std::thread::spawn(move || {
            let mut segmenters: Vec<(&'static str, Segmenter)> =
                speakers.into_iter().map(|(speaker, detector)| (speaker, Segmenter::new(speaker, detector))).collect();
            // Ends on Finish, or when every sender is gone (capture never started).
            for message in rx {
                match message {
                    Audio::Samples(speaker, samples) => {
                        let max = pacer.lock().unwrap().max_seconds();
                        if let Some((_, segmenter)) = segmenters.iter_mut().find(|(s, _)| *s == speaker) {
                            segmenter.set_max_seconds(max);
                            for block in segmenter.push(&samples) {
                                let _ = blocks.send(block);
                            }
                        }
                    }
                    Audio::Finish => break,
                }
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
