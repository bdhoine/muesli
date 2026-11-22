//! Muesli's macOS adapters. Everything here compiles only on macOS; elsewhere the crate is empty.
//!
//! Microphone and system audio go through cpal (`platform-common`; system audio is a Core Audio
//! process tap, macOS 14.4+). Meeting detection reads the Core Audio process list, Teams and the
//! calendar (EventKit). Questions are notifications with buttons (UNUserNotificationCenter);
//! permissions come from AVFoundation, EventKit, the notification settings and (system audio, private) TCC; deleted files go to
//! the Trash through NSFileManager; start at login is SMAppService (`launch` tells a start by the login
//! item from a start by a person).

#![cfg(target_os = "macos")]

use std::sync::Arc;

use muesli_core::ports::{Platform, PlatformWords, Tray};
use muesli_platform_common::cpal_capture::{CpalMic, CpalSystemAudio};
use muesli_platform_common::{SystemClock, SystemPaths};

pub mod calendar;
pub mod detector;
pub mod launch;
pub mod menu_icons;
pub mod menubar;
pub mod notifier;
pub mod system;
pub mod tcc;

/// Every macOS adapter, with the shell's tray.
pub fn platform(tray: Arc<dyn Tray>) -> Platform {
    Platform {
        clock: Arc::new(SystemClock),
        mic: Arc::new(CpalMic),
        system_audio: Arc::new(CpalSystemAudio),
        detector: Arc::new(detector::MacDetector::new()),
        notifier: Arc::new(notifier::Notifications::new()),
        permissions: Arc::new(system::MacPermissions),
        tray,
        trash: Arc::new(system::FinderTrash),
        paths: Arc::new(SystemPaths),
        login_item: Arc::new(system::AppService),
        words: PlatformWords::MAC,
    }
}
