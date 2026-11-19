//! What Windows allows Muesli. Only the microphone has a switch that matters: recording what the
//! computer plays (WASAPI loopback) needs no permission, and Windows has no system calendar.

use muesli_core::ports::{Access, Permission, Permissions};

const CONSENT: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";

/// Windows keeps the microphone switches as "Allow" / "Deny" strings: one for the whole PC and
/// one for desktop apps. Both must allow.
pub fn microphone_from_consent(global: Option<&str>, desktop_apps: Option<&str>) -> Access {
    let denied = |v: Option<&str>| v.is_some_and(|v| v.eq_ignore_ascii_case("Deny"));
    if denied(global) || denied(desktop_apps) {
        Access::Denied
    } else if global.is_some() || desktop_apps.is_some() {
        Access::Allowed
    } else {
        Access::Unknown
    }
}

fn consent(path: &str) -> Option<String> {
    windows_registry::CURRENT_USER.open(path).ok()?.get_string("Value").ok()
}

/// Settings > Privacy & security > Microphone.
pub fn microphone() -> Access {
    let global = consent(CONSENT);
    let desktop = consent(&format!(r"{CONSENT}\NonPackaged"));
    microphone_from_consent(global.as_deref(), desktop.as_deref())
}

/// Whether Windows will show Muesli's notifications.
pub fn notifications(app_id: &str) -> Access {
    use windows::core::HSTRING;
    use windows::UI::Notifications::{NotificationSetting, ToastNotificationManager};
    let setting = ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(app_id)).and_then(|n| n.Setting());
    match setting {
        Ok(NotificationSetting::Enabled) => Access::Allowed,
        Ok(_) => Access::Denied,
        Err(_) => Access::Unknown,
    }
}

/// Opens a `ms-settings:` page.
fn open_uri(uri: &str) -> Result<(), String> {
    std::process::Command::new("explorer.exe").arg(uri).spawn().map(|_| ()).map_err(|e| e.to_string())
}

/// The `Permissions` adapter.
pub struct WindowsPermissions {
    pub app_id: &'static str,
}

impl Permissions for WindowsPermissions {
    fn status(&self, permission: Permission) -> Access {
        match permission {
            Permission::Microphone => microphone(),
            Permission::Notifications => notifications(self.app_id),
            Permission::SystemAudio | Permission::Calendar => Access::NotNeeded,
        }
    }

    fn request(&self, permission: Permission) -> Result<(), String> {
        match permission {
            // Opening the microphone once makes Windows record Muesli in the privacy list.
            Permission::Microphone => muesli_platform_common::cpal_capture::probe_microphone(),
            _ => Ok(()),
        }
    }

    fn open_settings(&self, permission: Permission) -> Result<(), String> {
        match permission {
            Permission::Notifications => open_uri("ms-settings:notifications"),
            _ => open_uri("ms-settings:privacy-microphone"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn either_switch_can_deny_the_microphone() {
        assert_eq!(microphone_from_consent(Some("Allow"), Some("Allow")), Access::Allowed);
        assert_eq!(microphone_from_consent(Some("Deny"), Some("Allow")), Access::Denied);
        assert_eq!(microphone_from_consent(Some("Allow"), Some("deny")), Access::Denied);
        assert_eq!(microphone_from_consent(Some("Allow"), None), Access::Allowed);
        assert_eq!(microphone_from_consent(None, None), Access::Unknown);
    }
}
