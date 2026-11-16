//! Meeting detection. `muesli_core::DetectionLogic` makes the decisions; this crate samples the
//! system (is another app using the microphone? is Teams running? is there a calendar meeting?)
//! and feeds it every few seconds.

use chrono::{DateTime, Utc};
use muesli_core::{DetectedMeeting, DetectionEvent, DetectionLogic};

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows_impl;

/// What the platform can tell about the system right now.
pub trait Sampler {
    /// Whether an app other than Muesli is using a microphone; `None` when that cannot be told.
    fn mic_busy(&mut self) -> Option<bool>;
    fn teams_running(&mut self) -> bool;
    /// The calendar meeting running now, with its event id. Not available on every platform.
    fn calendar_now(&mut self, _now: DateTime<Utc>) -> Option<(DetectedMeeting, String)> {
        None
    }
}

pub struct Detector<S: Sampler> {
    pub logic: DetectionLogic,
    sampler: S,
}

impl<S: Sampler> Detector<S> {
    pub fn new(sampler: S) -> Self {
        Self { logic: DetectionLogic::default(), sampler }
    }

    /// Call every few seconds. `recording` is whether Muesli is recording right now.
    pub fn tick(&mut self, now: DateTime<Utc>, recording: bool) -> Vec<DetectionEvent> {
        let calendar = self.sampler.calendar_now(now);
        let mic = self.sampler.mic_busy();
        let teams = self.sampler.teams_running();
        self.logic.tick(now, recording, calendar, mic, teams)
    }
}

/// Process names that mean Microsoft Teams is running.
pub fn is_teams_process(name: &str) -> bool {
    let n = name.to_lowercase();
    matches!(n.as_str(), "ms-teams.exe" | "teams.exe" | "msteams" | "microsoft teams" | "teams" | "ms-teams")
}

/// Looks for a running Teams process. Keeps its process table between calls and refreshes only
/// what it needs (names), because it is asked every few seconds.
pub struct ProcessScanner {
    system: Option<sysinfo::System>,
}

impl ProcessScanner {
    pub fn new() -> Self {
        Self { system: sysinfo::System::new().ok() }
    }

    pub fn teams_running(&mut self) -> bool {
        use sysinfo::{ProcessRefreshKind, ProcessesToUpdate};
        let Some(system) = self.system.as_mut() else { return false };
        system.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
        system.processes().values().any(|p| is_teams_process(&p.name().to_string_lossy()))
    }
}

impl Default for ProcessScanner {
    fn default() -> Self {
        Self::new()
    }
}

/// The sampler for this platform.
pub struct SystemSampler {
    processes: ProcessScanner,
    #[cfg(windows)]
    inner: windows_impl::Windows,
    #[cfg(target_os = "macos")]
    inner: macos::Mac,
}

impl SystemSampler {
    pub fn new() -> Self {
        Self {
            processes: ProcessScanner::new(),
            #[cfg(windows)]
            inner: windows_impl::Windows::new(),
            #[cfg(target_os = "macos")]
            inner: macos::Mac::new(),
        }
    }
}

impl Default for SystemSampler {
    fn default() -> Self {
        Self::new()
    }
}

impl Sampler for SystemSampler {
    fn mic_busy(&mut self) -> Option<bool> {
        #[cfg(any(windows, target_os = "macos"))]
        return self.inner.mic_busy();
        #[allow(unreachable_code)]
        None
    }

    fn teams_running(&mut self) -> bool {
        self.processes.teams_running()
    }
}
