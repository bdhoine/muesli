//! Muesli's behaviour, free of any OS and any UI.
//!
//! The [`Engine`] owns what the app does: it watches for meetings (through the `MeetingDetector`
//! port), asks whether to start or stop (the `Notifier` port, or Muesli's own prompt window),
//! transcribes (the `MicCapture` / `SystemAudioCapture` ports and a [`SpeechModel`]), saves,
//! imports audio files, and keeps the tray menu current (the `Tray` port).
//!
//! The UI decides nothing. Each window asks for its view state ([`Engine::view`]) and sends
//! [`Intent`]s back; [`Host`] is what the shell around it (Tauri, or the simulator) does for the
//! engine: open windows, pick files, reveal a file, quit.

mod engine;
pub mod fake;
mod final_pass;
mod host;
mod intent;
mod live;
mod speech;
pub mod views;

pub use engine::{Engine, Options, ANSWER_TIMEOUT, CALL_END_DELAY, CHECK_EVERY, FIRST_CHECK_AFTER, TOAST_SECONDS};
pub use host::{Host, Window};
pub use intent::Intent;
pub use speech::{ModelStatus, SpeechModel, Transcriber};
