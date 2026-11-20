//! Pause-based blocks for live transcribing.
//!
//! Each speaker's stream goes through its own [`Segmenter`], which asks a [`SpeechDetector`]
//! whether each 32 ms frame is speech and cuts the stream into blocks for Whisper: a block ends
//! after a pause that follows speech, or at the quietest moment once it gets long. Audio without
//! speech never leaves the segmenter. File imports keep fixed 30 s chunks (`muesli-import`).
//!
//! Every number is a constant below, so they can be tuned in one place.

use crate::models::AudioChunk;

/// Samples per second of everything that reaches the segmenter.
pub const SAMPLE_RATE: usize = 16_000;
/// One detector frame: 512 samples, 32 ms (Silero's window at 16 kHz).
pub const FRAME: usize = 512;

/// A frame is speech from this probability on...
pub const SPEECH_ON: f32 = 0.5;
/// ...and speech goes on until the probability drops below this.
pub const SPEECH_OFF: f32 = 0.35;
/// A pause this long after speech ends a block.
pub const PAUSE_SECONDS: f64 = 1.0;
/// Speech shorter than this is held and joined to the next block.
pub const MIN_SPEECH_SECONDS: f64 = 1.0;
/// A held short utterance is sent on its own once the silence after it lasts this long.
pub const HOLD_SECONDS: f64 = 5.0;
/// A block never runs longer than this while Whisper keeps up...
pub const MAX_SECONDS: f64 = 10.0;
/// ...and is allowed to grow to this when Whisper falls behind (see [`Pacer`]).
pub const MAX_SECONDS_BEHIND: f64 = 30.0;
/// A forced cut is made at the quietest frame within this much of the block's end.
pub const CUT_SEARCH_SECONDS: f64 = 2.0;
/// Audio kept around speech, so word edges survive.
pub const PAD_SECONDS: f64 = 0.2;
/// What is left on stop is sent only when longer than this.
pub const TAIL_SECONDS: f64 = 1.0;
/// New audio is judged in steps of this many frames (0.512 s)...
pub const STEP_FRAMES: usize = 16;
/// ...with this many frames already judged in front of them (1.024 s), because the detector
/// starts afresh on each call.
pub const WARMUP_FRAMES: usize = 32;

/// Whole frames in `seconds`, rounded down so limits are never exceeded.
fn frames(seconds: f64) -> usize {
    (seconds * SAMPLE_RATE as f64 / FRAME as f64) as usize
}

/// Tells speech from everything else.
pub trait SpeechDetector: Send {
    /// One speech probability (0..1) per [`FRAME`] of `window`, whose length is a multiple of
    /// [`FRAME`]. The detector may start from a clean state on each call.
    fn probabilities(&mut self, window: &[f32]) -> Result<Vec<f32>, String>;
}

/// Calls a frame speech when it is loud enough: the stand-in for tests, the simulator and a
/// voice activity model that failed to load.
pub struct EnergyDetector {
    /// Root mean square from which a frame is speech.
    pub threshold: f32,
}

impl Default for EnergyDetector {
    fn default() -> Self {
        Self { threshold: 0.01 }
    }
}

impl SpeechDetector for EnergyDetector {
    fn probabilities(&mut self, window: &[f32]) -> Result<Vec<f32>, String> {
        Ok(window.chunks(FRAME).map(|f| if rms(f) >= self.threshold { 1.0 } else { 0.0 }).collect())
    }
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

/// A block being built, in absolute frame numbers.
struct Block {
    start: usize,
    first_speech: usize,
    /// End (exclusive) of the last speech frame.
    speech_end: usize,
}

/// Cuts one speaker's stream into blocks of speech. Not thread-safe: one worker owns it.
pub struct Segmenter {
    speaker: String,
    detector: Box<dyn SpeechDetector>,
    max_frames: usize,
    /// Samples not judged yet (less than a step, or less than a frame on finish).
    fresh: Vec<f32>,
    /// The last judged samples, for the detector's warm-up.
    history: Vec<f32>,
    /// Judged samples from frame `base` on, with each frame's loudness.
    audio: Vec<f32>,
    loudness: Vec<f32>,
    base: usize,
    /// Frames judged so far.
    judged: usize,
    in_speech: bool,
    block: Option<Block>,
}

impl Segmenter {
    pub fn new(speaker: &str, detector: Box<dyn SpeechDetector>) -> Self {
        Self {
            speaker: speaker.to_string(),
            detector,
            max_frames: frames(MAX_SECONDS),
            fresh: Vec::new(),
            history: Vec::new(),
            audio: Vec::new(),
            loudness: Vec::new(),
            base: 0,
            judged: 0,
            in_speech: false,
            block: None,
        }
    }

    /// The longest block, between [`MAX_SECONDS`] and [`MAX_SECONDS_BEHIND`]. Longer blocks also
    /// wait for the same amount past the normal maximum before a pause may end them.
    pub fn set_max_seconds(&mut self, seconds: f64) {
        self.max_frames = frames(seconds.clamp(MAX_SECONDS, MAX_SECONDS_BEHIND));
    }

    /// Adds samples and returns the blocks they complete.
    pub fn push(&mut self, samples: &[f32]) -> Vec<AudioChunk> {
        self.fresh.extend_from_slice(samples);
        let mut out = Vec::new();
        let step = STEP_FRAMES * FRAME;
        while self.fresh.len() >= step {
            let new: Vec<f32> = self.fresh.drain(..step).collect();
            self.judge(&new, &mut out);
        }
        out
    }

    /// The rest of the stream: what is left of the current block, whatever the pause, when it is
    /// longer than [`TAIL_SECONDS`].
    pub fn finish(&mut self) -> Vec<AudioChunk> {
        let whole = self.fresh.len() / FRAME * FRAME;
        let mut out = Vec::new();
        if whole > 0 {
            let new: Vec<f32> = self.fresh.drain(..whole).collect();
            self.judge(&new, &mut out);
        }
        let partial = std::mem::take(&mut self.fresh);
        if let Some(block) = self.block.take() {
            let mut samples = self.slice(block.start, block.speech_end + frames(PAD_SECONDS));
            if self.in_speech {
                samples.extend_from_slice(&partial);
            }
            if samples.len() > (TAIL_SECONDS * SAMPLE_RATE as f64) as usize {
                out.push(self.chunk(block.start, samples));
            }
        }
        out
    }

    fn judge(&mut self, new: &[f32], out: &mut Vec<AudioChunk>) {
        let mut window = self.history.clone();
        window.extend_from_slice(new);
        let n_new = new.len() / FRAME;
        // A detector that fails hears speech everywhere: Whisper's own voice detection still
        // skips the silence, as it did before blocks followed pauses.
        let probs = match self.detector.probabilities(&window) {
            Ok(p) if p.len() >= n_new => p[p.len() - n_new..].to_vec(),
            _ => vec![1.0; n_new],
        };
        self.history = window[window.len().saturating_sub(WARMUP_FRAMES * FRAME)..].to_vec();
        for (i, frame) in new.chunks(FRAME).enumerate() {
            self.in_speech = if self.in_speech { probs[i] >= SPEECH_OFF } else { probs[i] >= SPEECH_ON };
            self.audio.extend_from_slice(frame);
            self.loudness.push(rms(frame));
            self.judged += 1;
            self.advance(out);
        }
    }

    /// Updates the block after frame `judged - 1`.
    fn advance(&mut self, out: &mut Vec<AudioChunk>) {
        let f = self.judged - 1;
        let now = self.judged;
        let pad = frames(PAD_SECONDS);
        if self.in_speech {
            let block = self.block.get_or_insert(Block { start: f.saturating_sub(pad).max(self.base), first_speech: f, speech_end: now });
            block.speech_end = now;
        }
        if let Some(block) = &self.block {
            let gap = now - block.speech_end;
            let speech = block.speech_end - block.first_speech;
            let long_enough = speech >= frames(MIN_SPEECH_SECONDS) && block.speech_end - block.start >= self.min_frames();
            if gap >= frames(PAUSE_SECONDS) && (long_enough || gap >= frames(HOLD_SECONDS)) {
                let (start, end) = (block.start, block.speech_end + pad);
                self.emit(start, end, out);
                self.block = None;
            } else if now - block.start >= self.max_frames {
                self.cut(out);
            }
        }
        if self.block.is_none() {
            // Nothing to send: keep only the padding in front of speech that may come.
            let keep_from = now.saturating_sub(pad).max(self.base);
            self.drop_before(keep_from);
        }
    }

    /// The block is at its maximum: send it up to its quietest frame near the end.
    fn cut(&mut self, out: &mut Vec<AudioChunk>) {
        let block = self.block.as_ref().unwrap();
        let (start, speech_end) = (block.start, block.speech_end);
        let now = self.judged;
        let from = now.saturating_sub(frames(CUT_SEARCH_SECONDS)).max(start + 1);
        // The quietest frame; on a tie the latest, so the block is as long as allowed.
        let quietest = (from..now).fold(from, |best, f| if self.loud(f) <= self.loud(best) { f } else { best });
        let cut = quietest + 1;
        self.emit(start, cut, out);
        self.block = (speech_end > cut).then_some(Block { start: cut, first_speech: cut, speech_end });
    }

    fn loud(&self, frame: usize) -> f32 {
        self.loudness[frame - self.base]
    }

    /// The shortest block a pause may end: more than zero only while Whisper is behind.
    fn min_frames(&self) -> usize {
        self.max_frames - frames(MAX_SECONDS)
    }

    fn emit(&mut self, start: usize, end: usize, out: &mut Vec<AudioChunk>) {
        let samples = self.slice(start, end);
        out.push(self.chunk(start, samples));
        self.drop_before(end);
    }

    fn chunk(&self, start: usize, samples: Vec<f32>) -> AudioChunk {
        AudioChunk { speaker: self.speaker.clone(), offset_seconds: (start * FRAME) as f64 / SAMPLE_RATE as f64, samples }
    }

    fn slice(&self, start: usize, end: usize) -> Vec<f32> {
        let end = end.min(self.judged);
        self.audio[(start - self.base) * FRAME..(end - self.base) * FRAME].to_vec()
    }

    fn drop_before(&mut self, frame: usize) {
        if frame > self.base {
            let n = frame - self.base;
            self.audio.drain(..n * FRAME);
            self.loudness.drain(..n);
            self.base = frame;
        }
    }
}

/// Lengthens blocks when Whisper takes longer than the audio it transcribes, and shortens them
/// again once it keeps up. Whisper pads every block to 30 s before encoding, so a short block costs
/// nearly as much as a long one (`docs/AUDIO-SPIKE.md`).
pub struct Pacer {
    max_seconds: f64,
    ratio: Option<f64>,
}

/// Time taken over audio length, averaged, above which blocks grow...
pub const PACE_SLOW: f64 = 0.8;
/// ...and below which they shrink again...
pub const PACE_FAST: f64 = 0.4;
/// ...by this much at a time.
pub const PACE_STEP_SECONDS: f64 = 5.0;
/// Weight of the newest block in the average.
pub const PACE_WEIGHT: f64 = 0.3;

impl Default for Pacer {
    fn default() -> Self {
        Self { max_seconds: MAX_SECONDS, ratio: None }
    }
}

impl Pacer {
    /// The longest block the segmenters should make now.
    pub fn max_seconds(&self) -> f64 {
        self.max_seconds
    }

    /// Whisper took `took` seconds for a block of `audio` seconds.
    pub fn record(&mut self, audio: f64, took: f64) {
        if audio <= 0.0 {
            return;
        }
        let r = took / audio;
        let ratio = self.ratio.map_or(r, |avg| avg + PACE_WEIGHT * (r - avg));
        self.ratio = Some(ratio);
        if ratio > PACE_SLOW {
            self.max_seconds = (self.max_seconds + PACE_STEP_SECONDS).min(MAX_SECONDS_BEHIND);
        } else if ratio < PACE_FAST {
            self.max_seconds = (self.max_seconds - PACE_STEP_SECONDS).max(MAX_SECONDS);
        }
    }
}
