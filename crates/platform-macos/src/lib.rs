//! Muesli's macOS adapters. Everything here compiles only on macOS; elsewhere the crate is empty.
//!
//! Milestone 2 state: meeting detection (Core Audio process list and Teams), cpal capture, the
//! clock, folders and the trash are real. Notifications, permissions and the login item are
//! placeholders until milestone 6 (UNUserNotificationCenter, AVFoundation/TCC checks,
//! SMAppService); until then questions go to Muesli's own prompt window.

#![cfg(target_os = "macos")]

use std::sync::Arc;

use muesli_core::ports::{
    Access, LoginItem, Notice, Notifier, OnAction, OnAnswer, Permission, Permissions, Platform, PlatformWords, Question, Tray,
};
use muesli_platform_common::cpal_capture::{probe_microphone, probe_system_audio, CpalMic, CpalSystemAudio};
use muesli_platform_common::{SystemClock, SystemPaths, SystemTrash};

pub mod detector;

/// Every macOS adapter, with the shell's tray.
pub fn platform(tray: Arc<dyn Tray>) -> Platform {
    Platform {
        clock: Arc::new(SystemClock),
        mic: Arc::new(CpalMic),
        system_audio: Arc::new(CpalSystemAudio),
        detector: Arc::new(detector::MacDetector::new()),
        notifier: Arc::new(NoNotifications),
        permissions: Arc::new(MacPermissions),
        tray,
        trash: Arc::new(SystemTrash { name: "Trash" }),
        paths: Arc::new(SystemPaths),
        login_item: Arc::new(NoLoginItem),
        words: PlatformWords::MAC,
    }
}

/// Placeholder until UNUserNotificationCenter (milestone 6): every question goes to the prompt window.
pub struct NoNotifications;

impl Notifier for NoNotifications {
    fn ask(&self, _: &Question, _: OnAnswer) -> Result<(), String> {
        Err("Notifications are not implemented on macOS yet.".into())
    }

    fn notify(&self, _: &Notice, _: OnAction) -> Result<(), String> {
        Err("Notifications are not implemented on macOS yet.".into())
    }
}

/// Placeholder until the real checks (milestone 6): asks by opening a stream, reports Unknown.
pub struct MacPermissions;

impl Permissions for MacPermissions {
    fn status(&self, _: Permission) -> Access {
        Access::Unknown
    }

    fn request(&self, permission: Permission) -> Result<(), String> {
        match permission {
            Permission::Microphone => probe_microphone(),
            Permission::SystemAudio => probe_system_audio(),
            _ => Err("Not implemented on macOS yet.".into()),
        }
    }

    fn open_settings(&self, permission: Permission) -> Result<(), String> {
        let url = match permission {
            Permission::Microphone => "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
            Permission::Notifications => "x-apple.systempreferences:com.apple.Notifications-Settings.extension",
            Permission::Calendar => "x-apple.systempreferences:com.apple.preference.security?Privacy_Calendars",
            Permission::SystemAudio => "x-apple.systempreferences:com.apple.preference.security?Privacy_AudioCapture",
        };
        std::process::Command::new("open").arg(url).spawn().map(|_| ()).map_err(|e| e.to_string())
    }
}

/// Placeholder until SMAppService (milestone 6).
pub struct NoLoginItem;

impl LoginItem for NoLoginItem {
    fn is_enabled(&self) -> bool {
        false
    }

    fn set_enabled(&self, _: bool) -> Result<(), String> {
        Err("Start at login is not implemented on macOS yet.".into())
    }
}
