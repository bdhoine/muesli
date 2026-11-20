//! Whether the taskbar is light or dark, for picking the tray icon.

const PERSONALIZE: &str = r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize";

/// True when the taskbar uses the light theme (`SystemUsesLightTheme` = 1). Windows 11 defaults to
/// a dark taskbar, so a missing value counts as dark.
pub fn taskbar_is_light() -> bool {
    windows_registry::CURRENT_USER.open(PERSONALIZE).and_then(|k| k.get_u32("SystemUsesLightTheme")).is_ok_and(|v| v == 1)
}
