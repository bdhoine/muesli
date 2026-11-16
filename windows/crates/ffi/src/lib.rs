//! UniFFI bindings of `muesli-core` for the Swift macOS app.
//!
//! Wrappers only: every function forwards to `muesli-core`, so the logic exists once and is tested
//! there. Types that UniFFI cannot map (lifetimes, tuples, `serde_json::Value`, closures) are
//! wrapped here, never changed in the core.

uniffi::setup_scaffolding!();

mod languages;
