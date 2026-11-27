//! The title bar of Muesli's windows: in the window's own colour (milk, or dark roast in dark
//! mode), with Windows' own minimise, maximise and close buttons, so the top of a window matches
//! the macOS design. Windows 11 only; Windows 10 ignores these attributes and keeps its default.

use std::ffi::c_void;

use windows::Win32::Foundation::{COLORREF, HWND};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR};

/// Milk background, roast text (light), and dark roast background, milk text (dark): `--bg` and
/// `--text` in `ui/style.css`.
const LIGHT: (u32, u32) = (0xf6f1e7, 0x3b2a1e);
const DARK: (u32, u32) = (0x1e1610, 0xf6f1e7);

/// `0xRRGGBB` to the `0x00BBGGRR` Windows wants.
fn colorref(rgb: u32) -> COLORREF {
    COLORREF(((rgb & 0xff) << 16) | (rgb & 0xff00) | ((rgb >> 16) & 0xff))
}

/// Colours the title bar of the window `hwnd` (the raw handle) for the light or dark theme.
pub fn tint_title_bar(hwnd: *mut c_void, dark: bool) {
    let (background, text) = if dark { DARK } else { LIGHT };
    let hwnd = HWND(hwnd);
    for (attribute, rgb) in [(DWMWA_CAPTION_COLOR, background), (DWMWA_TEXT_COLOR, text)] {
        let value = colorref(rgb);
        // SAFETY: `value` lives for the call and its size is passed along; a failure (Windows 10,
        // a closed window) leaves the default title bar.
        let _ = unsafe { DwmSetWindowAttribute(hwnd, attribute, (&raw const value).cast(), size_of::<COLORREF>() as u32) };
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn colours_are_turned_around_for_windows() {
        assert_eq!(super::colorref(0x112233).0, 0x332211);
    }
}
