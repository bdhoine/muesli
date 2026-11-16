//! Windows: another app is "using the microphone" when the default capture device has an active
//! audio session that belongs to a different process.

use windows::Win32::Media::Audio::{
    eCapture, eConsole, AudioSessionStateActive, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator,
    MMDeviceEnumerator,
};
use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};
use windows::core::Interface;

pub struct Windows {
    own_pid: u32,
}

impl Windows {
    pub fn new() -> Self {
        // Already initialised on this thread is fine (S_FALSE / RPC_E_CHANGED_MODE are ignored).
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
        Self { own_pid: std::process::id() }
    }

    pub fn mic_busy(&mut self) -> Option<bool> {
        unsafe { self.query().ok() }
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
