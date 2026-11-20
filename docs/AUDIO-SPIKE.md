# Audio pipeline spike (M0)

20 Nov 2026. Answers the three spike questions of the audio pipeline plan (pause-based blocks, echo cancellation, a macOS system-audio fallback). Everything here was run on a Linux container: 4 vCPU Intel Xeon @ 2.1 GHz, no GPU. **Nothing was run on a Mac or a PC, and no real model was available**: huggingface.co, where both models live, is blocked from the container. Where a number needed a model, it was measured with a file of the **same shape and zero or random weights**; compute cost does not depend on weight values, but the output is meaningless, so these runs give speed, never quality.

The code that produced every number is in `scripts/audio-spike/` (not part of the workspace, not shipped).

## 1. VAD on a rolling window

**Answer: yes, cheaply, but not with `segments_from_samples` and not with state carried between calls.**

What the code does (whisper.cpp 1.8.3 as vendored by whisper-rs-sys 0.15, `src/whisper.cpp`, `whisper_vad_detect_speech`):

- `WhisperVadContext::detect_speech(samples)` runs Silero over the samples in 512-sample frames (32 ms) and leaves one speech probability per frame in `probabilities()`. `segments_from_samples` is that plus whisper.cpp's own segment logic (min speech, min silence, padding), which only suits a whole buffer.
- **State does not carry between calls**: every call starts with `ggml_backend_buffer_clear(vctx->buffer, 0)` ("Reset LSTM hidden/cell states"). Inside one call the LSTM state carries from frame to frame.

Cost per call (`scripts/audio-spike/whisper/src/bin/vad.rs`, Silero v5 shapes with random weights, `scripts/audio-spike/fake_silero.py`; the real file is the same size, 885 KB):

| Window | 1 thread | 4 threads |
| --- | --- | --- |
| 0.512 s | 1.5 ms | 4.5 ms |
| 2.048 s | 6.1 ms | 18 ms |
| 4.096 s | 12.6 ms | 44 ms |
| 30 s | 93 ms | 284 ms |

About 0.1 ms per 32 ms frame on one thread, linear in the window. **Four threads (the default of `WhisperVadContextParams`) are three times slower** for a model this small: use `set_n_threads(1)`.

Recommendation:

- Keep the existing Silero model and download (V1). Call `detect_speech` and read `probabilities()`, not `segments_from_samples`.
- Because state resets per call, each step runs Silero over a short window that ends with the new audio: about 1 s already seen (to warm the LSTM up again) plus the new 0.5 s, and uses only the probabilities of the new frames. That is about 48 frames, ~5 ms per 0.5 s step per speaker, under 1% of one core for both speakers together. Whether one second of warm-up gives the same probabilities as an unbroken run needs the real model; not checked.
- The speech/pause decisions (hysteresis on the probabilities, the 1 s pause, the 10 s limit, the quietest cut) are ours, in `crates/core`, so they are testable with a stand-in detector; Silero only supplies probabilities per 32 ms frame.
- No energy-gate fallback is needed for cost. An energy detector is still useful as the stand-in for tests and the fakes.

## 2. Echo cancellation engine

Criteria: licence fit for an MIT app; real time on one core under ~10%; model under ~20 MB; callable from Rust without a heavy runtime; builds in CI on macOS, Windows and Linux.

| Engine | Licence | Model | Rust | Build | Verified here |
| --- | --- | --- | --- | --- | --- |
| **WebRTC AEC3, pure Rust port: `sonora` 0.3** | BSD-3-Clause | none (signal processing) | native crate | plain `cargo`, no C/C++ | built; synthetic benchmark below |
| WebRTC AEC3, Rust port: `aec3` 0.4 | MIT OR BSD-3-Clause | none | native crate | plain `cargo` | built; not benchmarked. Its README calls the API work in progress |
| WebRTC AEC3, C++ via `webrtc-audio-processing` 2.1 (`bundled`) | BSD-3-Clause (upstream WebRTC) | none | FFI | needs `meson` and `ninja` plus a C++ toolchain on every CI OS | build failed here without meson; not pursued |
| LocalVQE v1.4-AEC (DeepVQE derivative) | Apache-2.0 code and weights (per its repository) | ~3 MB F32 (203 K params) | none; C API (`localvqe_process_frame_f32`) | CMake ≥ 3.20, C++17, its own ggml | not built. Its own ggml next to whisper-rs's ggml in one binary risks duplicate symbols. Trained on DNS Challenge (CC BY 4.0) and AEC Challenge data, whose terms its README does not give. Windows "implemented but unverified" upstream |
| DTLN-aec | MIT (code); weights' terms not stated | 1.8 M–10.4 M params, TF-Lite (ONNX not confirmed for AEC) | none; needs TF-Lite or ONNX Runtime (`ort`, a large native library) or `tract` (pure Rust, untested with this stateful LSTM) | heavy runtime | not built; trained on AEC/DNS Challenge data, terms not stated |
| SpeexDSP MDF (`speexdsp`) | BSD-style | none | FFI | C | not tested: an older, weaker canceller |

Synthetic benchmark of `sonora` (`scripts/audio-spike/aec/src/main.rs`): speech-like far end (harmonics on a wandering pitch, syllable envelope), echo path with a delay plus four reflections, microphone noise at about -65 dBFS, 16 kHz mono, 10 ms frames, default `EchoCanceller` config, everything else off:

| Echo delay, gain | Single talk: echo reduction (ERLE) | Delay found | Double talk: near-to-echo before → after | Near-end level kept |
| --- | --- | --- | --- | --- |
| 40 ms, 0.5 | 34.6 dB | 36 ms | 6.6 → 10.3 dB | -1.0 dB |
| 150 ms, 0.3 | 30.2 dB | 148 ms | 11.0 → 16.2 dB | -0.5 dB |
| 280 ms, 0.3 | 30.2 dB | 276 ms | 11.0 → 14.6 dB | -0.7 dB |

- Headphones (far end playing, microphone hears only the near talker): level changes by -0.4 dB, the same as with a silent far end; this is AEC3's high-pass filter, not suppression. "You" is not attenuated (E4).
- CPU: 5–7 ms per second of audio on one thread of this Xeon, about 0.6% of one core (target < 10%). Not measured on Apple Silicon or Windows; `sonora` has NEON and SSE/AVX paths.
- Double talk: AEC3 keeps the near talker (within 1 dB) but removes only 4–5 dB of the echo while both talk. The text filter (`echo.rs`) stays needed there (E7).
- Not verified: real speech, real rooms, clock drift between two separate devices over a long meeting (AEC3 re-estimates the delay; untested here), Apple Silicon and Windows load.

**Recommendation: `sonora` (WebRTC AEC3 in pure Rust).** It meets every criterion that could be checked, has no model file, so E8 becomes "nothing to bundle or download" and REQUIREMENTS A10's two downloads stay two, builds with plain cargo on all three OSes, and has its own delay estimator (E3) and a transparent mode for headphones (E4). It does not cancel as well as a neural model in double talk; LocalVQE is the candidate to revisit if real-speech tests show that matters, at the cost of a C++/ggml build and a model download.

## 3. Speed headroom

Measured with a zero-weight file of the exact Large v3 Turbo q5_0 shapes (`scripts/audio-spike/fake_whisper.py`, 573 MB like the real file), `pcm_to_mel` + `encode`, 4 threads, CPU only (`scripts/audio-spike/whisper/src/bin/enc.rs`):

| Block length | Mel + encoder |
| --- | --- |
| 2 s | 23.0 s |
| 5 s | 22.2 s |
| 10 s | 22.3 s |
| 30 s | 23.6 s |

- **The encoder costs the same for any block length**: whisper.cpp pads every block to 30 s of mel before encoding. On this machine, even a 30 s block takes ~23 s to encode, before any decoding; a 10 s block takes the same 22 s. Pause-based blocks of a few seconds would multiply encoder work by up to 10 for continuous speech.
- The decoder (cost per token, so per word) was not measured: with zero weights the run aborted inside whisper.cpp.
- Not measured: Metal on Apple Silicon (the macOS build uses it), any real Windows laptop CPU, real models. This container is not representative of either; the shape of the result (a fixed cost per block) is what carries over.

Recommendation:

- V7 (lengthen blocks towards 30 s when Whisper falls behind, shorten again when it keeps up) is essential, not a nicety: on a CPU-only Windows laptop the live text may sit at 30 s blocks most of the time.
- whisper.cpp's `audio_ctx` parameter shrinks the encoder to the block's length and would cut the fixed cost roughly in proportion; it is known to cost accuracy and was not measured here. Worth trying on a real Mac and PC after M1, not before.
- Measure on a real machine: Whisper time per block against block length, with the real model, on Apple Silicon (Metal) and on a Windows laptop CPU.

## Questions for the maintainer

1. AEC engine: `sonora` is recommended; `webrtc-audio-processing` (the same algorithm in C++) is acceptable but needs meson in CI. Go with `sonora`?
2. With `sonora` there is no model to bundle or download. Agreed?
