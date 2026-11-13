# Contributing to Muesli

Thanks for helping. This file is the developer documentation: how to build, test and find your way around. The product description is in [README.md](README.md).

## Build

Requires macOS 14+ and Xcode 15+.

```bash
mkdir -p ~/Library/Application\ Support/Muesli
curl -L -o ~/Library/Application\ Support/Muesli/ggml-small.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin
./build.sh
open Muesli.app
```

`build.sh` runs `swift build -c release`, assembles `Muesli.app`, copies `muesli-mcp` next to the main executable, builds `Muesli.icns` from the iconset, packs the Claude Desktop extension `Muesli.mcpb` (manifest from `muesli-mcp --mcpb-manifest`, the 512 px icon and a signed copy of `muesli-mcp`) into `Contents/Resources`, and signs the bundle. Muesli needs a real bundle for notifications and permissions, so run the app, not the bare executable.

## Test

```bash
swift test
```

The tests cover everything in `MuesliCore`: audio chunking, Whisper segment mapping, language selection, transcript files, detection rules, the meeting store, crash recovery and importing older transcripts, preferences, Claude Desktop registration, MCP client setup snippets, the management tools and the MCP server over stdio. They need no microphone, calendar or network, and each test gets its own temporary data folders.

`MuesliCore` and `muesli-mcp` use only Foundation, so `swift build` and `swift test` also work on Linux. `Package.swift` adds the app target and its whisper.cpp dependency on macOS only.

When you add logic, put it in `MuesliCore` if it can live without AppKit, AVFoundation, EventKit or CoreAudio, and test it there. The app target should stay a thin shell.

### What is verified

- Transcription end to end, in local testing on macOS.
- `MuesliCore` and `muesli-mcp`: compiled and tested on Linux, including the real binary over stdio.
- Not yet compiled or run on macOS since they were written: the Settings window, the `state.json` heartbeat, the Claude Desktop bridge, and the per-process microphone check in `MeetingDetector`.

If you build on a Mac and something in the third group fails to compile or misbehaves, that is a useful bug report.

## How it works

```
 Detector (every 5 s)                 Capture                                  Output
 ├ Calendar (EventKit)  ─┐        ┌ Mic via AVAudioEngine ("You")
 ├ Teams process         ├─► Notification ─► Start ─┤                 ─► 30 s chunks ─► whisper.cpp (on-device) ─► transcripts folder
 └ Mic in use (CoreAudio)┘   "Start transcribing?"  └ System audio via ScreenCaptureKit ("Others")
```

```
Muesli.app                 no network; capture, detection, whisper.cpp
   │ writes state + meetings to ~/Library/Application Support/Muesli/
   │ writes .md/.json to the transcripts folder
   ▼
muesli-mcp                 separate executable, MCP server over stdio
   ▲
   └─ started by the assistant on the same Mac (Claude Desktop, Claude Code, Codex, …)
```

The app never opens a socket and nothing is exposed to the network. Everything assistant-related runs in `muesli-mcp`, which the assistant starts itself. It reads what the app wrote, and its management tools (`MeetingEditor`) change only finished meetings: the store entry and the exported `.md`/`.json`, located through `transcriptsFolder` in `state.json`. The reasoning and the full requirement list are in [REQUIREMENTS.md](REQUIREMENTS.md).

### Detection

`DetectionLogic` (in `MuesliCore`) holds the rules; `MeetingDetector` samples the system every 5 seconds and feeds it.

- A calendar event counts as a meeting when it has more than one attendee or mentions a known meeting host. It is suggested once per event.
- Mic activity counts as a call once another app has used a microphone for more than 10 seconds. Audio-only suggestions are at most 10 minutes apart.
- During a recording, once another app has used the mic and then released it for `endDelaySeconds` (default 60), the call counts as ended. A recording in which no other app used the mic never ends by itself.
- Telling Muesli's own mic use from other apps' uses the CoreAudio process list, which needs macOS 14.2. Before that, the detector cannot judge the mic while Muesli records.

### Meeting lifecycle

`recording → finalizing → ready`, or `failed`. The meeting is saved to the store when recording starts and again after every transcribed chunk, so a crash loses at most the audio not yet transcribed. On launch, `MeetingStore.recoverInterrupted()` turns leftovers into `ready` with `partial: true`, or `failed` when nothing was transcribed. It then runs `MeetingStore.importTranscripts()`, which copies any `.json` in the transcripts folder whose id is not in the store yet (meetings recorded before the store existed), so `list_transcripts` sees them; it never overwrites a stored meeting.

## Files on disk

| Path | Written by | Contents |
|---|---|---|
| `~/Documents/MeetingTranscripts/` (configurable) | app | The `.md` and `.json` pair per finished meeting |
| `~/Library/Application Support/Muesli/ggml-small.bin` | you | Whisper model |
| `…/Muesli/meetings/<id>.json` | app | Canonical copy of every meeting, rewritten during recording |
| `…/Muesli/state.json` | app | Current state and heartbeat, rewritten on change and every 10 s |
| `…/Muesli/config.json` | app | Preferences shared with `muesli-mcp` |
| `…/Muesli/audit.log` | `muesli-mcp` | One JSON line per management tool call (time, tool, meetingId); read calls are not logged |

Transcript JSON fields: `id`, `title`, `source` (calendar | teams | audio | manual), `startedAt`, `endedAt`, `attendees`, `state`, `partial`, `transcript[]` (`start`, `end`, `speaker`, `text`).

Environment variables: `TRANSCRIPT_DIR` overrides the transcripts folder; `MUESLI_SUPPORT_DIR` overrides the Application Support folder (used by the tests).

## Code layout

| Path | Purpose |
| --- | --- |
| `Sources/Muesli/App.swift` | Menu bar UI, notifications, record/stop flow, `state.json` heartbeat |
| `Sources/Muesli/MeetingDetector.swift` | Samples calendar, Teams and microphone use; feeds `DetectionLogic` |
| `Sources/Muesli/AudioCapture.swift` | Mic and system audio capture, resampled to 16 kHz mono |
| `Sources/Muesli/Transcriber.swift` | whisper.cpp through its C API (the `whisper_cpp` module of SwiftWhisper): language detection limited to the configured languages, then transcription |
| `Sources/Muesli/SettingsView.swift` | Settings window (sidebar: General, Transcription, AI Assistants), brand accent colours |
| `Sources/MuesliCore/AssistantSetup.swift` | Setup command or config snippet per MCP client (Claude Code, Codex, Gemini CLI, VS Code, Cursor, other) |
| `Sources/Muesli/Bridge.swift` | Claude Desktop extension status and install, or the `Muesli` entry in `claude_desktop_config.json` for accounts that block extensions |
| `Sources/Muesli/Permissions.swift` | Microphone and Screen Recording checks before recording |
| `Sources/MuesliCore/` | Shared, Foundation-only: data types, paths, transcript files, meeting store, preferences, transcription languages, audio chunking, detection rules, Claude Desktop extension manifest and install detection (`ClaudeDesktopExtension.swift`), MCP client setup snippets (`AssistantSetup.swift`), rename/attendees/speaker/delete (`MeetingEditor.swift`) |
| `Sources/MuesliCore/MCP/` | The MCP server: tools, stdio transport, audit log, `doctor` |
| `Sources/muesli-mcp/` | Entry point of the `muesli-mcp` executable |
| `Tests/MuesliCoreTests/` | Unit tests |
| `Info.plist`, `build.sh` | App bundle metadata and build script |
| `assets/brand/` | Logo, app icon and menu bar icon |
| `docs/CLAUDE_SETUP.md` | User-facing setup for Claude and other AI assistants |
| `REQUIREMENTS.md` | Requirements and design decisions for the Claude integration |

## The MCP server

```bash
.build/debug/muesli-mcp                 # stdio
.build/debug/muesli-mcp doctor          # checks preferences, access switch, app heartbeat and meetings
```

It is a hand-written JSON-RPC server with no dependencies, speaking MCP protocol versions up to 2025-06-18. To try it by hand:

```bash
export MUESLI_SUPPORT_DIR=/tmp/muesli-dev
mkdir -p $MUESLI_SUPPORT_DIR && echo '{"mcpEnabled":true}' > $MUESLI_SUPPORT_DIR/config.json
echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"get_status","arguments":{}}}' | .build/debug/muesli-mcp
```

Rules to keep:

- No tool may start or stop a recording or change settings. Management tools only touch meetings in state `ready` or `failed`, never overwrite another meeting's files, and delete by moving to the Trash. Read tools are annotated `readOnlyHint`, delete as `destructiveHint`.
- The stdio server refuses tool calls unless access is switched on.
- Management tool calls are written to `audit.log`.

## Permissions and code signing

Grant Microphone, Calendar and Screen Recording when asked. Screen Recording is what allows system audio capture; after enabling it, quit and reopen Muesli.

Without a signing certificate `build.sh` signs ad hoc, and macOS ties permissions for ad-hoc apps to the exact build, so each rebuild can invalidate them. If a permission looks enabled but still fails, remove Muesli from that list, or reset it, then relaunch and grant again:

```bash
tccutil reset Microphone com.github.bdhoine.muesli
tccutil reset ScreenCapture com.github.bdhoine.muesli
```

Signing with a real certificate avoids this. `build.sh` uses the first "Apple Development" or "Developer ID Application" certificate in your keychain (override with `SIGN_IDENTITY="..." ./build.sh`). To get a free Apple Development certificate: Xcode → Settings → Accounts → add your Apple ID → Manage Certificates → + → Apple Development. After the first signed build, reset and grant the permissions once more; they then survive rebuilds.

## Conventions

- **Brand.** Follow [assets/brand/BRAND.md](assets/brand/BRAND.md) for the logo, colours, icons and tone. Use the asset files as they are; do not redraw or recolour them.
- **No network.** The app opens no sockets and `muesli-mcp` speaks stdio only. Nothing listens on a port.
- **Audio is never written to disk.**
- **Dependencies.** SwiftWhisper is pinned to an exact commit in `Package.swift`; bump it deliberately. Avoid adding others.
- **Pull requests.** Keep them focused, add tests for logic in `MuesliCore`, and run `swift test` before opening one.
