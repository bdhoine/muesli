//! SF Symbols beside the items of the menu bar menu, as macOS's own menus have them.
//!
//! Tauri's menus (muda) can only show colour images, which stay black in dark mode, and the tray
//! hands macOS its menu only while it is open. So the symbols are added when a menu starts
//! tracking (`NSMenuDidBeginTrackingNotification`), matched by item title. Symbols are template
//! images: macOS tints them for light, dark and the highlighted row.

use std::collections::HashMap;
use std::ptr::NonNull;
use std::sync::{Mutex, Once};

use block2::RcBlock;
use objc2_app_kit::{NSImage, NSMenu, NSMenuDidBeginTrackingNotification};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSString};

/// Item title to SF Symbol name, for the menu shown now.
static SYMBOLS: Mutex<Option<HashMap<String, &'static str>>> = Mutex::new(None);

/// Sets the symbols for the menu's items (title to symbol name) and starts watching menus.
pub fn set(symbols: HashMap<String, &'static str>) {
    *SYMBOLS.lock().unwrap() = Some(symbols);
    watch();
}

fn watch() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let block = RcBlock::new(|note: NonNull<NSNotification>| {
            // SAFETY: AppKit passes a valid notification.
            let note = unsafe { note.as_ref() };
            if let Some(menu) = note.object().and_then(|o| o.downcast::<NSMenu>().ok()) {
                decorate(&menu);
            }
        });
        let center = NSNotificationCenter::defaultCenter();
        // SAFETY: the block lives as long as the observer; both are kept for the app's lifetime.
        let observer =
            unsafe { center.addObserverForName_object_queue_usingBlock(Some(NSMenuDidBeginTrackingNotification), None, None, &block) };
        std::mem::forget(observer);
        std::mem::forget(block);
    });
}

fn decorate(menu: &NSMenu) {
    let Ok(guard) = SYMBOLS.lock() else { return };
    let Some(symbols) = guard.as_ref() else { return };
    for item in menu.itemArray().iter() {
        let Some(name) = symbols.get(&item.title().to_string()) else { continue };
        if item.image().is_some() {
            continue;
        }
        if let Some(image) = NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(name), None) {
            item.setImage(Some(&image));
        }
    }
}
