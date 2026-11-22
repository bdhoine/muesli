//! How Muesli was started. macOS tells an app that was started by the login item through the
//! "open application" Apple event, which carries the property `lgit` (`keyAELaunchedAsLogInItem`);
//! a start from Applications, Spotlight or `open` does not. The event is only there while the app
//! finishes launching, so ask during launch (Tauri's `setup` runs then).

use objc2::msg_send;
use objc2::rc::Retained;
use objc2_foundation::{NSAppleEventDescriptor, NSAppleEventManager};

/// `kAEOpenApplication`, "oapp".
const OPEN_APPLICATION: u32 = u32::from_be_bytes(*b"oapp");
/// `keyAEPropData`, "prdt": where the launch event keeps its properties.
const PROPERTY_DATA: u32 = u32::from_be_bytes(*b"prdt");
/// `keyAELaunchedAsLogInItem`, "lgit".
const LAUNCHED_AS_LOGIN_ITEM: u32 = u32::from_be_bytes(*b"lgit");

/// Whether macOS started Muesli because it is a login item, rather than a person.
pub fn launched_at_login() -> bool {
    let manager = NSAppleEventManager::sharedAppleEventManager();
    let Some(event) = manager.currentAppleEvent() else { return false };
    // objc2-foundation hides these two methods behind a feature that pulls in another crate for
    // two integer typedefs, so they are called directly.
    let id: u32 = unsafe { msg_send![&*event, eventID] };
    if id != OPEN_APPLICATION {
        return false;
    }
    let property: Option<Retained<NSAppleEventDescriptor>> = unsafe { msg_send![&*event, paramDescriptorForKeyword: PROPERTY_DATA] };
    property.is_some_and(|d| d.enumCodeValue() == LAUNCHED_AS_LOGIN_ITEM)
}

#[cfg(test)]
mod tests {
    #[test]
    fn outside_a_launch_there_is_no_login_start() {
        // No Apple event is being handled in a test, so a person is assumed.
        assert!(!super::launched_at_login());
    }
}
