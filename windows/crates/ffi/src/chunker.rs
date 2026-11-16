use std::sync::Arc;

use muesli_core::chunker::AudioChunker;
use muesli_core::models::AudioChunk;

/// Receives each finished chunk. Called while the chunker holds its lock, so it must not call
/// back into the chunker.
#[uniffi::export(with_foreign)]
pub trait ChunkSink: Send + Sync {
    fn chunk_ready(&self, chunk: AudioChunk);
}

/// Collects 16 kHz mono samples per speaker and hands fixed-length chunks to Whisper.
#[derive(uniffi::Object)]
pub struct Chunker {
    inner: AudioChunker,
}

#[uniffi::export]
impl Chunker {
    #[uniffi::constructor]
    pub fn new(chunk_seconds: f64, rate: f64, speakers: Vec<String>, sink: Arc<dyn ChunkSink>) -> Self {
        let speakers: Vec<&str> = speakers.iter().map(String::as_str).collect();
        Self { inner: AudioChunker::with_options(chunk_seconds, rate, &speakers, move |chunk| sink.chunk_ready(chunk)) }
    }

    pub fn append(&self, speaker: String, samples: Vec<f32>) {
        self.inner.append(&speaker, &samples);
    }

    /// Emits what is left of each stream. Tails of one second or less are dropped.
    pub fn finish(&self) {
        self.inner.finish();
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    struct Collect(Mutex<Vec<AudioChunk>>);

    impl ChunkSink for Collect {
        fn chunk_ready(&self, chunk: AudioChunk) {
            self.0.lock().unwrap().push(chunk);
        }
    }

    #[test]
    fn emits_whole_chunks_with_offsets() {
        let sink = Arc::new(Collect(Mutex::new(Vec::new())));
        let chunker = Chunker::new(1.0, 10.0, vec!["You".into()], sink.clone());
        chunker.append("You".into(), vec![0.0; 25]);
        chunker.finish();
        let got = sink.0.lock().unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[1].offset_seconds, 1.0);
    }
}
