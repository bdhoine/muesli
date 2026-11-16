//! UniFFI bindings of `muesli-core` for the Swift macOS app.
//!
//! Wrappers only: every function forwards to `muesli-core`, so the logic exists once and is tested
//! there. Types that UniFFI cannot map (lifetimes, tuples, `serde_json::Value`, closures) are
//! wrapped here, never changed in the core.

uniffi::setup_scaffolding!();

mod assistants;
mod chunker;
mod claude_desktop;
mod detection;
mod languages;
mod models;
mod paths;
mod preferences;
mod status;
mod store;

mod whisper_model;
#[cfg(feature = "whisper")]
mod transcriber;

/// The app version, shared by both apps (the workspace version).
#[uniffi::export]
pub fn muesli_version() -> String {
    muesli_core::VERSION.to_string()
}
