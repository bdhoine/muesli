# Muesli for Claude

This plugin connects Claude to [Muesli](https://github.com/bdhoine/muesli), an on-device meeting transcriber that lives in the macOS menu bar or the Windows system tray. Ask Claude whether Muesli is transcribing, to read a transcript, to summarise a meeting and list its action items, or to rename a meeting, its attendees or a speaker.

> Muesli is not listed in Anthropic's plugin directory, which accepts local MCP servers only from Verified organizations. Install it from this repository as described below.

## Before you install

- Install the Muesli app (macOS 14.4 or later, or Windows 10/11; see the [repository](https://github.com/bdhoine/muesli)) and open it once.
- In Muesli, open **Settings → AI Assistants** and switch on access for assistants.
- Install [Node.js](https://nodejs.org) if you do not have it. The plugin starts a small launcher script with `node`, because a plugin's MCP command cannot differ per operating system, and the launcher finds Muesli's server on macOS and on Windows.
- A local MCP server loads in Claude Code and in Cowork sessions that run on your computer. Claude Chat ignores local servers.

## Install

In Claude Code, version 2.1.275 or later:

```
/plugin install muesli --marketplace bdhoine/muesli
```

In the Claude desktop app, open **Customize → Plugins**, choose **Add → Add marketplace** and enter `bdhoine/muesli`, then add Muesli from it.

## What it runs

The plugin starts `node server/muesli-mcp.js`, a short script (about 80 lines, in this folder) that finds the `muesli-mcp` server inside the installed app and runs it, passing standard input and output through. It looks in this order: the path in the `MUESLI_MCP` environment variable, the file `mcp-path` in Muesli's support folder (the app writes its own location there every time it starts), then the usual install folders (`/Applications/Muesli.app` on macOS; `%LOCALAPPDATA%\Muesli`, `%LOCALAPPDATA%\Programs\Muesli` and `%ProgramFiles%\Muesli` on Windows). The server talks to Claude over standard input and output only. It opens no network connection and no port, downloads nothing and installs nothing. It reads Muesli's files in the support folder (`~/Library/Application Support/Muesli` on macOS, `%APPDATA%\Muesli` on Windows) and your transcripts folder, and it can rename and delete transcripts there and change the vocabulary (`vocabulary.json`). Apart from the launcher, the plugin contains only this configuration, one skill with instructions for Claude, and an icon.

## Tools

| Tool | What it does |
|---|---|
| `get_status` | Whether Muesli is running, detecting a meeting, recording or idle, and the latest finished transcript |
| `list_transcripts` | Lists meetings, newest first |
| `get_transcript` | Reads one transcript |
| `rename_transcript` | Changes a finished meeting's title |
| `set_attendees` | Replaces a finished meeting's attendee list |
| `rename_speaker` | Renames a speaker label, for example "Others" to a person's name |
| `delete_transcript` | Moves a finished meeting to the Trash or Recycle Bin. Claude asks you first |
| `get_vocabulary` | Lists the names and terms Muesli spells right |
| `update_vocabulary` | Adds or removes names and terms for future transcripts |

Claude cannot start or stop transcribing.

The plugin's skill also tells Claude how to summarise a meeting: give a generic meeting a proper title, set the attendees and real speaker names when the conversation makes them clear, and write the summary in a fixed shape (purpose, key points, decisions, action items, open questions, next steps) from `skills/muesli/summary-template.md`. When you correct a misheard name or term, Claude adds the right spelling to Muesli's vocabulary, so the next transcripts get it right.

## Privacy

Transcription happens on your computer and audio is never stored. What Claude reads from a transcript is sent to Anthropic as part of your conversation, like anything else you share with Claude. Tell the people you meet with that you are transcribing. The full statement is in the [repository README](https://github.com/bdhoine/muesli#private-by-design).

## Source and license

The server, the app and this plugin are in the [muesli repository](https://github.com/bdhoine/muesli) (the plugin is the `plugin/` folder), released under the MIT license. Report problems in the [issue tracker](https://github.com/bdhoine/muesli/issues).
