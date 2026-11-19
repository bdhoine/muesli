//! macOS: the Core Audio process list says which processes are running input (macOS 14.2+).

use std::mem::size_of;
use std::ptr::NonNull;

use objc2_core_audio::{
    kAudioHardwarePropertyProcessObjectList, kAudioObjectPropertyElementMain, kAudioObjectPropertyScopeGlobal, kAudioObjectSystemObject,
    kAudioProcessPropertyIsRunningInput, kAudioProcessPropertyPID, AudioObjectGetPropertyData, AudioObjectGetPropertyDataSize,
    AudioObjectID, AudioObjectPropertyAddress,
};

pub struct Mac {
    own_pid: i32,
}

fn address(selector: u32) -> AudioObjectPropertyAddress {
    AudioObjectPropertyAddress { mSelector: selector, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain }
}

impl Mac {
    pub fn new() -> Self {
        Self { own_pid: std::process::id() as i32 }
    }

    pub fn mic_busy(&mut self) -> Option<bool> {
        unsafe {
            let list = address(kAudioHardwarePropertyProcessObjectList);
            let mut size = 0u32;
            if AudioObjectGetPropertyDataSize(
                kAudioObjectSystemObject as AudioObjectID,
                NonNull::from(&list),
                0,
                std::ptr::null(),
                NonNull::from(&mut size),
            ) != 0
            {
                return None; // older macOS: cannot tell
            }
            let count = size as usize / size_of::<AudioObjectID>();
            let mut objects = vec![0 as AudioObjectID; count];
            if AudioObjectGetPropertyData(
                kAudioObjectSystemObject as AudioObjectID,
                NonNull::from(&list),
                0,
                std::ptr::null(),
                NonNull::from(&mut size),
                NonNull::new(objects.as_mut_ptr().cast())?,
            ) != 0
            {
                return None;
            }
            for object in objects {
                let pid: i32 = read(object, kAudioProcessPropertyPID)?;
                let running: u32 = read(object, kAudioProcessPropertyIsRunningInput)?;
                if running != 0 && pid != self.own_pid {
                    return Some(true);
                }
            }
            Some(false)
        }
    }
}

unsafe fn read<T: Default>(object: AudioObjectID, selector: u32) -> Option<T> {
    let addr = address(selector);
    let mut value = T::default();
    let mut size = size_of::<T>() as u32;
    let status = AudioObjectGetPropertyData(
        object,
        NonNull::from(&addr),
        0,
        std::ptr::null(),
        NonNull::from(&mut size),
        NonNull::from(&mut value).cast(),
    );
    (status == 0).then_some(value)
}
