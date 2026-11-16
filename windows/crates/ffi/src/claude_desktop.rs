use std::path::Path;

use muesli_core::assistants::{self, ClaudeDesktopConfig, ClaudeDesktopExtension, ExtensionState};
use muesli_core::Preferences;


#[uniffi::remote(Enum)]
pub enum ExtensionState {
    ClaudeMissing,
    NotInstalled,
    Installed { version: String },
    Outdated { installed: String, bundled: String },
}

/// `~/Library/Application Support/Claude` on macOS.
#[uniffi::export]
pub fn claude_default_directory() -> String {
    ClaudeDesktopConfig::default_directory().to_string_lossy().into_owned()
}

/// Adds the Muesli entry to Claude Desktop's config; returns a status line.
#[uniffi::export]
pub fn claude_config_register(helper: String, directory: String) -> String {
    ClaudeDesktopConfig::register(Path::new(&helper), Path::new(&directory))
}

#[uniffi::export]
pub fn claude_config_unregister(directory: String) -> String {
    ClaudeDesktopConfig::unregister(Path::new(&directory))
}

#[uniffi::export]
pub fn claude_config_wanted(mcp_enabled: bool, config_entry: bool, extension_installed: bool) -> bool {
    ClaudeDesktopConfig::wanted(mcp_enabled, config_entry, extension_installed)
}

#[uniffi::export]
pub fn claude_config_sync(prefs: Preferences, helper: String, extension_installed: bool, directory: String) -> String {
    ClaudeDesktopConfig::sync(&prefs, Path::new(&helper), extension_installed, Path::new(&directory))
}

/// `app_installed`: whether the Claude app itself is installed, when the caller can tell.
#[uniffi::export]
pub fn claude_extension_state(bundled_version: String, claude: String, app_installed: Option<bool>) -> ExtensionState {
    ClaudeDesktopExtension::state_with(&bundled_version, Path::new(&claude), app_installed)
}

#[uniffi::export]
pub fn claude_extension_installed_version(claude: String) -> Option<String> {
    ClaudeDesktopExtension::installed_version(Path::new(&claude))
}

#[uniffi::export]
pub fn claude_extension_is_older(a: String, b: String) -> bool {
    assistants::is_older(&a, &b)
}
