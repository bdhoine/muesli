//! Ports: everything Muesli needs from the operating system, as traits.
//!
//! The core and the engine only talk to the OS through these. Each platform crate implements them
//! (`platform-macos`, `platform-windows`), `platform-fake` implements all of them for tests and the
//! simulator, and `port-contracts` holds one test suite per port that every adapter must pass.
//!
//! Adapters are shared as `Arc<dyn Port>`; the [`Platform`] bundle carries one of each. The
//! [`Updater`] is the exception: the shell provides it (the same adapter on both platforms) and
//! hands it to the engine with its options, and builds without updates (debug builds) have none.

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
    /// What a click on the notification itself (not on a button) does.
    pub click: Click,
}

/// What a click on a question's notification itself does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Click {
    /// Answers yes, as the accept button does.
    Accept,
    /// Only brings Muesli forward (`on_open`); the question stays open.
    Open,
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
    ///
    /// A click on a button calls `on_answer`; a click on the notification itself does what
    /// `question.click` says: `on_answer(true)`, or `on_open` with the question still open.
    fn ask(&self, question: &Question, on_answer: OnAnswer, on_open: OnAction) -> Result<(), String>;
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
        /// A second, smaller line (macOS) or a suffix (Windows): "Today 14:30 · 32 min".
        detail: Option<String>,
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
    /// When the recording started, so the menu can show how long it has run when it opens.
    pub transcribing_since: Option<DateTime<Utc>>,
    /// A small Pumpkin seed dot on the icon: an update is ready to install. The red transcribing
    /// dot wins when both are set.
    pub update_ready: bool,
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
// Updates

/// A stable release of Muesli that is newer than the running app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    /// "0.2.0", without the `v`.
    pub version: String,
    /// The release notes, if the release has any.
    pub notes: Option<String>,
}

/// Why an update could not be checked, downloaded or installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateError {
    /// No connection, a timeout, or the server answered with something unusable.
    Network(String),
    /// The download does not match the signature made with Muesli's key: it was discarded.
    BadSignature,
    /// Nothing was downloaded yet, or installing failed.
    Install(String),
}

impl std::fmt::Display for UpdateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Network(why) | Self::Install(why) => f.write_str(why),
            Self::BadSignature => f.write_str("The download did not match Muesli's signature, so it was discarded."),
        }
    }
}

/// Bytes received so far, and the total when the server says it.
pub type OnProgress = Box<dyn Fn(u64, Option<u64>) + Send + Sync>;

/// Muesli's own updates, from the GitHub releases of the project. Every call blocks; the engine
/// calls them on a thread of its own. The adapter is the same on macOS and Windows
/// (`tauri-plugin-updater`, in `apps/desktop`); the fake is in `platform-fake`.
pub trait Updater: Send + Sync {
    /// The running app's version, e.g. "0.1.0".
    fn current_version(&self) -> String;
    /// Looks for a newer stable release. `Ok(None)`: up to date, which includes no release at all
    /// and a release without update information. Never offers an older or equal version.
    fn check(&self) -> Result<Option<Release>, UpdateError>;
    /// Downloads the release the last `check` found and verifies its signature against the
    /// public key built into the app. A download that does not match is discarded
    /// (`UpdateError::BadSignature`) and can never be installed.
    fn download(&self, release: &Release, on_progress: OnProgress) -> Result<(), UpdateError>;
    /// Installs the downloaded update and starts the new version. Returns only when that failed.
    fn install_and_restart(&self) -> Result<(), UpdateError>;
    /// Installs the downloaded update without starting it again: Muesli is quitting.
    fn install_on_quit(&self) -> Result<(), UpdateError>;
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
