# Muesli requirements: AI assistant integration

Status: implemented, except where section 10 says otherwise. Scope: what must be built so Claude and other MCP clients on the same Mac (Claude Desktop, Claude Code, Codex CLI, Gemini CLI, VS Code, Cursor) can ask Muesli whether it is recording, whether a transcript is ready, fetch that transcript and manage finished meetings.

Change on 2026-11-13: remote access was removed in favour of local-only. Section 5 lists what went.

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

Swift package layout: a `MuesliCore` library (models, store, state), the `Muesli` app target, and a `muesli-mcp` executable target that depends only on `MuesliCore`.

## 3. App requirements (Muesli.app)

Lifecycle

- A1. Every meeting has a stable id and one of these states: `detected`, `recording`, `finalizing`, `ready`, `failed`.
- A2. The app writes `state.json` atomically on every state change and at least every 10 s as a heartbeat. Fields: `schemaVersion`, `updatedAt`, `pid`, `appState` (`idle` | `preparing` | `recording` | `finalizing` | `error` | `permissionMissing`), `detected` (title, source, since) or null, `current` (id, title, source, startedAt, segmentCount) or null, `lastReady` (id, title, endedAt) or null, `error` or null.
- A3. On launch, a meeting left in `recording` or `finalizing` by a crash is saved with what was transcribed and marked `ready` with `partial: true`, or `failed` if nothing was captured.

Detection

- A4. The detector keeps a current "meeting in progress" value for as long as the signal holds (calendar event running, mic in use), not only at the moment it prompts. This value is reported whether or not the user chose to record.
- A5. The detector keeps running during a recording and detects the end of a call (mic released for a configurable period, default 60 s). On end it prompts to stop, or stops automatically if the user enabled that setting.

Storage

- A6. Each meeting's canonical JSON is written to `~/Library/Application Support/Muesli/meetings/<id>.json`. During recording it is rewritten as chunks are transcribed, so partial transcripts survive a crash and can be read while recording.
- A7. The `.md` and `.json` files in the user's transcripts folder are still written when the meeting becomes `ready`. Their format does not change, apart from an added `state` field in the JSON.
- A8. Audio is never written to disk.

Assistant access

- A9. A Settings window sets the transcripts folder, chooses the transcription languages (default: the OS language plus English; Whisper picks the most likely of these per chunk) and has one switch, "Let AI assistants use Muesli" (off by default). It installs the Claude Desktop extension, or adds a config-file entry instead (M9), and shows the setup command or config for other assistants with a Copy button. Settings persist across launches in `config.json`.
- A9a. On the first launch a setup window is an assistant with one question per screen: model choice (selecting starts the download, with progress), languages (defaults from the Mac, add or remove), automatic stop when the call ends (yes, or ask first), AI assistants (the MCP switch), then one screen per permission (microphone, system audio, calendar, notifications). It is shown until finished (`onboardingDone` in `config.json`); a `config.json` from before onboarding counts as finished. Calendar and notification prompts are asked only on their own screen, never at launch.
- A9b. Muesli supports macOS (Swift app in `macos/`) and Windows (Tauri app in `windows/`); both are permanent. Windows: same files and MCP tools; models are stored in `%LOCALAPPDATA%\Muesli`; system audio through WASAPI loopback (no permission), microphone through WASAPI; calls detected from other apps using the microphone and from Teams; no calendar detection (Windows has no system calendar, see `PLATFORMS.md`); the transcripts folder is kept in `config.json` (`transcriptsFolder`); deleted transcripts go to the Recycle Bin; the first-launch assistant omits the system-audio screen.
- A10. The app itself opens no sockets. Its only network request is downloading the Whisper model (Small by default, or Medium or Large chosen in Settings; a custom model file can be used instead). Transcribing is disabled, and meeting prompts are not shown, until a model is ready; progress is shown in Settings and the menu while it downloads. System audio is captured with a Core Audio tap (macOS 14.4 or later), so only the System Audio Recording permission is needed.

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
- M9. Claude Desktop gets the bundled Desktop Extension `Muesli.mcpb` (name and icon). For organisations that block extensions, a switch adds a `Muesli` entry to `claude_desktop_config.json` instead, leaving other entries untouched; installing the extension removes that entry. The server refuses tool calls unless access is switched on.
- M10. `muesli-mcp doctor` checks the preferences file, whether access is switched on, whether the app is running (`state.json` heartbeat) and how many meetings it can see.

## 5. Remote access (removed)

Until 2026-11-13 Muesli could also be reached over the internet, for claude.ai in the browser and on mobile: `muesli-mcp --http <port>` behind a Cloudflare Tunnel, guarded by Cloudflare Access and checked again with the Access JWT in `muesli-mcp`. That mode is gone, together with its preferences, the remote provider picker, the remote checks in `doctor` and the remote audit by email. The requirements that described it (the HTTP half of M2, M7, the old M8, C1 to C8, S1, S2, R1, R2, R4 and acceptance criteria 3 and 5) were removed rather than rewritten.

Claude conversations sync across Claude apps, so work done in Claude Desktop on the Mac is visible in the Claude app on a phone.

## 6. Security and privacy

- S3. No tool can start or stop a recording or change settings. Management tools change only finished meetings; delete moves files to the Trash. Meetings that are still recording cannot be changed.
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

Done and tested with `swift test` on macOS (101 tests): M1 to M6, M8, M10, preferences, the meeting store and management tools, crash recovery (A3), the detection rules behind A4 and A5, the Claude Desktop extension manifest and install detection, and the config-file registration behind M9. The real binary was checked over stdio (`tools/list`, `list_transcripts`).

Built and run on macOS, checked by hand: the app (menu, Settings window, recording, transcription, permissions) and packaging of `Muesli.mcpb`. Not yet verified: installing the extension in Claude Desktop (blocked by organisation policy on the test account) and the per-process microphone check that feeds A4 and A5.

Open: acceptance criteria 2, 4 and 6 still need a full meeting run on a Mac.

Deviations: A5 needs macOS 14.2 or later; on 14.0 and 14.1 Muesli cannot tell its own microphone use from another app's, so the end of a call is not detected. The server answers MCP protocol versions up to 2025-06-18.
