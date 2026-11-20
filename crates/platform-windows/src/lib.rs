//! Muesli's Windows adapters. Everything here compiles only on Windows; elsewhere the crate is empty.
//!
//! The tray is drawn by the Tauri shell (`apps/desktop`), which passes its adapter to [`platform`].

#![cfg(windows)]

use std::sync::Arc;

use muesli_core::ports::{Platform, PlatformWords, Tray};
use muesli_platform_common::cpal_capture::{CpalMic, CpalSystemAudio};
use muesli_platform_common::{SystemClock, SystemPaths, SystemTrash};

pub mod detector;
pub mod login_item;
pub mod notifier;
pub mod permissions;
pub mod theme;

/// The id Windows knows the app by; the same as the bundle identifier.
pub const APP_ID: &str = "com.github.bdhoine.muesli";

/// Every Windows adapter, with the shell's tray.
pub fn platform(tray: Arc<dyn Tray>) -> Platform {
    let _ = notifier::register_app(APP_ID, "Muesli"); // so Windows shows prompts under our name
    Platform {
        clock: Arc::new(SystemClock),
        mic: Arc::new(CpalMic),
        system_audio: Arc::new(CpalSystemAudio),
        detector: Arc::new(detector::WindowsDetector::new()),
        notifier: Arc::new(notifier::Toasts { app_id: APP_ID }),
        permissions: Arc::new(permissions::WindowsPermissions { app_id: APP_ID }),
        tray,
        trash: Arc::new(SystemTrash { name: "Recycle Bin" }),
        paths: Arc::new(SystemPaths),
        login_item: Arc::new(login_item::RunKey::new("Muesli")),
        words: PlatformWords::WINDOWS,
    }
}
