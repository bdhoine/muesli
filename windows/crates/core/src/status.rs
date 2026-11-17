use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::json::{self, iso_date, iso_date_opt, to_pretty_sorted};
use crate::paths::Paths;

/// How long a `state.json` counts as fresh.
pub const FRESH_SECONDS: i64 = 30;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Detected {
    pub title: String,
    pub source: String,
    #[serde(with = "iso_date")]
    pub since: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Current {
    pub id: String,
    pub title: String,
    pub source: String,
    #[serde(with = "iso_date")]
    pub started_at: DateTime<Utc>,
    pub segment_count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastReady {
    pub id: String,
    pub title: String,
    #[serde(default, with = "iso_date_opt", skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<DateTime<Utc>>,
}

/// An audio file that is being transcribed (from the drop folder or picked by hand).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileJob {
    pub name: String,
    /// 0 to 1, or `None` while the length is not known yet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress: Option<f64>,
    /// Files queued behind this one.
    pub waiting: u32,
}

/// `state.json`: what the app is doing right now. Rewritten on every change and every 10 s.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    pub schema_version: u32,
    #[serde(with = "iso_date")]
    pub updated_at: DateTime<Utc>,
    pub pid: u32,
    /// idle | preparing | recording | finalizing | transcribingFile | error | permissionMissing
    pub app_state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detected: Option<Detected>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<Current>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_ready: Option<LastReady>,
    /// The audio file being transcribed, while `appState` is `transcribingFile`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_job: Option<FileJob>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcripts_folder: Option<String>,
}

impl AppStatus {
    pub fn new(app_state: impl Into<String>) -> Self {
        Self {
            schema_version: 1,
            updated_at: Utc::now(),
            pid: std::process::id(),
            app_state: app_state.into(),
            detected: None,
            current: None,
            last_ready: None,
            file_job: None,
            error: None,
            transcripts_folder: None,
        }
    }

    pub fn write(&self, paths: &Paths) -> std::io::Result<()> {
        json::write_atomic(&paths.state(), &to_pretty_sorted(self))
    }

    pub fn read(paths: &Paths) -> Option<Self> {
        json::from_str(&std::fs::read_to_string(paths.state()).ok()?)
    }

    pub fn is_fresh(&self, now: DateTime<Utc>) -> bool {
        (now - self.updated_at).num_seconds() < FRESH_SECONDS
    }
}

/// What the app is doing, as far as `state.json` is concerned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Activity {
    /// `preparing` is true while a speech model is being downloaded.
    Idle { preparing: bool },
    Recording,
    Finalizing,
    /// Transcribing an audio file while nothing is being recorded.
    TranscribingFile,
    Error(String),
    PermissionMissing(String),
}

impl Activity {
    /// The `appState` string and the error text that go into `state.json`.
    pub fn app_state(&self) -> (&'static str, Option<&str>) {
        match self {
            Self::Idle { preparing: false } => ("idle", None),
            Self::Idle { preparing: true } => ("preparing", None),
            Self::Recording => ("recording", None),
            Self::Finalizing => ("finalizing", None),
            Self::TranscribingFile => ("transcribingFile", None),
            Self::Error(message) => ("error", Some(message)),
            Self::PermissionMissing(message) => ("permissionMissing", Some(message)),
        }
    }
}

impl AppStatus {
    pub fn for_activity(activity: &Activity) -> Self {
        let (state, error) = activity.app_state();
        let mut out = Self::new(state);
        out.error = error.map(String::from);
        out
    }
}
