//! Echo cancellation on the live microphone stream.
//!
//! Without headphones the microphone hears the call from the speakers. An [`EchoCanceller`]
//! (LocalVQE in the app, `muesli-aec`) removes that echo given the system audio as the
//! reference; [`EchoStage`] does everything around it, so it can be tested with a stand-in:
//!
//! - framing both streams into hops of [`HOP`] samples;
//! - lining the reference up with the microphone: by sample count, padded with silence where the
//!   system audio delivered nothing (Windows loopback is silent between sounds), shifted by the
//!   echo delay it estimates from both streams' loudness (the canceller only covers a few tens of
//!   milliseconds itself; docs/AUDIO-SPIKE.md);
//! - the canceller's latency, so the output lines up with the input sample for sample;
//! - passing the microphone through untouched while the far end has been quiet for a while
//!   (headphones, or nobody else talking);
//! - giving up for good, silently, when the canceller fails or cannot keep up: the raw
//!   microphone is always better than a gap.
//!
//! The text filter (`echo.rs`) stays behind it for what slips through.

use std::collections::VecDeque;
use std::time::Instant;

use crate::segmenter::SAMPLE_RATE;

/// Samples per hop: 16 ms at 16 kHz, LocalVQE's hop.
pub const HOP: usize = 256;
/// Loudness envelope frame for the delay estimate: 10 ms.
const ENV: usize = 160;
/// Echo delays searched, in milliseconds: the microphone up to this far behind the reference...
pub const MAX_DELAY_MS: i64 = 800;
/// ...or this far ahead (the microphone stream started later than the system audio).
pub const MIN_DELAY_MS: i64 = -300;
/// Left for the canceller's own alignment when shifting the reference.
pub const LEAD_MS: i64 = 20;
/// Loudness history compared for an estimate...
pub const ESTIMATE_WINDOW_SECONDS: f64 = 4.0;
/// ...re-estimated this often (clock drift, another output device).
pub const ESTIMATE_EVERY_SECONDS: f64 = 1.0;
/// Pearson correlation of the envelopes from which an estimate is trusted.
pub const ESTIMATE_MIN_CORRELATION: f32 = 0.4;
/// ...and from which it replaces the delay in use: by this much better correlation.
pub const ESTIMATE_MARGIN: f32 = 0.1;
/// A changed delay is taken once the same one has won this many estimates in a row.
pub const ESTIMATE_WINS: usize = 3;
/// The far end counts as playing from this loudness (RMS, about -50 dBFS)...
pub const FAR_RMS: f32 = 0.003;
/// ...and the microphone passes untouched once it has been quiet this long.
pub const FAR_QUIET_SECONDS: f64 = 2.0;
/// The microphone waits at most this long for its reference; after that the reference counts as
/// silent there (it stalled, or delivers nothing while nothing plays).
pub const WAIT_SECONDS: f64 = 0.3;
/// Hops the canceller hears again before it starts (again) cleaning: 64 ms.
pub const PRIME_HOPS: usize = 4;
/// The canceller may use at most this share of real time, measured over [`BUDGET_HOPS`]...
pub const BUDGET: f64 = 0.5;
/// ...hops (about one second).
pub const BUDGET_HOPS: usize = 64;

fn samples_in(seconds: f64) -> usize {
    (seconds * SAMPLE_RATE as f64) as usize
}

fn ms_to_samples(ms: i64) -> i64 {
    ms * SAMPLE_RATE as i64 / 1000
}

/// Removes the echo of `reference` from `mic`, one hop at a time.
pub trait EchoCanceller: Send {
    /// How many samples the output lags behind the input (at most [`HOP`]).
    fn latency(&self) -> usize {
        0
    }
    /// One hop: `mic`, `reference` and `out` are [`HOP`] samples long.
    fn process(&mut self, mic: &[f32], reference: &[f32], out: &mut [f32]) -> Result<(), String>;
    /// Forgets the stream so far (the next hop starts afresh).
    fn reset(&mut self);
}

fn rms(x: &[f32]) -> f32 {
    if x.is_empty() {
        return 0.0;
    }
    (x.iter().map(|s| s * s).sum::<f32>() / x.len() as f32).sqrt()
}

/// One stream's loudness per [`ENV`] frame, from absolute frame `base` on.
#[derive(Default)]
struct Envelope {
    frames: VecDeque<f32>,
    base: usize,
    partial: Vec<f32>,
}

impl Envelope {
    fn push(&mut self, x: &[f32]) {
        self.partial.extend_from_slice(x);
        let whole = self.partial.len() / ENV * ENV;
        for frame in self.partial[..whole].chunks(ENV) {
            self.frames.push_back(rms(frame));
        }
        self.partial.drain(..whole);
    }
    fn get(&self, i: usize) -> Option<f32> {
        i.checked_sub(self.base).and_then(|k| self.frames.get(k).copied())
    }
    fn keep_from(&mut self, from: usize) {
        while self.base < from && !self.frames.is_empty() {
            self.frames.pop_front();
            self.base += 1;
        }
    }
}

/// The echo stage of one live meeting. Not thread-safe: the live worker owns it.
pub struct EchoStage {
    /// `None` once given up (or never had one): the microphone passes untouched.
    canceller: Option<Box<dyn EchoCanceller>>,
    latency: usize,
    clock: Box<dyn FnMut() -> f64 + Send>,
    /// Reference samples from absolute index `ref_base` on.
    reference: VecDeque<f32>,
    ref_base: usize,
    /// Microphone samples not processed yet, from absolute index `mic_base` on.
    mic: Vec<f32>,
    mic_base: usize,
    /// The last `latency` microphone samples, for passthrough, and the last [`PRIME_HOPS`] hops.
    tail: Vec<f32>,
    recent: Vec<f32>,
    /// Output samples still to drop at the start (the latency), and output ready to take.
    to_drop: usize,
    out: Vec<f32>,
    mic_env: Envelope,
    ref_env: Envelope,
    /// Echo delay: the microphone lags the reference by this many samples.
    delay: i64,
    estimated: bool,
    /// A different delay that keeps winning (envelope frames), and how often it has in a row.
    candidate: Option<(i64, usize)>,
    next_estimate: usize,
    /// Absolute microphone index up to which the far end counts as playing.
    far_until: Option<usize>,
    cancelling: bool,
    times: VecDeque<f64>,
}

impl EchoStage {
    /// A stage around `canceller`; `None` passes the microphone through untouched.
    pub fn new(canceller: Option<Box<dyn EchoCanceller>>) -> Self {
        let start = Instant::now();
        Self::with_clock(canceller, Box::new(move || start.elapsed().as_secs_f64()))
    }

    /// Like `new`, with `clock` (seconds) for the time budget, so tests can make a canceller slow.
    pub fn with_clock(canceller: Option<Box<dyn EchoCanceller>>, clock: Box<dyn FnMut() -> f64 + Send>) -> Self {
        let latency = canceller.as_ref().map_or(0, |c| c.latency().min(HOP));
        Self {
            canceller,
            latency,
            clock,
            reference: VecDeque::new(),
            ref_base: 0,
            mic: Vec::new(),
            mic_base: 0,
            tail: vec![0.0; latency],
            recent: vec![0.0; PRIME_HOPS * HOP],
            to_drop: latency,
            out: Vec::new(),
            mic_env: Envelope::default(),
            ref_env: Envelope::default(),
            delay: 0,
            estimated: false,
            candidate: None,
            next_estimate: samples_in(ESTIMATE_EVERY_SECONDS),
            far_until: None,
            cancelling: false,
            times: VecDeque::new(),
        }
    }

    /// Whether the canceller is still in use (false after giving up, or without one).
    pub fn active(&self) -> bool {
        self.canceller.is_some()
    }

    /// The echo delay in use, in seconds (the microphone behind the reference).
    pub fn delay_seconds(&self) -> f64 {
        self.delay as f64 / SAMPLE_RATE as f64
    }

    /// System audio, as it arrives.
    pub fn push_reference(&mut self, samples: &[f32]) {
        // After a stall (nothing delivered while nothing played), the system audio arrives
        // together with the microphone again: silence fills the gap up to where the microphone is.
        let mic_end = self.mic_base + self.mic.len();
        let ref_end = self.ref_end() as usize;
        if mic_end > ref_end + samples_in(WAIT_SECONDS) {
            let gap = vec![0.0; mic_end - ref_end];
            self.ref_env.push(&gap);
            self.reference.extend(gap);
        }
        self.ref_env.push(samples);
        self.reference.extend(samples.iter().copied());
        self.pump(false);
        self.trim(); // also when the microphone has stopped delivering
    }

    /// Microphone audio, as it arrives. The cleaned microphone comes out of [`take`](Self::take).
    pub fn push_mic(&mut self, samples: &[f32]) {
        self.mic_env.push(samples);
        self.mic.extend_from_slice(samples);
        self.pump(false);
    }

    /// The cleaned microphone so far, as many samples as went in (minus what is still in flight).
    pub fn take(&mut self) -> Vec<f32> {
        std::mem::take(&mut self.out)
    }

    /// Processes what is left, whatever the reference, and returns the rest of the microphone.
    pub fn finish(&mut self) -> Vec<f32> {
        let pad = (HOP - self.mic.len() % HOP) % HOP;
        self.mic.extend(std::iter::repeat_n(0.0, pad));
        self.pump(true);
        // What the canceller still holds (its latency) comes out with one more hop of silence.
        if self.latency > 0 {
            let mut hop = vec![0.0; HOP];
            let flushed = self.cancelling && self.canceller.as_mut().is_some_and(|c| c.process(&[0.0; HOP], &[0.0; HOP], &mut hop).is_ok());
            let held = if flushed { hop[..self.latency].to_vec() } else { self.tail.clone() };
            self.emit(&held);
        }
        // The padding never leaves: exactly as many samples come out as went in.
        let mut out = self.take();
        out.truncate(out.len().saturating_sub(pad));
        out
    }

    fn emit(&mut self, samples: &[f32]) {
        let drop = self.to_drop.min(samples.len());
        self.to_drop -= drop;
        self.out.extend_from_slice(&samples[drop..]);
    }

    /// Reference sample at absolute index `i` (silence outside what is held).
    fn reference_at(&self, i: i64) -> f32 {
        if i < self.ref_base as i64 {
            return 0.0;
        }
        self.reference.get((i - self.ref_base as i64) as usize).copied().unwrap_or(0.0)
    }

    fn ref_end(&self) -> i64 {
        (self.ref_base + self.reference.len()) as i64
    }

    /// Where the reference for microphone sample `t` starts: the delay minus the lead.
    fn shift(&self) -> i64 {
        self.delay - ms_to_samples(LEAD_MS)
    }

    fn pump(&mut self, finishing: bool) {
        while self.mic.len() >= HOP {
            let t = self.mic_base as i64;
            let need = t - self.shift() + HOP as i64;
            if need > self.ref_end() {
                let waited = self.mic.len() as i64 - HOP as i64;
                if !finishing && waited < samples_in(WAIT_SECONDS) as i64 {
                    return; // the reference may still come
                }
                // It stalled: silence stands in for this hop (`push_reference` lines up what
                // comes after the gap).
                let missing = (need - self.ref_end()) as usize;
                self.reference.extend(std::iter::repeat_n(0.0, missing));
                self.ref_env.push(&vec![0.0; missing]);
            }
            let reference: Vec<f32> = (0..HOP as i64).map(|k| self.reference_at(t - self.shift() + k)).collect();
            let mic: Vec<f32> = self.mic.drain(..HOP).collect();
            self.mic_base += HOP;
            self.hop(&mic, &reference);
            self.estimate();
            self.trim();
        }
    }

    fn hop(&mut self, mic: &[f32], reference: &[f32]) {
        let t = self.mic_base - HOP;
        if rms(reference) >= FAR_RMS {
            self.far_until = Some(t + HOP + samples_in(FAR_QUIET_SECONDS));
        }
        let far = self.far_until.is_some_and(|until| t < until);
        let mut cleaned = None;
        // Starting (again): the canceller hears the last few hops first, so it has a history to
        // look back into, and its delayed first samples are cleaned too.
        let primer: Vec<(Vec<f32>, Vec<f32>)> = if far && !self.cancelling && self.canceller.is_some() {
            (0..PRIME_HOPS)
                .map(|j| {
                    let u = t as i64 - ((PRIME_HOPS - j) * HOP) as i64;
                    let mic = self.recent[j * HOP..(j + 1) * HOP].to_vec();
                    let reference = (0..HOP as i64).map(|k| self.reference_at(u - self.shift() + k)).collect();
                    (mic, reference)
                })
                .collect()
        } else {
            Vec::new()
        };
        if far {
            if let Some(canceller) = self.canceller.as_mut() {
                let mut primed = Ok(());
                if !self.cancelling {
                    canceller.reset();
                    let mut scratch = vec![0.0; HOP];
                    for (m, r) in &primer {
                        primed = primed.and(canceller.process(m, r, &mut scratch));
                    }
                }
                let mut out = vec![0.0; HOP];
                let started = (self.clock)();
                let result = primed.and(canceller.process(mic, reference, &mut out));
                let spent = (self.clock)() - started;
                self.times.push_back(spent);
                if self.times.len() > BUDGET_HOPS {
                    self.times.pop_front();
                }
                let over_budget = self.times.len() == BUDGET_HOPS
                    && self.times.iter().sum::<f64>() > BUDGET * (BUDGET_HOPS * HOP) as f64 / SAMPLE_RATE as f64;
                if result.is_err() || over_budget {
                    self.canceller = None; // for the rest of the meeting
                } else {
                    cleaned = Some(out);
                }
            }
        }
        let output = match cleaned {
            Some(out) => {
                self.cancelling = true;
                out
            }
            None => {
                self.cancelling = false;
                // The raw microphone, as late as the canceller's output would be.
                let mut delayed = self.tail.clone();
                delayed.extend_from_slice(&mic[..HOP - self.latency]);
                delayed
            }
        };
        self.tail = mic[HOP - self.latency..].to_vec();
        self.recent.drain(..HOP);
        self.recent.extend_from_slice(mic);
        self.emit(&output);
    }

    /// Re-estimates the echo delay every [`ESTIMATE_EVERY_SECONDS`] of microphone, from the
    /// last [`ESTIMATE_WINDOW_SECONDS`] of both envelopes, when the far end played in it.
    fn estimate(&mut self) {
        if self.mic_base < self.next_estimate {
            return;
        }
        self.next_estimate = self.mic_base + samples_in(ESTIMATE_EVERY_SECONDS);
        let window = samples_in(ESTIMATE_WINDOW_SECONDS) / ENV;
        let end = self.mic_base / ENV;
        if end < window {
            return;
        }
        let start = end - window;
        let (min_lag, max_lag) = (ms_to_samples(MIN_DELAY_MS) / ENV as i64, ms_to_samples(MAX_DELAY_MS) / ENV as i64);
        let mic: Vec<f32> = (start..end).map(|i| self.mic_env.get(i).unwrap_or(0.0)).collect();
        if mic.iter().all(|&x| x < FAR_RMS) {
            return;
        }
        let correlation = |lag: i64| {
            let far: Vec<f32> = (start as i64..end as i64)
                .map(|i| if i - lag < 0 { 0.0 } else { self.ref_env.get((i - lag) as usize).unwrap_or(0.0) })
                .collect();
            if far.iter().all(|&x| x < FAR_RMS) {
                return None;
            }
            Some(pearson(&mic, &far))
        };
        let Some((r, lag)) = (min_lag..=max_lag).filter_map(|lag| correlation(lag).map(|r| (r, lag))).max_by(|a, b| a.0.total_cmp(&b.0))
        else {
            return;
        };
        if r < ESTIMATE_MIN_CORRELATION {
            return;
        }
        if !self.estimated {
            self.estimated = true;
            self.set_delay(lag * ENV as i64);
            return;
        }
        // A new delay has to explain the microphone clearly better than the one in use: while
        // both sides talk the envelopes correlate worse, and a jump would undo what the
        // canceller has learnt.
        let current = correlation(self.delay / ENV as i64).unwrap_or(f32::MIN);
        if r <= current + ESTIMATE_MARGIN {
            self.candidate = None;
            return;
        }
        // The same new delay (within a frame) has to win several estimates in a row.
        let wins = match self.candidate {
            Some((seen, wins)) if (seen - lag).abs() <= 1 => wins + 1,
            _ => 1,
        };
        self.candidate = Some((lag, wins));
        if wins >= ESTIMATE_WINS {
            self.set_delay(lag * ENV as i64);
            self.candidate = None;
        }
    }

    /// A new delay moves the reference all at once, which the canceller's own alignment does
    /// not follow (LocalVQE loses the echo for good): it starts again, primed, at the new one.
    fn set_delay(&mut self, delay: i64) {
        if delay != self.delay {
            self.delay = delay;
            self.cancelling = false;
        }
    }

    /// Drops reference and envelopes no estimate or hop can reach any more.
    fn trim(&mut self) {
        let keep_back = samples_in(ESTIMATE_WINDOW_SECONDS) as i64 + ms_to_samples(MAX_DELAY_MS) + HOP as i64;
        // A microphone that stopped must not let the reference pile up.
        let ahead = samples_in(10.0) as i64;
        let from = (self.mic_base as i64 - keep_back).max(self.ref_end() - keep_back - ahead).max(0) as usize;
        while self.ref_base < from && !self.reference.is_empty() {
            self.reference.pop_front();
            self.ref_base += 1;
        }
        self.ref_env.keep_from(from / ENV);
        self.mic_env.keep_from(from / ENV);
    }
}

fn pearson(a: &[f32], b: &[f32]) -> f32 {
    let n = a.len() as f32;
    let (ma, mb) = (a.iter().sum::<f32>() / n, b.iter().sum::<f32>() / n);
    let (mut cov, mut va, mut vb) = (0.0, 0.0, 0.0);
    for (x, y) in a.iter().zip(b) {
        cov += (x - ma) * (y - mb);
        va += (x - ma) * (x - ma);
        vb += (y - mb) * (y - mb);
    }
    if va <= 0.0 || vb <= 0.0 {
        return 0.0;
    }
    cov / (va.sqrt() * vb.sqrt())
}

/// Subtracts the reference at the lag (within twice [`LEAD_MS`]) and gain that leave nothing:
/// the stand-in canceller for tests and the simulator. It learns them while only the far end
/// plays (where one lag and gain explain the microphone exactly) and keeps them through double
/// talk and resets, like a converged canceller. Exact for an echo that is a scaled, delayed copy
/// of the reference; useless for anything real.
pub struct StandInCanceller {
    latency: usize,
    /// Reference history, so a lag can look back.
    history: Vec<f32>,
    /// Input waiting to come out `latency` samples late.
    pending: Vec<f32>,
    /// The echo path learnt so far: lag and gain, and whether they fitted exactly.
    lag: usize,
    gain: f32,
    learnt: bool,
    /// Hops since all lags were last tried.
    since_search: usize,
}

impl StandInCanceller {
    pub fn new(latency: usize) -> Self {
        let latency = latency.min(HOP);
        Self {
            latency,
            history: Self::empty_history(),
            pending: vec![0.0; latency],
            lag: 0,
            gain: 0.0,
            learnt: false,
            since_search: SEARCH_EVERY,
        }
    }

    fn empty_history() -> Vec<f32> {
        vec![0.0; ms_to_samples(2 * LEAD_MS) as usize]
    }
}

/// The stand-in tries every lag at most this often (hops), to stay cheap in debug builds.
const SEARCH_EVERY: usize = 8;

impl EchoCanceller for StandInCanceller {
    fn latency(&self) -> usize {
        self.latency
    }

    fn process(&mut self, mic: &[f32], reference: &[f32], out: &mut [f32]) -> Result<(), String> {
        let back = ms_to_samples(2 * LEAD_MS) as usize;
        self.history.extend_from_slice(reference);
        let at = self.history.len() - HOP;
        let fit = |lag: usize| {
            let r = &self.history[at - lag..at - lag + HOP];
            let rr: f32 = r.iter().map(|x| x * x).sum();
            if rr <= 0.0 {
                return (f32::MAX, lag, 0.0);
            }
            let gain = r.iter().zip(mic).map(|(x, m)| x * m).sum::<f32>() / rr;
            (r.iter().zip(mic).map(|(x, m)| (m - gain * x).powi(2)).sum::<f32>(), lag, gain)
        };
        let mic_energy: f32 = mic.iter().map(|x| x * x).sum();
        // Only a microphone that hears something can teach the path (silence fits any lag).
        let exact = |left: f32| rms(mic) >= FAR_RMS && left <= 1e-4 * mic_energy;
        self.since_search += 1;
        // Until it has learnt the path it tries every hop; after that, now and then.
        if !(self.learnt && exact(fit(self.lag).0)) && (!self.learnt || self.since_search >= SEARCH_EVERY) {
            self.since_search = 0;
            let best = (0..=back).map(fit).min_by(|a, b| a.0.total_cmp(&b.0)).unwrap();
            // Only an exact fit teaches the path; before that, the best guess stands in.
            if exact(best.0) || (!self.learnt && best.0 < f32::MAX) {
                (self.lag, self.gain, self.learnt) = (best.1, best.2, exact(best.0));
            }
        }
        let r = &self.history[at - self.lag..at - self.lag + HOP];
        let cleaned: Vec<f32> = mic.iter().zip(r).map(|(m, x)| m - self.gain * x).collect();
        self.history.drain(..self.history.len() - back - HOP);
        self.pending.extend_from_slice(&cleaned);
        out.copy_from_slice(&self.pending[..HOP]);
        self.pending.drain(..HOP);
        Ok(())
    }

    fn reset(&mut self) {
        self.history = Self::empty_history();
        self.pending = vec![0.0; self.latency];
    }
}
