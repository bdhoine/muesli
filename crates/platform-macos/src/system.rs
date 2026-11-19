//! Permissions, the Trash and the login item.

use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use block2::RcBlock;
use muesli_core::ports::{Access, LoginItem, Permission, Permissions, Trash};
use muesli_platform_common::cpal_capture::probe_system_audio;
use objc2::runtime::Bool;
use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};
use objc2_foundation::{NSFileManager, NSString, NSURL};
use objc2_service_management::{SMAppService, SMAppServiceStatus};

fn microphone() -> Access {
    let Some(audio) = (unsafe { AVMediaTypeAudio }) else { return Access::Unknown };
    match unsafe { AVCaptureDevice::authorizationStatusForMediaType(audio) } {
        AVAuthorizationStatus::Authorized => Access::Allowed,
        AVAuthorizationStatus::NotDetermined => Access::NotAsked,
        AVAuthorizationStatus::Denied | AVAuthorizationStatus::Restricted => Access::Denied,
        _ => Access::Unknown,
    }
}

fn request_microphone() -> Result<(), String> {
    let audio = unsafe { AVMediaTypeAudio }.ok_or("No audio media type.")?;
    let (tx, rx) = mpsc::channel::<bool>();
    let block = RcBlock::new(move |granted: Bool| {
        let _ = tx.send(granted.as_bool());
    });
    unsafe { AVCaptureDevice::requestAccessForMediaType_completionHandler(audio, &block) };
    match rx.recv_timeout(Duration::from_secs(60)) {
        Ok(true) => Ok(()),
        _ => Err("Microphone access was not allowed.".into()),
    }
}

/// System audio ("System Audio Recording Only") has no public status call. Muesli remembers in
/// this process whether a probe worked; the first probe makes macOS ask.
static SYSTEM_AUDIO_OK: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The `Permissions` adapter.
pub struct MacPermissions;

impl Permissions for MacPermissions {
    fn status(&self, permission: Permission) -> Access {
        match permission {
            Permission::Microphone => microphone(),
            Permission::Calendar => crate::calendar::access(),
            Permission::Notifications => crate::notifier::access(),
            Permission::SystemAudio => {
                if SYSTEM_AUDIO_OK.load(std::sync::atomic::Ordering::Relaxed) {
                    Access::Allowed
                } else {
                    Access::NotAsked
                }
            }
        }
    }

    fn request(&self, permission: Permission) -> Result<(), String> {
        match permission {
            Permission::Microphone => request_microphone(),
            Permission::Calendar => crate::calendar::request(),
            Permission::Notifications => crate::notifier::request(),
            Permission::SystemAudio => {
                // Opening the tap is what makes macOS ask; a refusal surfaces as silence, not an
                // error, so this can only say that the system was asked.
                probe_system_audio()?;
                SYSTEM_AUDIO_OK.store(true, std::sync::atomic::Ordering::Relaxed);
                Ok(())
            }
        }
    }

    fn open_settings(&self, permission: Permission) -> Result<(), String> {
        let url = match permission {
            Permission::Microphone => "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone",
            Permission::Notifications => "x-apple.systempreferences:com.apple.Notifications-Settings.extension",
            Permission::Calendar => "x-apple.systempreferences:com.apple.preference.security?Privacy_Calendars",
            Permission::SystemAudio => "x-apple.systempreferences:com.apple.preference.security?Privacy_AudioCapture",
        };
        std::process::Command::new("/usr/bin/open").arg(url).spawn().map(|_| ()).map_err(|e| e.to_string())
    }
}

/// The `Trash` adapter: NSFileManager, so Finder's "Put Back" works.
pub struct FinderTrash;

impl Trash for FinderTrash {
    fn trash(&self, path: &Path) -> Result<(), String> {
        if !path.exists() {
            return Err(format!("{} does not exist", path.display()));
        }
        let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
        NSFileManager::defaultManager().trashItemAtURL_resultingItemURL_error(&url, None).map_err(|e| e.localizedDescription().to_string())
    }

    fn name(&self) -> &'static str {
        "Trash"
    }
}

/// The `LoginItem` adapter: SMAppService (macOS 13+), listed in System Settings > General >
/// Login Items under Muesli's name.
pub struct AppService;

impl LoginItem for AppService {
    fn is_enabled(&self) -> bool {
        unsafe { SMAppService::mainAppService().status() == SMAppServiceStatus::Enabled }
    }

    fn set_enabled(&self, on: bool) -> Result<(), String> {
        let service = unsafe { SMAppService::mainAppService() };
        let result = if on { unsafe { service.registerAndReturnError() } } else { unsafe { service.unregisterAndReturnError() } };
        result.map_err(|e| e.localizedDescription().to_string())
    }
}
