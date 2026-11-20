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

## M2 decision: LocalVQE (20 Nov 2026)

The maintainer chose LocalVQE (best double talk for a transcriber) over `sonora`. Nothing of M2 is built yet. What was found, and the plan, for whoever continues:

**Facts checked** (source cloned from `https://github.com/localai-org/LocalVQE`, commit `7f61cd05095d42e56a135b303f5bb4d761da22b9`, 19 Oct 2026):

- Apache-2.0 code; the README says the weights are trained on the ICASSP 2023 DNS Challenge (CC BY 4.0) and fine-tuned on the ICASSP 2022/2023 AEC Challenge data, whose terms it does not give. **Check the AEC Challenge data terms before shipping.** Ship its LICENSE (and attribution) with the app.
- C API (`ggml/localvqe_api.h`): `localvqe_new(path)`, `localvqe_options_set_threads`, `localvqe_process_frame_f32(ctx, mic, ref, 256, out)` per 256-sample hop (16 ms at 16 kHz), `localvqe_reset`, `localvqe_last_error`, `localvqe_free`. Mic and reference go in together per hop; `ggml/test_align.cpp` suggests it aligns them itself. Verify how much delay it tolerates before relying on that (E3).
- Model for Muesli: **`localvqe-v1.4-aec-200K-f32.gguf`** (echo only, ~3 MB, 203 K params). It keeps voice, noise and room unchanged, which suits Whisper (trained on noisy audio; noise suppression often hurts recognition). The joint v1.3 (19 MB) also suppresses noise and reverb; compare only if v1.4-AEC leaves too much. SHA-256 from its allowlist (`ggml/model_hash.cpp`): `b6e43138588a83bfe903ab5e143b4020b91c1e1629f5a575ac5855ff0003c731`. LocalVQE refuses files not on that allowlist.
- Its README (not measured here): v1.4-AEC, 1 thread, Ryzen 9 7900: 1.29 ms p50 per 16 ms hop (8% of a core); double-talk echo MOS 4.20.
- Weights are only on Hugging Face (`https://huggingface.co/LocalAI-io/LocalVQE`), blocked in the environment where this was written, so **nothing was run**: no model, no speed or quality numbers.
- Build: CMake ≥ 3.20, C++17, and a **patched ggml** (git submodule `ggml/vendor/ggml` plus `ggml/patches/ggml-gru.patch`, applied at configure time). On x86 its CMake builds ggml CPU variants as separately loaded `.so`s (`GGML_BACKEND_DL`); force `GGML_BACKEND_DL=OFF`, `GGML_STATIC=ON`, `GGML_NATIVE=OFF` for a self-contained library.

**The risk to solve first: two ggml copies.** whisper-rs links whisper.cpp's ggml statically; LocalVQE has its own patched ggml with the same symbol names. Linking both statically into one binary fails or, worse, mixes them. Plan: build LocalVQE as a **shared library** (`LOCALVQE_BUILD_SHARED=ON`, ggml inside it, ggml symbols hidden: `-fvisibility=hidden`, `-Wl,--exclude-libs,ALL` on Linux) and **load it at run time** with `libloading`. macOS (two-level namespace) and Windows (per-DLL imports) keep the two apart; check Linux too since the workspace builds there. Loading at run time also gives the fail-safe for free: no library or no model means passthrough (E6).

**Plan:**

1. `crates/core`: `EchoCanceller` trait (one hop of mic plus reference in, cleaned mic out) and an `EchoStage` holding the framing (256-sample hops), the reference buffer (E3), passthrough when the far end has been silent for a while (E4), and the fail-safe (E6: an error, or more than about half real time spent over a window of hops, means raw microphone for the rest of the meeting, no message). Tests with a stand-in canceller and seeded synthetic signals.
2. `crates/engine/src/live.rs`: the worker already sees both streams in arrival order. "Others" samples feed the stage's reference before going to their segmenter; "You" samples go through the stage, then to theirs. Get the canceller from a factory in `Options` (or `SpeechModel`), `None` in tests by default, a stand-in in the simulator.
3. `crates/aec` (`muesli-aec`): LocalVQE as a git submodule (pinned commit, nested ggml submodule), `build.rs` with the `cmake` crate behind a `localvqe` feature (like `transcribe` in `crates/whisper`), the `libloading` wrapper implementing `EchoCanceller`. Set threads to 1. Ignored quality tests read clips from env vars (`MUESLI_AEC_MIC_WAV`, `MUESLI_AEC_REF_WAV`, `MUESLI_AEC_NEAR_WAV`): echo reduction ≥ 20 dB on single talk; near-end words kept, ideally scored by Whisper's output.
4. `apps/desktop`: bundle the shared library (Tauri resources; on macOS sign it with the app) and the 3 MB model. E8: under ~5 MB, so bundle it, fetched in CI at build time with the SHA-256 above and never committed. REQUIREMENTS A10 then still names exactly two downloads; say there that echo cancellation runs locally from a bundled model.
5. Simulator scenario where the microphone contains delayed system audio; docs (README, CONTRIBUTING, AGENTS, REQUIREMENTS A10/A10a, PLATFORMS, CHANGELOG "Without headphones, your microphone's copy of the call is removed before transcribing").

To run anything real, the environment needs `huggingface.co` allowed (Network access in the cloud environment's settings); that also allows real Whisper and Silero runs for sections 1 and 3 above.

## M2 step 0: LocalVQE measured on a Mac (20 Nov 2026)

Apple M1, macOS, LocalVQE at the pinned commit built as a shared library (`LOCALVQE_BUILD_SHARED=ON`; on arm64 its CMake already builds ggml statically, CPU only), model `localvqe-v1.4-aec-200K-f32.gguf` (2.9 MB, SHA-256 matches the allowlist above). Code: `scripts/audio-spike/localvqe/`. Speech: two `say` voices (16 kHz), the far end played through a synthetic echo path (delay plus four reflections). Whisper Large v3 Turbo q5_0 (Metal) transcribed the results **in the same process**.

- **Two ggml copies in one process work**: whisper-rs's (static, Metal) and LocalVQE's (inside its dylib, loaded with `libloading`). No crash, both give correct output. The dylib still exports its ggml symbols (870); `RTLD_LOCAL` plus macOS's two-level namespace keep them apart. Not checked on Windows or Linux.
- **Speed**: 1.89 ms p50, 2.1 ms p99 per 16 ms hop on one thread: 12% of one M1 core (its README: 8% on a Ryzen 9).
- **Delay is the catch**: LocalVQE's own alignment covers only a short lag (`dmax=16`). Echo reduction (ERLE, single talk) by echo delay:

| Echo delay | Reference as captured | Reference shifted by (delay − 20 ms) |
| --- | --- | --- |
| 40 ms | 48.3 dB | 62.5 dB |
| 150 ms | 11.1 dB | 60.3 dB |
| 280 ms | 8.2 dB | 55.3 dB |
| 500 ms | 0 dB | 62.4 dB |

  Real delays (output buffers, Bluetooth, the speaker-to-microphone path) are often over 100 ms, so Muesli has to **estimate the delay and shift the reference itself**, leaving LocalVQE the last few tens of milliseconds. A cross-correlation of 10 ms energy envelopes found a 280 ms echo exactly.
- **Near end kept**: microphone alone (far end silent, or headphones) comes out at ±0.0 dB.
- **Output latency**: `localvqe_process_frame_f32` returns each hop one hop late (256 samples, 16 ms). The stage has to drop that much at the start and flush it at the end, so timestamps and the passthrough switch stay aligned.
- **Double talk** (150 ms echo, reference not shifted, latency corrected): near-to-echo 12.0 dB in, 17.1 dB out.
- **What Whisper hears** (150 ms echo, reference not shifted): echo alone, raw gives the whole far-end sentence as "You"; cleaned gives "4. The budget for the second pool." (a residue, from the poor alignment at 150 ms). Double talk: raw and cleaned both give the near sentence exactly, and the cleaned one has no far-end words.

Plan change: `EchoStage` (M2 step 1) gets a delay estimator: it tracks the echo delay from the envelopes of both streams, shifts the reference by it, and re-estimates during the meeting (clock drift, a change of output device).

## End-of-meeting pass: cost on a Mac (20 Nov 2026)

Apple M1, Whisper Large v3 Turbo q5_0 (the Metal build), Silero voice activity on, languages English and Dutch (so each chunk also runs language detection). A 3.5-minute clip of continuous speech (a `say` voice), transcribed three ways (`crates/whisper/tests/transcribe.rs`, `end_pass_speed`):

| How | Compute per minute of speech | Per hour of speech | Words |
| --- | --- | --- | --- |
| Live-sized blocks, 8 s, greedy | 37.3 s | 37 min | 702 |
| 30 s chunks, greedy | 17.3 s | 17 min | 702 |
| 30 s chunks, beam search (the end pass) | 19.8 s | 20 min | 703 |

- The end pass costs about **20 minutes per hour of speech** on an M1. A one-hour meeting has roughly an hour of speech across both speakers, so expect about that after a long call. It is cancellable (Start, Quit) and keeps the live text then.
- Beam search adds only about 15% over greedy; the 30 s chunks themselves are half the cost of live-sized blocks (whisper.cpp pads every block to 30 s, section 3).
- Synthetic speech is too clean to show a quality difference (the word counts match); judge quality on real meetings.
- Not measured: a Windows laptop (CPU only, expect several times slower), and how much the per-chunk language detection (a second encoder run when more than one language is chosen) adds.
