# Muesli requirements: AI assistant integration

Status: implemented, except where section 10 says otherwise. Scope: what must be built so Claude and other MCP clients on the same Mac (Claude Desktop, Claude Code, Codex CLI, Gemini CLI, VS Code, Cursor) can ask Muesli whether it is recording, whether a transcript is ready, fetch that transcript and manage finished meetings.

Change on 2026-11-13: remote access was removed in favour of local-only. Section 5 lists what went.

Change on 2026-11-19 (the refinement): one Tauri app for macOS and Windows on a Rust engine behind ports; new main window, setup, Settings and menu; the end of a call is always asked, with a 5-minute timeout; the drop folder, auto-stop, the delay and recent-count settings and the custom model file are gone. Section 3a describes the user-facing behaviour; the requirements below are updated to match.

## 1. Goals and non-goals

Goals

- G1. Muesli detects calls and meetings in progress, transcribes them on-device and writes a file. No cloud or API is involved in capture, detection or transcription.
- G2. Claude and other MCP clients running on the same Mac can read Muesli's state and transcripts, and manage finished meetings.
- G3. The integration is an optional add-on, off by default. Neither the app nor `muesli-mcp` exposes anything to the network.

Non-goals

- Starting or stopping a recording from Claude.
- Summaries, action items or any other processing of the transcript; Claude does that.
- Per-person speaker diarization.
- Storing transcripts anywhere other than the Mac.
- Access from another device or over the internet, including claude.ai in the browser and on mobile.

## 2. Architecture

```
Muesli.app                 network only for model download; capture, detection, whisper.cpp
   │ writes state + meetings to ~/Library/Application Support/Muesli/
   │ writes .md/.json to the transcripts folder (unchanged)
   ▼
muesli-mcp                 separate executable, MCP server over stdio
   ▲
   └─ started by the assistant on the same Mac (Claude Desktop, Claude Code, Codex CLI, …)
```

Layout: one Rust workspace. `crates/core` (data, rules, the ports), `crates/engine` (behaviour, view states, intents), `crates/mcp` (`muesli-mcp`), the platform adapters (`crates/platform-*`) and the Tauri app (`apps/desktop`) with the web UI (`ui/`), the same on macOS and Windows.

## 3. App requirements (Muesli.app)

Lifecycle

- A1. Every meeting has a stable id and one of these states: `detected`, `recording`, `finalizing`, `ready`, `failed`.
- A2. The app writes `state.json` atomically on every state change and at least every 10 s as a heartbeat. Fields: `schemaVersion`, `updatedAt`, `pid`, `appState` (`idle` | `preparing` | `recording` | `finalizing` | `transcribingFile` | `error` | `permissionMissing`), `detected` (title, source, since) or null, `current` (id, title, source, startedAt, segmentCount) or null, `lastReady` (id, title, endedAt) or null, `fileJob` (name, progress, waiting) while an audio file is being transcribed, `error` or null.
- A3. On launch, a meeting left in `recording` or `finalizing` by a crash is saved with what was transcribed and marked `ready` with `partial: true`, or `failed` if nothing was captured.

Detection

- A4. The detector keeps a current "meeting in progress" value for as long as the signal holds (calendar event running, mic in use), not only at the moment it prompts. This value is reported whether or not the user chose to transcribe. A meeting is suggested only once: a calendar event once per occurrence, a call (mic in use by another app) once however long it lasts, whether the prompt was dismissed, ignored or accepted. A call counts as new only after the microphone was free for 60 s, and an audio call that overruns its calendar meeting is not suggested again. Muesli never starts transcribing by itself.
- A5. The detector keeps running while transcribing and detects the end of a call (mic released for a fixed 60 s). On end it always asks whether to stop and save; with no answer within 5 minutes it stops, saves and posts a notice "Saved: <title>" with an Open button. There is no auto-stop setting and no delay setting.

Storage

- A6. Each meeting's canonical JSON is written to `~/Library/Application Support/Muesli/meetings/<id>.json`. During recording it is rewritten as chunks are transcribed, so partial transcripts survive a crash and can be read while recording.
- A7. The `.md` and `.json` files in the user's transcripts folder are still written when the meeting becomes `ready`. Their format does not change, apart from an added `state` field in the JSON.
- A8. Audio is never written to disk.

Assistant access

- A9. Settings is one scrolling page (section 3a): the transcripts folder, start at login, the transcription languages (default: the OS language plus English; Whisper picks the most likely of these per chunk), and one switch, "Let AI assistants use Muesli" (off by default). It installs the Claude Desktop extension, or adds a config-file entry instead (M9), and shows the setup command for other assistants with a Copy button. Settings persist across launches in `config.json`; keys of earlier versions are dropped on start.
- A9c. The speech model. There is one (Large v3 Turbo) and nothing to pick; a failed download can be retried ("Download again") and shows the reason and the address it tried. Connecting to the model server gives up after 15 s instead of hanging. Model files of earlier versions are deleted automatically. Settings > Advanced shows the model's status and what the operating system currently allows, with a button to the system settings.
- A9a. On the first launch a setup window shows four screens with a dot indicator: Welcome ("Meet Muesli", three promises, "Get started", which starts the model download), Languages, Permissions (one screen; macOS: microphone and system audio required, calendar and notifications optional; Windows: microphone only; Continue waits for the required ones; the status is read again when the window comes back to the front) and Done ("Muesli is in your menu bar / system tray", the model progress, the notice to tell people, "Open Muesli"). It is shown until finished (`onboardingDone` in `config.json`); a `config.json` from before onboarding counts as finished. Permissions are asked only from this screen and Settings, never at launch.
- A9b. Muesli is one app for macOS and Windows (Tauri, `apps/desktop`), with platform adapters behind ports. Windows: models in `%LOCALAPPDATA%\Muesli`; system audio through WASAPI loopback (no permission), microphone through WASAPI; calls detected from other apps using the microphone and from Teams; no calendar detection (Windows has no system calendar, see `PLATFORMS.md`); deleted transcripts go to the Recycle Bin. `PLATFORMS.md` lists every difference.
- A9d. Audio files. "Transcribe Audio File…" (menu and main window) picks one or more files (mp3, m4a/aac, wav, flac, ogg, alac); files dropped on a Muesli window are transcribed too. Progress shows in the main window's list, with Cancel. One file at a time, after the model is ready; a live transcription always goes first. Files are never moved or deleted. The result is a normal meeting with source `file`, speaker `Speaker`, the file name as title and the file's date; a file that cannot be read, or fails part-way, is reported and keeps what was transcribed. If the app quits during an import, the next import of that file resumes after the last transcribed chunk in the same meeting. There is no drop folder.
- A9e. Claude plugin. `plugin/` is a plugin bundle (license, README, manifest, one skill, the `muesli-mcp` server of the installed app as its MCP server), installed from this repository; Anthropic's directory accepts local MCP servers only from Verified organizations, so it is not listed there. A plugin's MCP command cannot differ per OS, so it starts a Node launcher that finds `muesli-mcp` on macOS and Windows (A9f). It passes `claude plugin validate ./plugin --profile directory --strict`, and its version, icon and license are kept in step with the app by `scripts/sync-plugin.py` (checked in CI).
- A9f. The app records the absolute path of its `muesli-mcp` in `mcp-path` in the support folder on every start (`record_helper_path`), so the plugin launcher finds the server wherever Muesli is installed. The `.mcpb` extension and the config-file entry are separate routes.
- A10. The app itself opens no sockets. Its only network requests are downloading the Whisper speech model (Large v3 Turbo, one fixed model) and the small Silero voice activity model. Transcribing is disabled, and meeting prompts are not shown, until both are ready; progress is shown in setup, Settings and the menu ("Getting Ready") while they download. Audio without speech is never transcribed: the voice activity model finds the speech first, because Whisper invents text for silence and noise. System audio is captured with a Core Audio tap (macOS 14.4 or later), so only the System Audio Recording permission is needed.

## 3a. User-facing behaviour (2026-11 refinement)

The UI says "transcribe / transcribing", never "record"; JSON and MCP values keep their names.

- U1. Start: a question "Looks like a meeting: <title>" / "Detected via <source>. Start transcribing?" [Start transcribing] [Not now], as a notification; when notifications are off or cannot be shown, a small Muesli window asks the same (both platforms).
- U2. End: "The call seems to have ended" / "Muesli is still transcribing. Stop and save the transcript?" [Stop and save] [Keep transcribing]; no answer in 5 minutes: stop, save, notice "Saved: <title>" / "No answer for 5 minutes, so Muesli stopped and saved the transcript." [Open].
- U3. Menu (menu bar / tray): Start Transcribing, or a header "Transcribing · <title>" with Stop and Save; Open Muesli; Transcribe Audio File…; a "Recent Transcripts" header and the 3 latest transcripts (open in the main window); Settings… (⌘,); Quit Muesli (⌘Q). macOS: an SF Symbol beside each item. While the model downloads, a "Getting Ready" section with progress, and Start and file import disabled. A red dot on the icon while transcribing. macOS: a click shows the menu. Windows: a left click opens the main window, a right click shows the menu.
- U4. Main window: list on the left, newest first, with the live meeting and file imports at the top (imports with progress and Cancel); reader on the right with title, date, duration, source, attendees and the text (`[mm:ss] **Speaker**: text`). The live meeting updates every 30 s, read-only, with a "Transcribing · live, read-only" badge and Stop and Save. A finished meeting can be renamed (also its exported files; the same logic as `rename_transcript`) and deleted (to the Trash / Recycle Bin, no confirmation, an Undo toast for about 6 s; the same logic as `delete_transcript`). Toolbar: Transcribe Audio File…, Show in Finder / Explorer. No search, no speaker or attendee editing (assistants keep `rename_speaker` and `set_attendees`). macOS: a Dock icon only while a Muesli window is open.
- U5. Settings, setup and the model: A9, A9a and A9c.
- U6. The UI decides nothing: the engine (`crates/engine`) gives each window its view state and takes intents; platform words come from the view. Dark mode follows the system; colours and type follow `assets/brand/BRAND.md`.

## 4. MCP server requirements (muesli-mcp)

- M1. Reads from `~/Library/Application Support/Muesli/`. It never touches audio. It writes meeting data only through the management tools (M3), for meetings that are no longer recording.
- M2. Speaks MCP over stdio only. It is started by the assistant and never opens a socket.
- M3. Tools. The first three are read-only and annotated as such; the management tools are annotated as writes, delete as destructive:

  | Tool | Input | Output |
  |---|---|---|
  | `get_status` | none | `appRunning` (heartbeat younger than 30 s), `appState`, `detected`, `current`, `lastReady`, `error` |
  | `list_transcripts` | `since?`, `limit?` (default 20), `state?` | id, title, source, startedAt, endedAt, state, durationMinutes, attendees |
  | `get_transcript` | `id`, `format?` (`markdown` \| `json`), `fromSeconds?`, `toSeconds?` | transcript text or segments, `state`, `partial`, `truncated`, `nextFromSeconds` |
  | `rename_transcript` | `id`, `title` | updated meeting summary; exported files renamed |
  | `set_attendees` | `id`, `attendees` | updated meeting summary |
  | `rename_speaker` | `id`, `from`, `to` | updated meeting summary |
  | `delete_transcript` | `id` | `deleted`, `title`, `movedToTrash` |

- M4. `get_transcript` works on a meeting that is still `recording` and returns what exists so far with `partial: true`.
- M5. A single response is capped (default about 60,000 characters). Longer transcripts are paged with `fromSeconds` and `nextFromSeconds`.
- M6. If the heartbeat is stale, `get_status` reports `appRunning: false` and never reports `recording`.
- M8. Each management tool call is appended to `~/Library/Application Support/Muesli/audit.log`, one JSON line with time, tool and meeting id. Read calls are not logged.
- M9. Claude Desktop gets the bundled Desktop Extension `Muesli.mcpb` (name and icon). The `.mcpb` stays the one-click route on both platforms: it is the only install that gives Claude Desktop Chat the tools (a plugin's local server does not load in Chat), and Anthropic has deprecated `.mcpb` only for directory listings, not for installing a local file (checked 2026-11-17; no sunset date is published, so re-check the MCPB guide from time to time). For organisations that block extensions, a switch adds a `Muesli` entry to `claude_desktop_config.json` instead, leaving other entries untouched; installing the extension removes that entry. The server refuses tool calls unless access is switched on.
- M10. `muesli-mcp doctor` checks the preferences file, whether access is switched on, whether the app is running (`state.json` heartbeat) and how many meetings it can see.

## 5. Remote access (removed)

Until 2026-11-13 Muesli could also be reached over the internet, for claude.ai in the browser and on mobile: `muesli-mcp --http <port>` behind a Cloudflare Tunnel, guarded by Cloudflare Access and checked again with the Access JWT in `muesli-mcp`. That mode is gone, together with its preferences, the remote provider picker, the remote checks in `doctor` and the remote audit by email. The requirements that described it (the HTTP half of M2, M7, the old M8, C1 to C8, S1, S2, R1, R2, R4 and acceptance criteria 3 and 5) were removed rather than rewritten.

Claude conversations sync across Claude apps, so work done in Claude Desktop on the Mac is visible in the Claude app on a phone.

## 6. Security and privacy

- S3. No tool can start or stop transcribing or change settings. Management tools change only finished meetings; delete moves files to the Trash. Meetings that are still recording cannot be changed.
- S4. The setup guide and Settings state plainly that transcript text an assistant reads is sent to its provider (Anthropic, for Claude).
- S5. The existing notice stays: tell participants you are transcribing and check company policy.

## 7. Risks and open questions

- R3. Assistants can only poll. Nothing can notify a conversation that a transcript became ready.

## 8. Acceptance criteria

1. A network monitor shows no connections from Muesli or `muesli-mcp` during detection, recording, saving and tool calls, and no listening port.
2. In Claude Desktop with the extension installed: "Is Muesli recording?" returns the true state within 10 s of a change.
3. Removed with remote access.
4. During a recording, Claude can fetch the transcript so far.
5. Removed with remote access.
6. Killing the app mid-recording, then relaunching, yields a saved partial transcript and a correct status.

## 9. Build order

1. App: lifecycle, `state.json`, canonical meeting store, continuous detection (A1 to A8).
2. `muesli-mcp` over stdio and Claude Desktop registration (M1 to M6, M9).
3. Management tools, audit log, Settings switch (M3, M8, A9).
4. Setup guide and `doctor` command (M10).

## 10. Implementation status

Implemented in the Rust workspace and tested on Linux: the engine against the fakes (prompts, timings, main window, Settings, setup, imports, migration), the UI against the simulator in Chromium for both skins, M1 to M6, M8, M10, the stores and management tools, crash recovery (A3) and the detection rules behind A4 and A5.

Not yet run in the real app on either platform since the refinement: everything that touches the OS (capture, notifications, permissions, calendar, Trash, login item, tray). The macOS adapters have not been compiled. `PLATFORMS.md` section 3 tracks what has been verified on real machines.

Deviations: A5 needs macOS 14.2 or later; on 14.0 and 14.1 Muesli cannot tell its own microphone use from another app's, so the end of a call is not detected. The server answers MCP protocol versions up to 2025-06-18.
