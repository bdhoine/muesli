# Connecting Muesli to an AI assistant

Muesli ships a small MCP server, `muesli-mcp`, inside the app bundle. An assistant such as Claude uses it to ask whether Muesli is recording, to list transcripts and read them, and to manage finished meetings: rename them, set attendees, rename speakers ("Others" → "Ann") and delete them. Deleted files go to the Trash (macOS) or the Recycle Bin (Windows).

This works the same on macOS and Windows; only the paths differ. Everything stays on your computer. The assistant starts `muesli-mcp` itself and talks to it over stdio. The app never opens a socket and nothing is exposed to the network. A transcript the assistant reads is sent to its provider, for example to Anthropic for Claude. Assistants cannot start or stop a recording, and cannot change a meeting that is still being recorded.

## Switch it on

Settings… → AI Assistants → switch on **Let AI assistants use Muesli**. It is off by default. While it is off, the server refuses every tool call, so an installed extension or configured assistant reads nothing.

## Claude Desktop

Under **Claude Desktop**, click **Install in Claude Desktop…**. Claude Desktop opens its install dialog for the Muesli extension (`Muesli.mcpb`, bundled in the app); confirm it. Muesli then shows up as **Muesli** with its icon under Claude Desktop → Settings → Extensions.

If Claude Desktop greys out **Install** with *"This extension cannot be installed. Contact your organization owner"*, your Claude organisation blocks extensions it has not approved. Switch on **Add to Claude Desktop's config file instead**: Muesli adds a `Muesli` entry to `claude_desktop_config.json` (other entries are left alone). Quit and reopen Claude Desktop; it lists Muesli under its local developer servers, without an icon. Installing the extension later removes that entry.

The extension carries its own copy of `muesli-mcp`. When Settings says a newer version is available, click **Update Extension…**. To remove it, use Claude Desktop → Settings → Extensions.

Earlier versions added a lowercase `muesli` entry to `claude_desktop_config.json`; Muesli removes it on launch, so the server is never listed twice.

## Other assistants

Claude Code, Codex CLI, Gemini CLI, VS Code, Cursor and other MCP clients connect the same way. Pick yours under **Set up another assistant** and copy the command or config, for example for Claude Code:

```bash
claude mcp add --scope user muesli -- '/Applications/Muesli.app/Contents/MacOS/muesli-mcp'
```

On Windows, Settings shows the same command with the path of `muesli-mcp.exe` next to the installed app, quoted for PowerShell. Copy it from there:

```powershell
claude mcp add --scope user muesli -- 'C:\Program Files\Muesli\muesli-mcp.exe'
```

## Claude on your phone

There is no direct connection from claude.ai or the Claude mobile app to Muesli. Conversations sync across Claude apps, though, so a conversation in Claude Desktop on your Mac that read a transcript can be continued on your phone.

## What an assistant can call

| Tool | Does |
|---|---|
| `get_status` | Returns whether the app is running, whether a call or meeting is detected, the current recording and the last finished transcript |
| `list_transcripts` | Lists meetings, newest first; filter with `since`, `state`, `limit` |
| `get_transcript` | Returns one transcript as Markdown or JSON; works during a recording (`partial: true`); long ones are paged with `fromSeconds` |
| `rename_transcript` | Renames a finished meeting and its exported files |
| `set_attendees` | Sets the attendees of a finished meeting |
| `rename_speaker` | Renames a speaker in a finished meeting, for example "Others" to "Ann" |
| `delete_transcript` | Moves a finished meeting's files to the Trash or Recycle Bin |

The first three only read. The others work only on meetings that are no longer recording.

A transcript is complete when its `state` is `ready`. The assistant has to ask; Muesli cannot notify a conversation.

Calls to the management tools are logged to `~/Library/Application Support/Muesli/audit.log` (Windows: `%APPDATA%\Muesli\audit.log`), one JSON line per call with time, tool and meeting id. Read calls are not logged.

## Troubleshooting

Run the built-in check first:

```bash
/Applications/Muesli.app/Contents/MacOS/muesli-mcp doctor
```

It reports whether the preferences file exists, whether access is switched on, whether the app is running, and how many meetings it can see.

- **Claude Desktop does not show Muesli.** Restart Claude Desktop. Check that the extension is listed under Settings → Extensions, or, with the config-file option, that the `Muesli` entry in `~/Library/Application Support/Claude/claude_desktop_config.json` (Windows: `%APPDATA%\Claude\claude_desktop_config.json`) points at the app's current location; switching the option off and on rewrites it.
- **Tools answer "switched off".** Switch on **Let AI assistants use Muesli** in Settings… → AI Assistants.
- **`get_status` says the app is not running.** Open Muesli. The server relies on the app's heartbeat in `state.json`.
