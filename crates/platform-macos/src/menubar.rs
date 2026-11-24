//! Whether the menu bar is dark, as the status item's own button sees it. That is not always the
//! system's light or dark mode: with a dark wallpaper the menu bar can be dark in light mode.
//! Template images follow it by themselves; a coloured one (the tray icon's red dot) cannot, so
//! the shell picks a white or a black glyph from this.

use objc2_app_kit::{NSAppearanceCustomization, NSStatusItem};
use objc2_foundation::MainThreadMarker;

/// `Some(true)` when the menu bar is dark. `None` when that cannot be told (not on the main
/// thread, the item has no button yet, or its menu is open: the highlighted button does not show
/// the bar's own appearance).
pub fn is_dark(item: &NSStatusItem) -> Option<bool> {
    let mtm = MainThreadMarker::new()?;
    let button = item.button(mtm)?;
    if button.isHighlighted() {
        return None;
    }
    // "NSAppearanceNameDarkAqua", or "NSAppearanceNameVibrantDark" on older systems.
    Some(button.effectiveAppearance().name().to_string().contains("Dark"))
}
