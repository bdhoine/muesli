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
    [dir.join("Muesli.mcpb"), dir.join("../Resources/Muesli.mcpb"), dir.join("resources/Muesli.mcpb")]
        .into_iter()
        .find(|p| p.exists())
}

pub fn extension_state() -> ExtensionState {
    ClaudeDesktopExtension::state(VERSION, &ClaudeDesktopConfig::default_directory())
}

/// Applies the MCP preferences to Claude Desktop's config file and records a status line.
pub fn apply(app: &App, prefs: &Preferences) {
    let Some(helper) = helper_path() else {
        *app.claude_status.lock().unwrap() = "muesli-mcp is missing next to the app.".into();
        return;
    };
    let dir = ClaudeDesktopConfig::default_directory();
    *app.claude_status.lock().unwrap() =
        ClaudeDesktopConfig::sync(prefs, &helper, extension_state().is_installed(), &dir);
}
