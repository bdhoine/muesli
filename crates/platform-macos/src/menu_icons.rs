//! SF Symbols and second lines on the items of the menu bar menu, as macOS's own menus have them.
//!
//! Tauri's menus (muda) can only show colour images, which stay black in dark mode, and have no
//! subtitles; the tray also hands macOS its menu only while it is open. So both are added when a
//! menu starts tracking (`NSMenuDidBeginTrackingNotification`), matched by item title. Symbols
//! are template images: macOS tints them for light, dark and the highlighted row. The item with a
//! start time gets how long the recording has run, counted when the menu opens.

use std::collections::HashMap;
use std::ptr::NonNull;
use std::sync::{Mutex, Once};

use block2::RcBlock;
use chrono::{DateTime, Utc};
use muesli_core::store::elapsed;
use objc2_app_kit::{NSImage, NSMenu, NSMenuDidBeginTrackingNotification};
use objc2_foundation::{NSNotification, NSNotificationCenter, NSString};

/// What one menu item gets, found by its title.
#[derive(Debug, Clone, Default)]
pub struct Extras {
    pub symbol: Option<&'static str>,
    pub subtitle: Option<String>,
    /// Adds " · 12:07", the time since then, to the subtitle.
    pub since: Option<DateTime<Utc>>,
}

/// Item title to its extras, for the menu shown now.
static ITEMS: Mutex<Option<HashMap<String, Extras>>> = Mutex::new(None);

/// Sets the extras for the menu's items and starts watching menus.
pub fn set(items: HashMap<String, Extras>) {
    *ITEMS.lock().unwrap() = Some(items);
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
    let Ok(guard) = ITEMS.lock() else { return };
    let Some(items) = guard.as_ref() else { return };
    for item in menu.itemArray().iter() {
        let Some(extras) = items.get(&item.title().to_string()) else { continue };
        if let Some(name) = extras.symbol {
            if item.image().is_none() {
                if let Some(image) = NSImage::imageWithSystemSymbolName_accessibilityDescription(&NSString::from_str(name), None) {
                    item.setImage(Some(&image));
                }
            }
        }
        let running = extras.since.map(|t| elapsed((Utc::now() - t).num_seconds()));
        let subtitle = match (&extras.subtitle, running) {
            (Some(text), Some(clock)) => Some(format!("{text} · {clock}")),
            (Some(text), None) => Some(text.clone()),
            (None, Some(clock)) => Some(clock),
            (None, None) => None,
        };
        if let Some(subtitle) = subtitle {
            item.setSubtitle(Some(&NSString::from_str(&subtitle)));
        }
    }
}
