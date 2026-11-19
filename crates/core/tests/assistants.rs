#![allow(clippy::field_reassign_with_default)] // tests read better one field at a time

mod common;

use std::path::Path;

use common::Env;
use muesli_core::assistants::*;
use serde_json::Value;

const HELPER: &str = "/Applications/Muesli.app/Contents/MacOS/muesli-mcp";

#[test]
fn terminal_commands_name_the_server_and_helper() {
    assert_eq!(AssistantClient::ClaudeCode.snippet_for(HELPER, false), format!("claude mcp add --scope user muesli -- '{HELPER}'"));
    assert_eq!(AssistantClient::Codex.snippet_for(HELPER, false), format!("codex mcp add muesli -- '{HELPER}'"));
    assert_eq!(AssistantClient::GeminiCli.snippet_for(HELPER, false), format!("gemini mcp add --scope user muesli '{HELPER}'"));
}

#[test]
fn paths_with_spaces_and_quotes_stay_one_shell_word() {
    let odd = "/Users/me/My Apps/Bob's/muesli-mcp";
    assert_eq!(
        AssistantClient::Codex.snippet_for(odd, false),
        r#"codex mcp add muesli -- '/Users/me/My Apps/Bob'\''s/muesli-mcp'"#
    );
    // PowerShell doubles the single quote.
    assert_eq!(
        AssistantClient::Codex.snippet_for(r"C:\Users\Bob's Apps\muesli-mcp.exe", true),
        r"codex mcp add muesli -- 'C:\Users\Bob''s Apps\muesli-mcp.exe'"
    );
}

#[test]
fn vscode_command_carries_valid_json() {
    let command = AssistantClient::VsCode.snippet_for(HELPER, false);
    let json = command.strip_prefix("code --add-mcp '").unwrap().strip_suffix('\'').unwrap();
    let v: Value = serde_json::from_str(json).unwrap();
    assert_eq!(v["name"], "muesli");
    assert_eq!(v["command"], HELPER);
}

#[test]
fn windows_paths_survive_in_json_snippets() {
    let text = AssistantClient::Cursor.snippet_for(r"C:\Program Files\Muesli\muesli-mcp.exe", true);
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["mcpServers"]["muesli"]["command"], r"C:\Program Files\Muesli\muesli-mcp.exe");
}

#[test]
fn config_snippet_is_an_mcp_servers_object() {
    for client in [AssistantClient::Cursor, AssistantClient::Other] {
        let text = client.snippet_for(HELPER, false);
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(v["mcpServers"]["muesli"]["command"], HELPER);
        assert!(!text.contains(r"\/"));
        assert!(!client.is_terminal_command());
    }
}

#[test]
fn every_client_has_copy() {
    for client in AssistantClient::ALL {
        assert!(!client.name().is_empty());
        assert!(!client.instructions().is_empty());
        assert!(!client.snippet(HELPER).is_empty());
    }
}

// MARK: Claude Desktop config

fn claude(env: &Env) -> std::path::PathBuf {
    env.dir.path().join("Claude")
}

fn config(env: &Env) -> Value {
    serde_json::from_str(&std::fs::read_to_string(claude(env).join("claude_desktop_config.json")).unwrap()).unwrap()
}

#[test]
fn register_needs_claude_desktop() {
    let env = Env::new();
    assert!(ClaudeDesktopConfig::register(Path::new(HELPER), &claude(&env)).contains("not found"));
    assert!(!claude(&env).exists()); // does not create Claude's folder
}

#[test]
fn register_creates_the_file_when_missing() {
    let env = Env::new();
    std::fs::create_dir_all(claude(&env)).unwrap();
    assert!(ClaudeDesktopConfig::register(Path::new(HELPER), &claude(&env)).starts_with("Registered"));
    let entry = &config(&env)["mcpServers"]["Muesli"];
    assert_eq!(entry["command"], HELPER);
    assert_eq!(entry["args"].as_array().unwrap().len(), 0);
}

#[test]
fn register_and_unregister_leave_everything_else_alone() {
    let env = Env::new();
    std::fs::create_dir_all(claude(&env)).unwrap();
    let existing = r#"{"globalShortcut":"Alt+Space","mcpServers":{"filesystem":{"command":"npx","args":["-y","server-filesystem","/Users/me"],"env":{"A":"1"}}}}"#;
    std::fs::write(claude(&env).join("claude_desktop_config.json"), existing).unwrap();
    let before = config(&env);

    ClaudeDesktopConfig::register(Path::new(HELPER), &claude(&env));
    let servers = config(&env)["mcpServers"].as_object().unwrap().clone();
    assert_eq!(servers.keys().cloned().collect::<Vec<_>>(), ["Muesli", "filesystem"]);
    assert_eq!(config(&env)["globalShortcut"], "Alt+Space");
    assert_eq!(ClaudeDesktopConfig::register(Path::new(HELPER), &claude(&env)), "Registered with Claude Desktop.");

    assert!(ClaudeDesktopConfig::unregister(&claude(&env)).starts_with("Removed"));
    assert_eq!(config(&env), before);
    assert_eq!(ClaudeDesktopConfig::unregister(&claude(&env)), "Off");
}

#[test]
fn register_updates_a_moved_app() {
    let env = Env::new();
    std::fs::create_dir_all(claude(&env)).unwrap();
    ClaudeDesktopConfig::register(Path::new("/old/muesli-mcp"), &claude(&env));
    assert!(ClaudeDesktopConfig::register(Path::new(HELPER), &claude(&env)).contains("Quit and reopen"));
    assert_eq!(config(&env)["mcpServers"]["Muesli"]["command"], HELPER);
}

#[test]
fn the_old_lowercase_entry_is_replaced_and_removed() {
    let env = Env::new();
    std::fs::create_dir_all(claude(&env)).unwrap();
    let file = claude(&env).join("claude_desktop_config.json");
    let legacy = r#"{"mcpServers":{"muesli":{"command":"/old/muesli-mcp","args":[]}}}"#;
    std::fs::write(&file, legacy).unwrap();
    assert!(ClaudeDesktopConfig::register(Path::new(HELPER), &claude(&env)).starts_with("Registered"));
    assert_eq!(config(&env)["mcpServers"].as_object().unwrap().keys().collect::<Vec<_>>(), ["Muesli"]);

    std::fs::write(&file, legacy).unwrap();
    assert!(ClaudeDesktopConfig::unregister(&claude(&env)).starts_with("Removed"));
    assert_eq!(config(&env)["mcpServers"].as_object().unwrap().len(), 0);
}

#[test]
fn an_unreadable_config_is_left_untouched() {
    let env = Env::new();
    std::fs::create_dir_all(claude(&env)).unwrap();
    let file = claude(&env).join("claude_desktop_config.json");
    std::fs::write(&file, "{ not json").unwrap();
    assert!(ClaudeDesktopConfig::register(Path::new(HELPER), &claude(&env)).contains("left it untouched"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "{ not json");
}

// MARK: extension

fn install(env: &Env, manifest: &str, folder: &str) {
    let target = ClaudeDesktopExtension::extensions_directory(&claude(env)).join(folder);
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("manifest.json"), manifest).unwrap();
}

#[test]
fn extension_state_without_claude_desktop() {
    let env = Env::new();
    assert_eq!(ClaudeDesktopExtension::state("1.0.0", &claude(&env)), ExtensionState::ClaudeMissing);
}

#[test]
fn extension_state_before_install() {
    let env = Env::new();
    std::fs::create_dir_all(claude(&env)).unwrap();
    install(&env, r#"{"name":"someone-else","version":"9.9.9"}"#, "other");
    assert_eq!(ClaudeDesktopExtension::state("1.0.0", &claude(&env)), ExtensionState::NotInstalled);
}

#[test]
fn extension_installed_and_outdated() {
    let env = Env::new();
    install(&env, r#"{"name":"muesli","version":"0.2.0"}"#, "local.mcpb.muesli.muesli");
    assert_eq!(ClaudeDesktopExtension::state("0.2.0", &claude(&env)), ExtensionState::Installed { version: "0.2.0".into() });
    assert_eq!(
        ClaudeDesktopExtension::state("0.10.0", &claude(&env)),
        ExtensionState::Outdated { installed: "0.2.0".into(), bundled: "0.10.0".into() }
    );
    assert_eq!(ClaudeDesktopExtension::state("0.1.9", &claude(&env)), ExtensionState::Installed { version: "0.2.0".into() });
}

#[test]
fn version_ordering() {
    assert!(is_older("0.2", "0.2.1"));
    assert!(!is_older("0.2.0", "0.2"));
    assert!(is_older("0.9.0", "0.10.0"));
}

#[test]
fn manifest_has_what_claude_desktop_needs() {
    let text = ClaudeDesktopExtension::manifest_json("1.2.3", "server/muesli-mcp", "icon.png");
    let m: Value = serde_json::from_str(&text).unwrap();
    for key in ["manifest_version", "name", "version", "description", "author", "server"] {
        assert!(m.get(key).is_some(), "{key}");
    }
    assert_eq!(m["name"], "muesli");
    assert_eq!(m["display_name"], "Muesli");
    assert_eq!(m["version"], "1.2.3");
    assert_eq!(m["icon"], "icon.png");
    assert_eq!(m["server"]["type"], "binary");
    assert_eq!(m["server"]["entry_point"], "server/muesli-mcp");
    assert_eq!(m["server"]["mcp_config"]["command"], "${__dirname}/server/muesli-mcp");
    let tools: Vec<&str> = m["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(tools.len(), 7);
    assert!(tools.contains(&"delete_transcript"));
    assert!(!text.contains(r"\/"));
}

#[test]
fn the_config_entry_follows_the_preferences() {
    assert!(ClaudeDesktopConfig::wanted(true, true, false));
    assert!(!ClaudeDesktopConfig::wanted(true, true, true)); // the extension replaces it
    assert!(!ClaudeDesktopConfig::wanted(false, true, false));
    assert!(!ClaudeDesktopConfig::wanted(true, false, false));

    let env = Env::new();
    std::fs::create_dir_all(claude(&env)).unwrap();
    let mut prefs = muesli_core::Preferences::default();
    prefs.mcp_enabled = true;
    prefs.claude_desktop_config_entry = true;
    assert!(ClaudeDesktopConfig::sync(&prefs, Path::new(HELPER), false, &claude(&env)).starts_with("Registered"));
    assert!(ClaudeDesktopConfig::sync(&prefs, Path::new(HELPER), true, &claude(&env)).starts_with("Removed"));
    assert_eq!(ClaudeDesktopConfig::sync(&prefs, Path::new(HELPER), true, &claude(&env)), "Off");
    assert!(ExtensionState::Outdated { installed: "1".into(), bundled: "2".into() }.is_installed());
    assert!(!ExtensionState::NotInstalled.is_installed());
}
