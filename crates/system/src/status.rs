//! What the operating system currently allows Muesli, for the Permissions page in Settings.

/// The answer to "may Muesli do this?".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Allowed,
    Denied,
    /// The system does not say (or this platform has no such switch).
    Unknown,
}

impl Access {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Allowed => "allowed",
            Self::Denied => "denied",
            Self::Unknown => "unknown",
        }
    }
}

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

#[cfg(windows)]
mod imp {
    use super::{microphone_from_consent, Access};

    const CONSENT: &str = r"Software\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\microphone";

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
}

#[cfg(not(windows))]
mod imp {
    use super::Access;

    pub fn microphone() -> Access {
        Access::Unknown
    }

    pub fn notifications(_app_id: &str) -> Access {
        Access::Unknown
    }
}

pub use imp::{microphone, notifications};

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
