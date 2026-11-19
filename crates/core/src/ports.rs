//! Ports: everything Muesli needs from the operating system, as traits.
//!
//! The core and the engine only talk to the OS through these. Each platform crate implements them
//! (`platform-macos`, `platform-windows`), `platform-fake` implements all of them for tests and the
//! simulator, and `port-contracts` holds one test suite per port that every adapter must pass.
//!
//! Adapters are shared as `Arc<dyn Port>`; the [`Platform`] bundle carries one of each.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::models::DetectedMeeting;

// ---------------------------------------------------------------------------------------------
// Clock

/// The time. Everything that waits (the 60 s end-of-call delay, the 5 minute answer timeout,
/// the live transcript refresh, the undo toast) reads it here, so tests can move time by hand.
pub trait Clock: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
}

// ---------------------------------------------------------------------------------------------
// Audio

/// Called with 16 kHz mono f32 samples, in order, from an audio thread.
pub type OnSamples = Box<dyn FnMut(&[f32]) + Send>;
/// Called once when a stream breaks for good (device unplugged, permission revoked).
pub type OnError = Arc<dyn Fn(String) + Send + Sync>;

/// A running capture. Dropping it stops it too; `stop` also hands over the samples still queued.
/// Not `Send`: some platforms tie a stream to the thread that opened it.
pub trait AudioStream {
    fn stop(&mut self);
}

/// The microphone: "You".
pub trait MicCapture: Send + Sync {
    fn start(&self, on_samples: OnSamples, on_error: OnError) -> Result<Box<dyn AudioStream>, String>;
}

/// What the computer plays (the other side of a call): "Others".
pub trait SystemAudioCapture: Send + Sync {
    fn start(&self, on_samples: OnSamples, on_error: OnError) -> Result<Box<dyn AudioStream>, String>;
}

// ---------------------------------------------------------------------------------------------
// Meeting detection

/// One look at the system, taken every few seconds. `DetectionLogic` makes the decisions.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DetectionSample {
    /// The calendar meeting running now, with its event id. `None` where there is no calendar.
    pub calendar: Option<(DetectedMeeting, String)>,
    /// Whether an app other than Muesli is using a microphone; `None` when that cannot be told.
    pub mic_busy: Option<bool>,
    pub teams_running: bool,
}

pub trait MeetingDetector: Send + Sync {
    fn sample(&self, now: DateTime<Utc>) -> DetectionSample;
}

// ---------------------------------------------------------------------------------------------
// Notifications

/// A yes/no question asked as a notification with two buttons.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Question {
    pub title: String,
    pub body: String,
    /// The button that means yes, e.g. "Start transcribing".
    pub accept: String,
    /// The button that means no, e.g. "Not now".
    pub decline: String,
}

/// A notification that only informs, with one optional action button.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notice {
    pub title: String,
    pub body: String,
    /// The button, e.g. "Open"; `None` for none.
    pub action: Option<String>,
}

/// `Some(true)`: accepted, `Some(false)`: declined or dismissed.
pub type OnAnswer = Box<dyn Fn(bool) + Send + Sync>;
pub type OnAction = Box<dyn Fn() + Send + Sync>;

pub trait Notifier: Send + Sync {
    /// Shows the question. `Err` means it cannot be shown (notifications off or not supported);
    /// the engine then asks in Muesli's own prompt window.
    fn ask(&self, question: &Question, on_answer: OnAnswer) -> Result<(), String>;
    /// Shows a notice. `Err` as for `ask`.
    fn notify(&self, notice: &Notice, on_action: OnAction) -> Result<(), String>;
}

// ---------------------------------------------------------------------------------------------
// Permissions

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Permission {
    Microphone,
    SystemAudio,
    Calendar,
    Notifications,
}

impl Permission {
    pub const ALL: [Permission; 4] = [Self::Microphone, Self::SystemAudio, Self::Calendar, Self::Notifications];
}

/// The answer to "may Muesli do this?".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Access {
    Allowed,
    Denied,
    /// Not asked yet: `request` will show the system's question.
    NotAsked,
    /// The system does not say.
    Unknown,
    /// This platform has no such permission (Windows: system audio, calendar).
    NotNeeded,
}

impl Access {
    /// Whether Muesli can go ahead.
    pub fn is_granted(self) -> bool {
        matches!(self, Self::Allowed | Self::NotNeeded)
    }
}

pub trait Permissions: Send + Sync {
    fn status(&self, permission: Permission) -> Access;
    /// Shows the system's question, if it still asks one. Only ever called from the setup
    /// screen or Settings, when the user pressed "Allow…".
    fn request(&self, permission: Permission) -> Result<(), String>;
    /// Opens the system settings page where the permission is switched.
    fn open_settings(&self, permission: Permission) -> Result<(), String>;
}

// ---------------------------------------------------------------------------------------------
// Tray (menu bar on macOS, notification area on Windows)

/// One line of the tray menu. The engine builds the menu; the adapter only draws it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TrayItem {
    /// A clickable line; the id comes back through the tray's callback.
    Action {
        id: String,
        label: String,
        enabled: bool,
    },
    /// A line of text that cannot be clicked (a header, progress).
    Label {
        label: String,
    },
    Separator,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrayMenu {
    pub items: Vec<TrayItem>,
    /// A red dot on the icon while transcribing.
    pub transcribing: bool,
    pub tooltip: String,
}

pub trait Tray: Send + Sync {
    /// Draws the menu. Called whenever the menu changes; the adapter may skip identical menus.
    fn show(&self, menu: &TrayMenu);
}

// ---------------------------------------------------------------------------------------------
// Trash

pub trait Trash: Send + Sync {
    /// Moves a file to the Trash / Recycle Bin.
    fn trash(&self, path: &Path) -> Result<(), String>;
    /// What the user calls it: "Trash" (macOS), "Recycle Bin" (Windows).
    fn name(&self) -> &'static str;
}

// ---------------------------------------------------------------------------------------------
// Folders

/// Where the platform keeps things. `Paths::from_platform` turns this into Muesli's folders.
pub trait PlatformPaths: Send + Sync {
    /// The per-user app data folder that holds Muesli's own folder (macOS: Application Support,
    /// Windows: roaming AppData).
    fn data_dir(&self) -> PathBuf;
    /// Where large files belong (Windows: local AppData). Same as `data_dir` where there is no difference.
    fn local_data_dir(&self) -> PathBuf;
    /// The user's Documents folder.
    fn documents_dir(&self) -> PathBuf;
    fn home_dir(&self) -> PathBuf;
    /// An environment variable (`MUESLI_SUPPORT_DIR`, `TRANSCRIPT_DIR`).
    fn env(&self, name: &str) -> Option<String>;
}

// ---------------------------------------------------------------------------------------------
// Start at login

pub trait LoginItem: Send + Sync {
    fn is_enabled(&self) -> bool;
    fn set_enabled(&self, on: bool) -> Result<(), String>;
}

// ---------------------------------------------------------------------------------------------
// The words a platform uses

/// The platform's own words, so the UI never has to decide them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformWords {
    /// "mac" or "windows": picks the UI skin.
    pub skin: &'static str,
    /// "Mac" / "PC"
    pub computer: &'static str,
    /// "Trash" / "Recycle Bin"
    pub trash: &'static str,
    /// "Finder" / "Explorer"
    pub file_manager: &'static str,
    /// "menu bar" / "system tray"
    pub tray: &'static str,
    /// "Start at login" / "Start when I sign in"
    pub start_at_login: &'static str,
}

impl PlatformWords {
    pub const MAC: Self =
        Self { skin: "mac", computer: "Mac", trash: "Trash", file_manager: "Finder", tray: "menu bar", start_at_login: "Start at login" };
    pub const WINDOWS: Self = Self {
        skin: "windows",
        computer: "PC",
        trash: "Recycle Bin",
        file_manager: "Explorer",
        tray: "system tray",
        start_at_login: "Start when I sign in",
    };
}

// ---------------------------------------------------------------------------------------------
// The bundle

/// One adapter per port: what a platform crate hands to the engine.
#[derive(Clone)]
pub struct Platform {
    pub clock: Arc<dyn Clock>,
    pub mic: Arc<dyn MicCapture>,
    pub system_audio: Arc<dyn SystemAudioCapture>,
    pub detector: Arc<dyn MeetingDetector>,
    pub notifier: Arc<dyn Notifier>,
    pub permissions: Arc<dyn Permissions>,
    pub tray: Arc<dyn Tray>,
    pub trash: Arc<dyn Trash>,
    pub paths: Arc<dyn PlatformPaths>,
    pub login_item: Arc<dyn LoginItem>,
    pub words: PlatformWords,
}
