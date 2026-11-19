//! Meeting detection on Windows. Another app is "using the microphone" when the default capture
//! device has an active audio session that belongs to a different process. Windows has no system
//! calendar, so there are no calendar meetings.

use std::sync::Mutex;

use chrono::{DateTime, Utc};
use muesli_core::ports::{DetectionSample, MeetingDetector};
use muesli_platform_common::teams::ProcessScanner;

use windows::core::Interface;
use windows::Win32::Media::Audio::{
    eCapture, eConsole, AudioSessionStateActive, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, MMDeviceEnumerator,
};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};

/// The `MeetingDetector` adapter.
pub struct WindowsDetector {
    sessions: Mutex<Sessions>,
    processes: Mutex<ProcessScanner>,
}

impl WindowsDetector {
    pub fn new() -> Self {
        Self { sessions: Mutex::new(Sessions::new()), processes: Mutex::new(ProcessScanner::new()) }
    }
}

impl Default for WindowsDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl MeetingDetector for WindowsDetector {
    fn sample(&self, _now: DateTime<Utc>) -> DetectionSample {
        DetectionSample {
            calendar: None,
            mic_busy: self.sessions.lock().unwrap().mic_busy(),
            teams_running: self.processes.lock().unwrap().teams_running(),
        }
    }
}

struct Sessions {
    own_pid: u32,
}

impl Sessions {
    fn new() -> Self {
        Self { own_pid: std::process::id() }
    }

    fn mic_busy(&mut self) -> Option<bool> {
        // The detector is asked from whichever thread runs detection, so COM is set up here.
        // Already initialised on this thread is fine (S_FALSE / RPC_E_CHANGED_MODE are ignored).
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
            self.query().ok()
        }
    }

    unsafe fn query(&self) -> windows::core::Result<bool> {
        let enumerator: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let device = enumerator.GetDefaultAudioEndpoint(eCapture, eConsole)?;
        let manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;
        let sessions = manager.GetSessionEnumerator()?;
        for i in 0..sessions.GetCount()? {
            let control = sessions.GetSession(i)?;
            if control.GetState()? != AudioSessionStateActive {
                continue;
            }
            let control2: IAudioSessionControl2 = control.cast()?;
            let pid = control2.GetProcessId()?;
            if pid != 0 && pid != self.own_pid {
                return Ok(true);
            }
        }
        Ok(false)
    }
}
