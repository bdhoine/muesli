# Contributing to Muesli

Thanks for helping. This file is the developer documentation: how to build, test and find your way around. The product description is in [README.md](README.md).

## Repository layout

Muesli supports macOS and Windows, and the two apps are peers in this repository:

```
macos/          The macOS app (Swift): Package.swift, Sources/, Tests/, Info.plist, build.sh
windows/        The Windows app (Rust and Tauri): crates/, src-tauri/, ui/
fixtures/       The contract between the two: files and MCP replies produced by the Swift app, checked by the Rust tests
assets/brand/   The one source for logo, icons and colours
scripts/        build-mcpb.py (Claude Desktop extension), sync-icons.py, regenerate-fixtures.sh
docs/           REQUIREMENTS.md, PLATFORMS.md, CLAUDE_SETUP.md
```

The sections below describe the macOS app first, then [the Windows app](#the-windows-app-windows). Both are permanent; neither replaces the other.

## Build (macOS app)

Requires macOS 14.4+ and Xcode 15+.

```bash
macos/build.sh
open macos/Muesli.app
```

On first launch Muesli downloads the Small model into `~/Library/Application Support/Muesli/`. Set `MUESLI_SUPPORT_DIR` to keep a test copy elsewhere.

`build.sh` runs `swift build -c release`, assembles `Muesli.app`, copies `muesli-mcp` next to the main executable, builds `Muesli.icns` from the iconset, packs the Claude Desktop extension `Muesli.mcpb` (manifest from `muesli-mcp --mcpb-manifest`, the 512 px icon and a signed copy of `muesli-mcp`) into `Contents/Resources`, and signs the bundle. Muesli needs a real bundle for notifications and permissions, so run the app, not the bare executable.

## Test

```bash
cd macos
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
 └ Mic in use (CoreAudio)┘   "Start transcribing?"  └ System audio via Core Audio tap ("Others")
```

```
Muesli.app                 network only for model download; capture, detection, whisper.cpp
   │ writes state + meetings to ~/Library/Application Support/Muesli/
   │ writes .md/.json to the transcripts folder
   ▼
muesli-mcp                 separate executable, MCP server over stdio
   ▲
   └─ started by the assistant on the same Mac (Claude Desktop, Claude Code, Codex, …)
```

The app never opens a socket and nothing is exposed to the network. Everything assistant-related runs in `muesli-mcp`, which the assistant starts itself. It reads what the app wrote, and its management tools (`MeetingEditor`) change only finished meetings: the store entry and the exported `.md`/`.json`, located through `transcriptsFolder` in `state.json`. The reasoning and the full requirement list are in [docs/REQUIREMENTS.md](docs/REQUIREMENTS.md).

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
| `~/Library/Application Support/Muesli/ggml-*.bin` | the app (model download) or you | Whisper model |
| `…/Muesli/meetings/<id>.json` | app | Canonical copy of every meeting, rewritten during recording |
| `…/Muesli/state.json` | app | Current state and heartbeat, rewritten on change and every 10 s |
| `…/Muesli/config.json` | app | Preferences shared with `muesli-mcp` |
| `…/Muesli/audit.log` | `muesli-mcp` | One JSON line per management tool call (time, tool, meetingId); read calls are not logged |

Transcript JSON fields: `id`, `title`, `source` (calendar | teams | audio | manual), `startedAt`, `endedAt`, `attendees`, `state`, `partial`, `transcript[]` (`start`, `end`, `speaker`, `text`).

Environment variables: `TRANSCRIPT_DIR` overrides the transcripts folder; `MUESLI_SUPPORT_DIR` overrides the Application Support folder (used by the tests).

## Code layout

| Path | Purpose |
| --- | --- |
| `macos/Sources/Muesli/App.swift` | Menu bar UI, notifications, record/stop flow, `state.json` heartbeat |
| `macos/Sources/Muesli/MeetingDetector.swift` | Samples calendar, Teams and microphone use; feeds `DetectionLogic` |
| `macos/Sources/Muesli/AudioCapture.swift` | Mic and system audio capture, resampled to 16 kHz mono |
| `macos/Sources/Muesli/Transcriber.swift` | whisper.cpp through its C API (the `whisper_cpp` module of SwiftWhisper): language detection limited to the configured languages, then transcription |
| `macos/Sources/Muesli/SettingsView.swift` | Settings window (sidebar: General, Transcription, AI Assistants), brand accent colours |
| `macos/Sources/MuesliCore/AssistantSetup.swift` | Setup command or config snippet per MCP client (Claude Code, Codex, Gemini CLI, VS Code, Cursor, other) |
| `macos/Sources/Muesli/Bridge.swift` | Claude Desktop extension status and install, or the `Muesli` entry in `claude_desktop_config.json` for accounts that block extensions |
| `macos/Sources/Muesli/OnboardingView.swift` | First-launch setup window (model, languages, assistants, permissions) |
| `macos/Sources/Muesli/Permissions.swift` | Microphone and System Audio Recording checks before recording |
| `macos/Sources/MuesliCore/` | Shared, Foundation-only: data types, paths, transcript files, meeting store, preferences, transcription languages, audio chunking, detection rules, Claude Desktop extension manifest and install detection (`ClaudeDesktopExtension.swift`), MCP client setup snippets (`AssistantSetup.swift`), rename/attendees/speaker/delete (`MeetingEditor.swift`) |
| `macos/Sources/MuesliCore/MCP/` | The MCP server: tools, stdio transport, audit log, `doctor` |
| `macos/Sources/muesli-mcp/` | Entry point of the `muesli-mcp` executable |
| `macos/Tests/MuesliCoreTests/` | Unit tests |
| `macos/Info.plist`, `macos/build.sh` | App bundle metadata and build script |
| `assets/brand/` | Logo, app icon and menu bar icon |
| `docs/CLAUDE_SETUP.md` | User-facing setup for Claude and other AI assistants |
| `docs/REQUIREMENTS.md` | Requirements and design decisions for the Claude integration |

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

## The Windows app (`windows/`)

The Windows app is written in Rust with Tauri. It also builds and runs on macOS, so it can be developed and tested from a Mac, but the macOS product is the Swift app. [docs/PLATFORMS.md](docs/PLATFORMS.md) lists how the two differ. Both read and write the same files and expose the same MCP tools, so a change to a file format or a tool must be made in both, and in both test suites.

| Crate | What it holds |
|---|---|
| `crates/core` | Models, preferences, `state.json`, languages, chunker, detection logic, store (append, finish, recover), editor, assistant setup and Claude Desktop config. No OS code. |
| `crates/mcp` | `muesli-mcp`, the MCP server (stdio) |
| `crates/whisper` | Model download and validation, whisper.cpp transcriber (`whisper-rs`) |
| `crates/audio` | Capture with `cpal`: microphone, WASAPI loopback (Windows), Core Audio tap (macOS). Resampling with `rubato`. |
| `crates/detect` | Samplers: Core Audio process list (macOS), WASAPI sessions (Windows), Teams process |
| `crates/notify` | Notifications with answer buttons: Windows toasts for the meeting prompts |
| `src-tauri` | The Tauri app: tray, windows, recording session, commands. Keep it thin. |
| `ui/` | The web UI (plain JS, no build step): Settings, onboarding, prompt |

```bash
cd windows
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p muesli-app            # the app (on a Mac: for development only; it shares the Swift app's data folder)
```

Install Rust with `brew install rustup && rustup default stable` (macOS) or from rustup.rs (Windows, plus the Visual Studio C++ build tools and CMake).

- **Formats stay compatible.** `fixtures/` at the repo root is the contract between the two apps: `fixtures/core` holds files written by the Swift encoders, which `crates/core/tests/golden.rs` loads; `fixtures/mcp` holds 33 requests and what the Swift `muesli-mcp` answered, which `crates/mcp/tests/contract.rs` replays. When a format or tool changes on purpose, run `scripts/regenerate-fixtures.sh` and make the Rust tests pass again.
- **Icons have one source.** `windows/src-tauri/icons/` is generated from `assets/brand` by `scripts/sync-icons.py`; CI fails if it is out of date. Never edit it by hand.
- **One version.** The Windows app's version is the workspace version in `windows/Cargo.toml`; the Swift app's is `MuesliInfo.version`. Keep them equal.
- **Paths are passed around** as a `Paths` value instead of read from the environment, so tests run in parallel. On Windows, models live in `%LOCALAPPDATA%\Muesli`, everything else in `%APPDATA%\Muesli`.
- **Use libraries for the hard parts**: `rubato` (resampling), `cpal` (audio devices), `whisper-rs`, `uuid`, `tempfile` (atomic writes), `trash`. Do not hand-roll replacements.
- **Try the UI without the app**: serve `windows/ui` and open `index.html?mock=1&window=onboarding` (or `settings`, `prompt`).
- **Check capture on a real machine**: `cargo run -p muesli-audio --example probe -- 8`. Check the resampler by ear or with Whisper: `--example resample_wav -- in.wav out.wav`. Transcribe real speech: the ignored test in `crates/whisper/tests/transcribe.rs`.
- **Windows without a PC**: `.github/workflows/ci.yml` runs the tests on macOS, Windows and Linux; **Build app** (`app.yml`) builds the installers. From a Mac, `rustup target add x86_64-pc-windows-msvc` then `cargo check --target x86_64-pc-windows-msvc -p muesli-core -p muesli-mcp -p muesli-audio -p muesli-detect -p muesli-notify` type-checks most Windows code; `src-tauri` and `whisper` need CI.

## Permissions and code signing

Grant Microphone, Calendar and System Audio Recording when asked. System Audio Recording (the "System Audio Recording Only" pane) is what allows capturing the other side's audio through a Core Audio tap; there is no public API to check it, so `Permissions.swift` reads the TCC status through the private TCC framework and skips the check if that is unavailable. This is the one private API in the app: re-check it on every major macOS release.

Without a signing certificate `build.sh` signs ad hoc, and macOS ties permissions for ad-hoc apps to the exact build, so each rebuild can invalidate them. If a permission looks enabled but still fails, remove Muesli from that list, or reset it, then relaunch and grant again:

```bash
tccutil reset Microphone com.github.bdhoine.muesli
tccutil reset AudioCapture com.github.bdhoine.muesli
```

Signing with a real certificate avoids this. `build.sh` uses the first "Apple Development" or "Developer ID Application" certificate in your keychain (override with `SIGN_IDENTITY="..." ./build.sh`). To get a free Apple Development certificate: Xcode → Settings → Accounts → add your Apple ID → Manage Certificates → + → Apple Development. After the first signed build, reset and grant the permissions once more; they then survive rebuilds.

## Conventions

- **Brand.** Follow [assets/brand/BRAND.md](assets/brand/BRAND.md) for the logo, colours, icons and tone. Use the asset files as they are; do not redraw or recolour them.
- **Minimal network.** The only request is the Whisper model download from Hugging Face (`ModelController`). Nothing listens on a port and `muesli-mcp` speaks stdio only.
- **Audio is never written to disk.**
- **Dependencies.** SwiftWhisper is pinned to an exact commit in `Package.swift`; bump it deliberately. Avoid adding others.
- **Pull requests.** Keep them focused, add tests for logic in `MuesliCore`, and run `swift test` before opening one.
