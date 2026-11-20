# Audio spike

Throwaway benchmarks behind `docs/AUDIO-SPIKE.md`. Not in the workspace, not shipped, not run by CI.

- `fake_silero.py`, `fake_whisper.py`: write model files with the real shapes and random or zero weights, for timing only (`python3 fake_whisper.py out.bin`; needs numpy).
- `aec/`: `cargo run --release` — echo cancellation on synthetic signals.
- `whisper/`: `cargo run --release --bin vad -- fake-silero.bin`, `cargo run --release --bin enc -- model.bin` — VAD cost per window, encoder time per block length. Works with the real models too.
