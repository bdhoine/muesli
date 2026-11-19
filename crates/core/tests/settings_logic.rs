//! Rules behind the Settings window. The macOS app has the same tests (SettingsLogicTests.swift).
mod common;

use common::Env;
use muesli_core::assistants::{ClaudeDesktopExtension, ExtensionState};

#[test]
fn claude_counts_as_missing_when_the_app_is_not_installed() {
    let env = Env::new();
    let claude = env.dir.path().join("Claude");
    std::fs::create_dir_all(&claude).unwrap(); // leftover data folder
    assert_eq!(ClaudeDesktopExtension::state_with("1.0.0", &claude, Some(false)), ExtensionState::ClaudeMissing);
    assert_eq!(ClaudeDesktopExtension::state_with("1.0.0", &claude, Some(true)), ExtensionState::NotInstalled);
    assert_eq!(ClaudeDesktopExtension::state_with("1.0.0", &claude, None), ExtensionState::NotInstalled);
    assert_eq!(ClaudeDesktopExtension::state_with("1.0.0", &env.dir.path().join("none"), Some(true)), ExtensionState::NotInstalled);
}
