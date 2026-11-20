//! Connecting AI assistants: copy-paste setup commands, Claude Desktop's config file, and the
//! Claude Desktop extension (`.mcpb`) manifest and install state.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::json::{to_pretty_sorted, write_atomic};

/// The name the server is listed under in assistants.
pub const SERVER_NAME: &str = "muesli";

/// File in the support folder that holds the absolute path of this install's `muesli-mcp`. Both
/// apps rewrite it on every start, so anything that has to start the server (the Claude plugin)
/// finds it wherever Muesli was installed, on macOS and on Windows.
pub const HELPER_POINTER: &str = "mcp-path";

/// Records where `muesli-mcp` is (see `HELPER_POINTER`).
pub fn record_helper_path(paths: &crate::Paths, helper: &Path) -> std::io::Result<()> {
    crate::json::write_atomic(&paths.support.join(HELPER_POINTER), &helper.to_string_lossy())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssistantClient {
    ClaudeCode,
    Codex,
    GeminiCli,
    VsCode,
    Cursor,
    Other,
}

impl AssistantClient {
    pub const ALL: [AssistantClient; 6] = [Self::ClaudeCode, Self::Codex, Self::GeminiCli, Self::VsCode, Self::Cursor, Self::Other];

    pub fn name(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::Codex => "Codex CLI",
            Self::GeminiCli => "Gemini CLI",
            Self::VsCode => "VS Code (Copilot)",
            Self::Cursor => "Cursor",
            Self::Other => "Other MCP client",
        }
    }

    pub fn instructions(self) -> String {
        self.instructions_for(cfg!(windows))
    }

    /// What to do with the snippet, in the words of the platform (Terminal or PowerShell).
    pub fn instructions_for(self, windows: bool) -> String {
        let terminal = if windows { "PowerShell" } else { "Terminal" };
        match self {
            Self::ClaudeCode => format!("Run this in {terminal}. It adds Muesli for your user, so it works in every project."),
            Self::Codex => format!("Run this in {terminal}. Codex saves it in ~/.codex/config.toml."),
            Self::GeminiCli => format!("Run this in {terminal}. It adds Muesli for your user in ~/.gemini/settings.json."),
            Self::VsCode => format!("Run this in {terminal}, then use Muesli from Copilot Chat in agent mode."),
            Self::Cursor => {
                "Add this to ~/.cursor/mcp.json, next to any servers already there. Or use Cursor Settings → MCP → Add new server.".into()
            }
            Self::Other => "Muesli speaks MCP over stdio. Most clients accept this in their configuration file.".into(),
        }
    }

    pub fn is_terminal_command(self) -> bool {
        matches!(self, Self::ClaudeCode | Self::Codex | Self::GeminiCli | Self::VsCode)
    }

    /// The text to copy, for a `muesli-mcp` at `helper`. Quoting follows the current OS.
    pub fn snippet(self, helper: &str) -> String {
        self.snippet_for(helper, cfg!(windows))
    }

    pub fn snippet_for(self, helper: &str, windows: bool) -> String {
        let quote = |s: &str| shell_quote(s, windows);
        match self {
            Self::ClaudeCode => format!("claude mcp add --scope user {SERVER_NAME} -- {}", quote(helper)),
            Self::Codex => format!("codex mcp add {SERVER_NAME} -- {}", quote(helper)),
            Self::GeminiCli => format!("gemini mcp add --scope user {SERVER_NAME} {}", quote(helper)),
            Self::VsCode => {
                let json = json!({ "name": SERVER_NAME, "command": helper });
                let text = serde_json::to_string(&json).unwrap_or_default();
                format!("code --add-mcp {}", quote(&text))
            }
            Self::Cursor | Self::Other => to_pretty_sorted(&json!({ "mcpServers": { SERVER_NAME: { "command": helper } } })),
        }
    }
}

/// Quotes one word for a POSIX shell, or for PowerShell on Windows (single quotes, doubled inside).
pub fn shell_quote(s: &str, windows: bool) -> String {
    if windows {
        format!("'{}'", s.replace('\'', "''"))
    } else {
        format!("'{}'", s.replace('\'', "'\\''"))
    }
}

// MARK: Claude Desktop config file

/// Adds or removes the "Muesli" entry in Claude Desktop's MCP server list.
/// Every other key in that file is left exactly as it was.
pub struct ClaudeDesktopConfig;

impl ClaudeDesktopConfig {
    pub const KEY: &'static str = "Muesli";
    pub const LEGACY_KEYS: [&'static str; 1] = ["muesli"];

    /// macOS: `~/Library/Application Support/Claude`; Windows: `%APPDATA%\Claude`.
    pub fn default_directory(platform: &dyn crate::ports::PlatformPaths) -> PathBuf {
        platform.data_dir().join("Claude")
    }

    fn file(directory: &Path) -> PathBuf {
        directory.join("claude_desktop_config.json")
    }

    /// Returns a status line for the Settings window.
    pub fn register(helper: &Path, directory: &Path) -> String {
        let file = Self::file(directory);
        if !directory.exists() {
            return "Claude Desktop was not found on this computer. Install it, then switch this off and on again.".into();
        }
        let Some(mut root) = read(&file) else {
            return format!("Could not read {}; left it untouched. Add the server by hand.", file_name(&file));
        };
        let command = helper.to_string_lossy().into_owned();
        let servers = root.entry("mcpServers").or_insert_with(|| json!({}));
        let Some(servers) = servers.as_object_mut() else {
            return format!("Could not read {}; left it untouched. Add the server by hand.", file_name(&file));
        };
        let has_legacy = Self::LEGACY_KEYS.iter().any(|k| servers.contains_key(*k));
        if !has_legacy && servers.get(Self::KEY).and_then(|e| e.get("command")).and_then(Value::as_str) == Some(command.as_str()) {
            return "Registered with Claude Desktop.".into();
        }
        for legacy in Self::LEGACY_KEYS {
            servers.remove(legacy);
        }
        servers.insert(Self::KEY.into(), json!({ "command": command, "args": [] }));
        if write(&root, &file) {
            "Registered with Claude Desktop. Quit and reopen Claude Desktop to load it.".into()
        } else {
            format!("Could not write {}.", file_name(&file))
        }
    }

    /// Whether Claude Desktop's config file should carry the Muesli entry: only when assistants
    /// are allowed, the user asked for the config entry, and the extension is not installed
    /// (the extension replaces it).
    pub fn wanted(mcp_enabled: bool, config_entry: bool, extension_installed: bool) -> bool {
        mcp_enabled && config_entry && !extension_installed
    }

    /// Adds or removes the entry to match the preferences. Returns the status line (empty: nothing to say).
    pub fn sync(prefs: &crate::Preferences, helper: &Path, extension_installed: bool, directory: &Path) -> String {
        if Self::wanted(prefs.mcp_enabled, prefs.claude_desktop_config_entry, extension_installed) {
            Self::register(helper, directory)
        } else {
            Self::unregister(directory)
        }
    }

    /// Removes the entry. Returns the status line: empty when there was nothing to remove.
    pub fn unregister(directory: &Path) -> String {
        let file = Self::file(directory);
        if !file.exists() {
            return String::new();
        }
        let Some(mut root) = read(&file) else { return String::new() };
        let Some(servers) = root.get_mut("mcpServers").and_then(Value::as_object_mut) else { return String::new() };
        let names = std::iter::once(Self::KEY).chain(Self::LEGACY_KEYS);
        if !names.clone().any(|n| servers.contains_key(n)) {
            return String::new();
        }
        for name in names {
            servers.remove(name);
        }
        if write(&root, &file) {
            "Removed from Claude Desktop (takes effect when Claude Desktop restarts).".into()
        } else {
            format!("Could not write {}.", file_name(&file))
        }
    }
}

fn file_name(file: &Path) -> String {
    file.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// The file's JSON object; an empty object when the file does not exist yet; `None` when it
/// cannot be parsed.
fn read(file: &Path) -> Option<Map<String, Value>> {
    if !file.exists() {
        return Some(Map::new());
    }
    let text = fs::read_to_string(file).ok()?;
    if text.trim().is_empty() {
        return Some(Map::new());
    }
    match serde_json::from_str::<Value>(&text).ok()? {
        Value::Object(o) => Some(o),
        _ => None,
    }
}

fn write(root: &Map<String, Value>, file: &Path) -> bool {
    write_atomic(file, &to_pretty_sorted(root)).is_ok()
}

// MARK: Claude Desktop extension (.mcpb)

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExtensionState {
    ClaudeMissing,
    NotInstalled,
    Installed { version: String },
    Outdated { installed: String, bundled: String },
}

impl ExtensionState {
    pub fn is_installed(&self) -> bool {
        matches!(self, Self::Installed { .. } | Self::Outdated { .. })
    }
}

pub struct ClaudeDesktopExtension;

impl ClaudeDesktopExtension {
    pub const NAME: &'static str = SERVER_NAME;

    pub fn extensions_directory(claude: &Path) -> PathBuf {
        claude.join("Claude Extensions")
    }

    pub fn state(bundled_version: &str, claude: &Path) -> ExtensionState {
        Self::state_with(bundled_version, claude, None)
    }

    /// `app_installed`: whether the Claude app itself is installed, when the caller can tell.
    /// Without it, Claude counts as present when its data folder exists.
    pub fn state_with(bundled_version: &str, claude: &Path, app_installed: Option<bool>) -> ExtensionState {
        if !app_installed.unwrap_or_else(|| claude.exists()) {
            return ExtensionState::ClaudeMissing;
        }
        match Self::installed_version(claude) {
            None => ExtensionState::NotInstalled,
            Some(installed) if is_older(&installed, bundled_version) => {
                ExtensionState::Outdated { installed, bundled: bundled_version.into() }
            }
            Some(version) => ExtensionState::Installed { version },
        }
    }

    pub fn installed_version(claude: &Path) -> Option<String> {
        for folder in fs::read_dir(Self::extensions_directory(claude)).into_iter().flatten().flatten() {
            let Ok(text) = fs::read_to_string(folder.path().join("manifest.json")) else { continue };
            let Ok(manifest) = serde_json::from_str::<Value>(&text) else { continue };
            if manifest.get("name").and_then(Value::as_str) == Some(Self::NAME) {
                return Some(manifest.get("version").and_then(Value::as_str).unwrap_or("0").to_string());
            }
        }
        None
    }

    /// `binary` is the path of the server inside the bundle, e.g. `server/muesli-mcp`.
    pub fn manifest(version: &str, binary: &str, icon: &str) -> Value {
        let trash = crate::trash_name();
        json!({
            "manifest_version": "0.3",
            "name": Self::NAME,
            "display_name": "Muesli",
            "version": version,
            "description": "Read and manage your Muesli meeting transcripts: recording status, the list of meetings, full transcripts, renaming or deleting them, and the vocabulary of names and terms.",
            "long_description": format!("Muesli is an on-device meeting transcriber that lives in your menu bar or system tray. This extension lets Claude read its status and transcripts, rename meetings, set attendees, rename speakers and delete meetings (deleted files go to the {trash}), and add names and terms to the vocabulary Muesli spells right in future transcripts. Claude cannot start or stop a recording. The Muesli app must be installed, with AI assistant access switched on in Settings."),
            "author": { "name": "Muesli", "url": "https://github.com/bdhoine/muesli" },
            "homepage": "https://github.com/bdhoine/muesli",
            "icon": icon,
            "server": {
                "type": "binary",
                "entry_point": binary,
                "mcp_config": { "command": format!("${{__dirname}}/{binary}"), "args": [], "env": {} },
            },
            "tools": [
                { "name": "get_status", "description": "Whether Muesli is running, detecting a meeting or recording, and the latest finished transcript." },
                { "name": "list_transcripts", "description": "List meetings known to Muesli, newest first." },
                { "name": "get_transcript", "description": "Read one meeting transcript by id." },
                { "name": "rename_transcript", "description": "Change a finished meeting's title." },
                { "name": "set_attendees", "description": "Replace a finished meeting's attendee list." },
                { "name": "rename_speaker", "description": "Rename a speaker label, e.g. Others to a person's name." },
                { "name": "get_vocabulary", "description": "The names and terms Muesli spells right." },
                { "name": "update_vocabulary", "description": "Add or remove names and terms for future transcripts." },
                { "name": "delete_transcript", "description": format!("Delete a finished meeting; its files go to the {trash}.") },
            ],
            "compatibility": { "platforms": [if cfg!(windows) { "win32" } else { "darwin" }] },
        })
    }

    pub fn manifest_json(version: &str, binary: &str, icon: &str) -> String {
        to_pretty_sorted(&Self::manifest(version, binary, icon))
    }
}

pub fn is_older(a: &str, b: &str) -> bool {
    let parse = |s: &str| s.split('.').map(|p| p.parse::<u64>().unwrap_or(0)).collect::<Vec<_>>();
    let (x, y) = (parse(a), parse(b));
    for i in 0..x.len().max(y.len()) {
        let (l, r) = (x.get(i).copied().unwrap_or(0), y.get(i).copied().unwrap_or(0));
        if l != r {
            return l < r;
        }
    }
    false
}
