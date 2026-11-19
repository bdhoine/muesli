use chrono::{DateTime, Utc};
use muesli_core::status::{self, AppStatus, Current, Detected, FileJob, LastReady};

use crate::paths::PathSet;

pub type StatusDetected = Detected;
pub type StatusCurrent = Current;
pub type StatusLastReady = LastReady;
pub type StatusFileJob = FileJob;

#[uniffi::remote(Record)]
pub struct StatusDetected {
    pub title: String,
    pub source: String,
    pub since: DateTime<Utc>,
}

#[uniffi::remote(Record)]
pub struct StatusCurrent {
    pub id: String,
    pub title: String,
    pub source: String,
    pub started_at: DateTime<Utc>,
    pub segment_count: u32,
}

#[uniffi::remote(Record)]
pub struct StatusLastReady {
    pub id: String,
    pub title: String,
    pub ended_at: Option<DateTime<Utc>>,
}

#[uniffi::remote(Record)]
pub struct StatusFileJob {
    pub name: String,
    pub progress: Option<f64>,
    pub waiting: u32,
}

#[uniffi::remote(Record)]
pub struct AppStatus {
    pub schema_version: u32,
    pub updated_at: DateTime<Utc>,
    pub pid: u32,
    pub app_state: String,
    pub detected: Option<StatusDetected>,
    pub current: Option<StatusCurrent>,
    pub last_ready: Option<StatusLastReady>,
    pub file_job: Option<StatusFileJob>,
    pub error: Option<String>,
    pub transcripts_folder: Option<String>,
}

/// A status stamped with the current time and process id.
#[uniffi::export]
pub fn app_status_new(app_state: String) -> AppStatus {
    AppStatus::new(app_state)
}

#[uniffi::export]
pub fn app_status_write(status: AppStatus, paths: PathSet) {
    let _ = status.write(&paths);
}

#[uniffi::export]
pub fn app_status_read(paths: PathSet) -> Option<AppStatus> {
    AppStatus::read(&paths)
}

#[uniffi::export]
pub fn app_status_is_fresh(status: AppStatus, now: DateTime<Utc>) -> bool {
    status.is_fresh(now)
}

#[uniffi::export]
pub fn app_status_fresh_seconds() -> i64 {
    status::FRESH_SECONDS
}
