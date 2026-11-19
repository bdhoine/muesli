//! Connects Muesli to Claude Desktop according to Preferences. The app never opens a socket:
//! assistants start the bundled `muesli-mcp` themselves and talk to it over stdio.

use std::path::PathBuf;

use muesli_core::assistants::{ClaudeDesktopConfig, ClaudeDesktopExtension, ExtensionState};
use muesli_core::{Preferences, VERSION};

use crate::state::App;

/// The `muesli-mcp` binary next to the app.
pub fn helper_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let name = if cfg!(windows) { "muesli-mcp.exe" } else { "muesli-mcp" };
    let candidate = exe.parent()?.join(name);
    candidate.exists().then_some(candidate)
}

/// The Claude Desktop extension bundled with the app, if any.
pub fn extension_path() -> Option<PathBuf> {
    let dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    [dir.join("Muesli.mcpb"), dir.join("../Resources/Muesli.mcpb"), dir.join("resources/Muesli.mcpb")].into_iter().find(|p| p.exists())
}

/// Whether the Claude desktop app is installed. On Windows the installer's folder is checked,
/// and Claude's data folder as a fallback, because the install location differs between builds.
pub fn claude_installed() -> bool {
    let data = ClaudeDesktopConfig::default_directory();
    if cfg!(target_os = "macos") {
        let home_apps = dirs::home_dir().map(|h| h.join("Applications/Claude.app"));
        std::path::Path::new("/Applications/Claude.app").exists() || home_apps.is_some_and(|p| p.exists())
    } else {
        let exe = dirs::data_local_dir().map(|d| d.join("AnthropicClaude").join("claude.exe"));
        exe.is_some_and(|p| p.exists()) || data.exists()
    }
}

pub fn extension_state() -> ExtensionState {
    ClaudeDesktopExtension::state_with(VERSION, &ClaudeDesktopConfig::default_directory(), Some(claude_installed()))
}

/// Applies the MCP preferences to Claude Desktop's config file and records a status line.
pub fn apply(app: &App, prefs: &Preferences) {
    let Some(helper) = helper_path() else {
        *app.claude_status.lock().unwrap() = "muesli-mcp is missing next to the app.".into();
        return;
    };
    let _ = muesli_core::assistants::record_helper_path(&app.paths(), &helper); // the Claude plugin finds muesli-mcp through this
    let dir = ClaudeDesktopConfig::default_directory();
    *app.claude_status.lock().unwrap() = ClaudeDesktopConfig::sync(prefs, &helper, extension_state().is_installed(), &dir);
}
