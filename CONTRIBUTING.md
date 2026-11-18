# Contributing to Muesli

Thanks for helping. This file is the developer documentation: how to build, test and find your way around. The product description is in [README.md](README.md).

## Repository layout

Muesli supports macOS and Windows, and the two apps are peers in this repository:

```
macos/          The macOS app (Swift): Package.swift, Sources/, Tests/, Info.plist, build.sh
windows/        The Windows app (Rust and Tauri): crates/, src-tauri/, ui/
fixtures/       Frozen file formats and MCP replies, checked by the Rust tests
assets/brand/   The one source for logo, icons and colours
scripts/        build-mcpb.py (Claude Desktop extension), build-ffi.sh (Rust core for Swift), sync-icons.py
docs/           REQUIREMENTS.md, PLATFORMS.md, CLAUDE_SETUP.md
```

The sections below describe the macOS app first, then [the Windows app](#the-windows-app-windows). Both are permanent; neither replaces the other.

## Build (macOS app)

Requires macOS 14.4+ and Xcode 15+.

```bash
macos/build.sh
open macos/Muesli.app
```

On first launch Muesli downloads the speech model (Large v3 Turbo, q5_0, 574 MB) and the voice activity model into `~/Library/Application Support/Muesli/`. Set `MUESLI_SUPPORT_DIR` to keep a test copy elsewhere.

`build.sh` runs `swift build -c release` and `cargo build --release -p muesli-mcp --target aarch64-apple-darwin` (the MCP server is the Rust binary from `windows/crates/mcp`, shared with the Windows app, so a Rust toolchain is required), assembles `Muesli.app`, copies `muesli-mcp` next to the main executable, builds `Muesli.icns` from the iconset, packs the Claude Desktop extension `Muesli.mcpb` (manifest from `muesli-mcp --mcpb-manifest`, the 512 px tile icon `extension-icon-512.png` (the app icon without its transparent margin, so Claude Desktop fills its box) and a signed copy of `muesli-mcp`) into `Contents/Resources`, and signs the bundle. Muesli needs a real bundle for notifications and permissions, so run the app, not the bare executable.

## Test

```bash
cd macos
swift test
```

The Swift tests cover `MuesliCore` and double as a check that the bindings behave: audio chunking, Whisper segment mapping, detection rules, Claude Desktop registration, and through the Rust core the transcript files, meeting store, crash recovery, preferences, `state.json`, paths and the transcripts-folder migration. The Rust logic itself is tested in `windows/crates/core`, `crates/mcp` and `crates/ffi` (`cargo test`). They need no microphone, calendar or network, and each test gets its own temporary data folders.

`MuesliCore` is Foundation plus the shared Rust core, so `swift build` and `swift test` need macOS and a one-time `scripts/build-ffi.sh` (run by `build.sh` too). That script builds `windows/crates/ffi` into the git-ignored `macos/Frameworks/MuesliFFI.xcframework` and the generated `macos/Sources/MuesliFFI/muesli_ffi.swift` (UniFFI). Logic moves from `MuesliCore` into `windows/crates/core` module by module; Done so far: `Languages`, `WhisperModel`, `AssistantClient`, `Meeting` and `TranscriptSegment`, `Paths`, `Preferences`, `AppStatus`, `Store` and `MeetingStore` (the meeting editor now exists only in Rust, used by `muesli-mcp`). `Meeting`, `Preferences`, `AppStatus` and `WhisperModel` are the generated types themselves; the other Swift types are thin wrappers, so the app code barely changed. Also shared: `DetectionLogic` (a `Detector` object), `AudioChunker` (a `Chunker` with a callback sink), `ClaudeDesktopConfig` and the extension state. Left in Swift on purpose: locale-dependent text (`Languages.displayName`, `RecentTranscript.subtitle`) and `WhisperModel.unused`. Transcription is shared too: `Transcriber.swift` calls the `muesli-whisper` crate (whisper.cpp through `whisper-rs`, with Metal) via `muesli-ffi`, built with the `whisper` feature, which needs `cmake` and the Xcode command line tools. `Package.swift` adds the app target on macOS only and links the system frameworks that whisper.cpp needs.

When you add logic, put it in `windows/crates/core` (and a wrapper in `windows/crates/ffi`) if it can live without AppKit, AVFoundation, EventKit or CoreAudio, and test it there with `cargo test`. `MuesliCore` and the app target stay thin shells.

### What is verified

- Transcription end to end, in local testing on macOS.
- `MuesliCore`: tested on macOS (no longer builds on Linux since it links the Rust core). `muesli-mcp` (Rust): tested with `cargo test`, including the real binary over stdio.
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

`DetectionLogic` (Rust, `crates/core/src/detection.rs`) holds the rules; `MeetingDetector` samples the system every 5 seconds and feeds it.

- A calendar event counts as a meeting when it has more than one attendee or mentions a known meeting host. It is suggested once per event.
- Mic activity counts as a call once another app has used a microphone for more than 10 seconds. Audio-only suggestions are at most 10 minutes apart, and a call is suggested once however long it lasts: it counts as handled once suggested (or recorded, or covered by a suggested calendar event), and only a microphone that stays free for the end delay (default 60 s, so a mute does not count) makes the next call a new one.
- During a recording, once another app has used the mic and then released it for `endDelaySeconds` (default 60), the call counts as ended. A recording in which no other app used the mic never ends by itself.
- Telling Muesli's own mic use from other apps' uses the CoreAudio process list, which needs macOS 14.2. Before that, the detector cannot judge the mic while Muesli records.

### Meeting lifecycle

`recording → finalizing → ready`, or `failed`. The meeting is saved to the store when recording starts and again after every transcribed chunk, so a crash loses at most the audio not yet transcribed. On launch, `MeetingStore.recoverInterrupted()` turns leftovers into `ready` with `partial: true`, or `failed` when nothing was transcribed. It then runs `MeetingStore.importTranscripts()`, which copies any `.json` in the transcripts folder whose id is not in the store yet (meetings recorded before the store existed), so `list_transcripts` sees them; it never overwrites a stored meeting.

## Changelog

`CHANGELOG.md` follows Keep a Changelog and lists only what a user can see or use. Add a line under `[Unreleased]` with every such change. To release: bump the version in `windows/Cargo.toml`, run `python3 scripts/sync-plugin.py`, update the fixtures that carry the version (`fixtures/mcp/expected.jsonl`), rename `[Unreleased]` to the version and date, add a new empty `[Unreleased]` and update the compare links.

## Claude plugin

The Claude Desktop extension (`Muesli.mcpb`, one click from Settings → AI Assistants) is kept as the primary route for Claude Desktop Chat; the plugin below is the route for Claude Code and Cowork. Directory listings of `.mcpb` are deprecated by Anthropic, local installs are not.

`plugin/` is a Claude plugin bundle (`.claude-plugin/plugin.json`, `.mcp.json`, one skill, README, license, icon), the only way Anthropic's directory takes a local MCP server now (`.mcpb` listings are deprecated). `.claude-plugin/marketplace.json` at the repo root lists it, so `/plugin install muesli --marketplace bdhoine/muesli` works. Plugin MCP commands take no per-OS variants and no home-folder variables, so `.mcp.json` starts `node ${CLAUDE_PLUGIN_ROOT}/server/muesli-mcp.js`, a small launcher that finds the app's `muesli-mcp` on macOS and Windows: the `MUESLI_MCP` variable, then `mcp-path` in the support folder (both apps write it on every start, `record_helper_path` in `crates/core/src/assistants.rs`), then the usual install folders. It needs Node.js. Test it with `node --test scripts/plugin-launcher.test.js` (CI runs it on Linux). `scripts/sync-plugin.py` keeps the plugin's version (the Cargo workspace version), icon and license in step; CI runs it with `--check`. Run `python3 scripts/sync-plugin.py` after bumping the version, and check with `claude plugin validate ./plugin --profile directory --strict` (and `claude plugin validate .` for the marketplace file). When a tool is added to or removed from `muesli-mcp`, update `plugin/skills/muesli/SKILL.md` and `plugin/README.md`.

The plugin is installed from this repo (`/plugin install muesli --marketplace bdhoine/muesli`), not from Anthropic's directory. The directory's portal (https://claude.ai/directory/manage, "Plugin bundle", repository `bdhoine/muesli`, plugin path `plugin`) was tried on 2026-11-17 and blocks the plugin: "Local MCP servers accepted only from Verified organizations". Anthropic documents no way to apply for that status, and the Verified badge cannot be requested, so the listing is parked. Its other findings were warnings (the directory-only listing fields in `plugin.json`, a note that `muesli` is also a GitHub account name). If Anthropic ever opens local servers to everyone, re-run Validate at that URL.

## Audio files

Two ways in, one code path (`muesli-import`). Files picked from the menu show a small progress window; files found in the drop folder (`dropFolder` in `config.json`, polled every 5 s by `DropFolder`, which only offers a file once its size and time stopped changing and never twice) only show a status line in the menu and Settings. A queue runs one file at a time and waits while a recording is going on; the Windows app also pauses between chunks (`Polite` in `files.rs`), the macOS app shares one loaded model with the recording (`AppState.sharedTranscriber`). An import that is cut off (the app quits or crashes) is not lost: its partial meeting (same title, starting at the file's date) is taken over by the next import of that file, which skips the 30 s chunks already transcribed and carries on in the same meeting; if several partial meetings of one file exist, the furthest is kept and the others are removed. After a folder file succeeds, `dropFolderCleanup` (`keep` | `trash` | `delete`) is applied; files picked by hand are never touched. The meeting has `source: "file"`, a single speaker `Speaker`, the file name as title and the file's modified time as its end; `state.json` says `transcribingFile` with a `fileJob` (name, progress, waiting) while one runs and nothing is recording. Test audio is in `fixtures/audio/` (one second of a tone per format); real speech: `MUESLI_TEST_MODEL`, `MUESLI_TEST_VAD` and `MUESLI_TEST_AUDIO` with `cargo test -p muesli-import --features whisper -- --ignored` or `swift test --filter AudioFileTests`.

## Files on disk

| Path | Written by | Contents |
|---|---|---|
| `~/Documents/MeetingTranscripts/` (configurable) | app | The `.md` and `.json` pair per finished meeting |
| `~/Library/Application Support/Muesli/ggml-*.bin` | the app (model download) or you | Whisper speech model and the Silero voice activity model; files from earlier versions (`ggml-small.bin`, `ggml-medium.bin`, `ggml-large-v3.bin`) can be removed in Settings |
| `…/Muesli/meetings/<id>.json` | app | Canonical copy of every meeting, rewritten during recording |
| `…/Muesli/state.json` | app | Current state and heartbeat, rewritten on change and every 10 s |
| `…/Muesli/config.json` | app | Preferences shared with `muesli-mcp` |
| `…/Muesli/imported.json` | app | Audio files from the drop folder that were already handled (path, size, modified time), so a file that is kept is not transcribed twice |
| `…/Muesli/audit.log` | `muesli-mcp` | One JSON line per management tool call (time, tool, meetingId); read calls are not logged |

Transcript JSON fields: `id`, `title`, `source` (calendar | teams | audio | manual), `startedAt`, `endedAt`, `attendees`, `state`, `partial`, `transcript[]` (`start`, `end`, `speaker`, `text`).

Environment variables: `TRANSCRIPT_DIR` overrides the transcripts folder; `MUESLI_SUPPORT_DIR` overrides the Application Support folder (used by the tests).

## Code layout

| Path | Purpose |
| --- | --- |
| `macos/Sources/Muesli/App.swift` | Menu bar UI, notifications, record/stop flow, `state.json` heartbeat |
| `macos/Sources/Muesli/MeetingDetector.swift` | Samples calendar, Teams and microphone use; feeds `DetectionLogic` |
| `macos/Sources/Muesli/AudioCapture.swift` | Mic and system audio capture, resampled to 16 kHz mono |
| `macos/Sources/Muesli/Transcriber.swift` | Wraps the Rust `Transcriber` (`crates/whisper`, whisper.cpp through `whisper-rs`): language detection limited to the configured languages, then transcription |
| `macos/Sources/Muesli/SettingsView.swift` | Settings window (sidebar: General, Transcription, AI Assistants), brand accent colours |
| `macos/Sources/MuesliCore/AssistantSetup.swift` | Setup command or config snippet per MCP client (Claude Code, Codex, Gemini CLI, VS Code, Cursor, other) |
| `macos/Sources/Muesli/Bridge.swift` | Claude Desktop extension status and install, or the `Muesli` entry in `claude_desktop_config.json` for accounts that block extensions |
| `macos/Sources/Muesli/OnboardingView.swift` | First-launch setup window (languages, assistants, permissions) |
| `macos/Sources/Muesli/Permissions.swift` | Microphone and System Audio Recording checks before recording |
| `macos/Sources/MuesliCore/` | Swift wrappers around the Rust core (data types, paths, transcript files, meeting store and preferences come from `windows/crates/ffi`), plus transcription languages, audio chunking, detection rules, Claude Desktop extension manifest and install detection (`ClaudeDesktopExtension.swift`), MCP client setup snippets (`AssistantSetup.swift`) |
| `windows/crates/mcp/` | The MCP server and the `muesli-mcp` executable (Rust, shared by both apps): tools, stdio transport, audit log, `doctor` |
| `macos/Tests/MuesliCoreTests/` | Unit tests |
| `macos/Info.plist`, `macos/build.sh` | App bundle metadata and build script |
| `assets/brand/` | Logo, app icon and menu bar icon |
| `docs/CLAUDE_SETUP.md` | User-facing setup for Claude and other AI assistants |
| `docs/REQUIREMENTS.md` | Requirements and design decisions for the Claude integration |

## The MCP server

```bash
windows/target/release/muesli-mcp       # stdio (cd windows && cargo build --release -p muesli-mcp)
windows/target/release/muesli-mcp doctor          # checks preferences, access switch, app heartbeat and meetings
```

It is a hand-written JSON-RPC server, speaking MCP protocol versions up to 2025-06-18. To try it by hand:

```bash
export MUESLI_SUPPORT_DIR=/tmp/muesli-dev
mkdir -p $MUESLI_SUPPORT_DIR && echo '{"mcpEnabled":true}' > $MUESLI_SUPPORT_DIR/config.json
echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"get_status","arguments":{}}}' | windows/target/release/muesli-mcp
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
| `crates/audio` | Resampling, microphone and system-audio capture (feature `capture`), and `decode_file`: any audio file (mp3, m4a/aac, flac, wav, ogg, alac) to 16 kHz mono blocks, with `symphonia` |
| `crates/import` | `import_file`: decode, 30 s chunks, transcribe, save as a meeting with source `file`; shared by both apps (macOS through `muesli-ffi`) |
| `crates/whisper` | Model download and validation (`ModelManager`, used by both apps), whisper.cpp transcriber (`whisper-rs`) with voice activity detection |
| `crates/audio` | Capture with `cpal`: microphone, WASAPI loopback (Windows), Core Audio tap (macOS). Resampling with `rubato`. |
| `crates/detect` | Samplers: Core Audio process list (macOS), WASAPI sessions (Windows), Teams process |
| `crates/system` | Dealings with Windows that need no audio: toasts with answer buttons for the meeting prompts, and the permission status shown in Settings |
| `src-tauri` | The Tauri app: tray, windows, recording session, commands. Keep it thin. |
| `ui/` | The web UI (plain JS, no build step): Settings, onboarding, prompt |

```bash
cd windows
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p muesli-app            # the app (on a Mac: for development only; it shares the Swift app's data folder)
```

Install Rust with `brew install rustup && rustup default stable` (macOS) or from rustup.rs (Windows, plus the Visual Studio C++ build tools and CMake).

- **Formats stay compatible.** `fixtures/` at the repo root freezes the formats: `fixtures/core` holds files written by the former Swift encoders, which `crates/core/tests/golden.rs` loads; `fixtures/mcp` holds 33 requests and the frozen replies (recorded from the former Swift server), which `crates/mcp/tests/contract.rs` replays. The MCP server is one Rust implementation for both apps, so a tool change is made once, in `crates/mcp`; update `fixtures/mcp/expected.jsonl` by hand when a reply changes on purpose. The file formats are one Rust implementation too; when one changes on purpose, update `fixtures/core` by hand and keep the golden tests passing.
- **Icons have one source.** `windows/src-tauri/icons/` is generated from `assets/brand` by `scripts/sync-icons.py`; CI fails if it is out of date. Never edit it by hand.
- **One version.** The Windows app's version is the workspace version in `windows/Cargo.toml`; the Swift app's is `MuesliInfo.version`. Keep them equal.
- **Paths are passed around** as a `Paths` value instead of read from the environment, so tests run in parallel. On Windows, models live in `%LOCALAPPDATA%\Muesli`, everything else in `%APPDATA%\Muesli`.
- **Use libraries for the hard parts**: `rubato` (resampling), `cpal` (audio devices), `whisper-rs`, `uuid`, `tempfile` (atomic writes), `trash`. Do not hand-roll replacements.
- **Try the UI without the app**: serve `windows/ui` and open `index.html?mock=1&window=onboarding` (or `settings`, `prompt`).
- **Check capture on a real machine**: `cargo run -p muesli-audio --example probe -- 8`. Check the resampler by ear or with Whisper: `--example resample_wav -- in.wav out.wav`. Transcribe real speech: the ignored test in `crates/whisper/tests/transcribe.rs`.
- **Windows without a PC**: `.github/workflows/ci.yml` runs the tests on Windows and Linux. It is started by hand (Actions tab, or `gh workflow run CI`). There are no macOS jobs because GitHub's macOS runners are not available to this private repository: run `swift test` and `macos/build.sh` locally; **Build app** (`app.yml`) builds the installers. From a Mac, `rustup target add x86_64-pc-windows-msvc` then `cargo check --target x86_64-pc-windows-msvc -p muesli-core -p muesli-mcp -p muesli-audio -p muesli-detect -p muesli-system` type-checks most Windows code; `src-tauri` and `whisper` need CI.

## Permissions and code signing

Grant Microphone, Calendar and System Audio Recording when asked. System Audio Recording (the "System Audio Recording Only" pane) is what allows capturing the other side's audio through a Core Audio tap; there is no public API to check it, so `Permissions.swift` reads the TCC status through the private TCC framework and skips the check if that is unavailable. This is the one private API in the app: re-check it on every major macOS release.

Without a signing certificate `build.sh` signs ad hoc, and macOS ties permissions for ad-hoc apps to the exact build, so each rebuild can invalidate them. If a permission looks enabled but still fails, remove Muesli from that list, or reset it, then relaunch and grant again:

```bash
tccutil reset Microphone com.github.bdhoine.muesli
tccutil reset AudioCapture com.github.bdhoine.muesli
```

Signing with a real certificate avoids this. `build.sh` uses the first "Apple Development" or "Developer ID Application" certificate in your keychain (override with `SIGN_IDENTITY="..." ./build.sh`). To get a free Apple Development certificate: Xcode → Settings → Accounts → add your Apple ID → Manage Certificates → + → Apple Development. After the first signed build, reset and grant the permissions once more; they then survive rebuilds.

## Resetting the app for testing

To see first launch again (onboarding, permission prompts, model download), quit Muesli, then reset what it keeps. Every step is optional; do only the ones you need.

**macOS**

```bash
# 1. Preferences (incl. the onboarding flag), state and meetings; keeps the models, so no 574 MB download
find ~/Library/Application\ Support/Muesli -mindepth 1 -maxdepth 1 ! -name 'ggml-*.bin' -exec rm -rf {} +
# 2. Also forget the models (forces the download again)
rm -rf ~/Library/Application\ Support/Muesli
# 3. Only needed if you ran a version before 0.0.2 (it kept the transcripts folder here)
defaults delete com.github.bdhoine.muesli
# 4. Permissions, so macOS asks again
tccutil reset Microphone com.github.bdhoine.muesli
tccutil reset AudioCapture com.github.bdhoine.muesli
tccutil reset Calendar com.github.bdhoine.muesli
# or every privacy grant at once (microphone, system audio, calendar, ...)
tccutil reset All com.github.bdhoine.muesli
```

`tccutil` only clears privacy grants, and the app must be quit first. Two grants live elsewhere: notifications (System Settings → Notifications → Muesli; macOS cannot reset them from the command line, and the entry disappears once you delete the app) and the login item (System Settings → General → Login Items, switch Muesli off).

- Finished transcripts live in `~/Documents/MeetingTranscripts/` (or the folder in `config.json`) and are not touched; remove them by hand for a clean slate. Meetings there are imported again on the next launch, so delete them if you want an empty list.
- Muesli registers itself as a login item again at every launch, so switch it off after a test run if you do not want it to start at login.
- For a throwaway run that never touches your real data, start the app with `MUESLI_SUPPORT_DIR=/tmp/muesli-test TRANSCRIPT_DIR=/tmp/muesli-test/transcripts open -n macos/Muesli.app` (or run the binary directly). Permissions are still tied to the app, so step 4 applies.
- To test the model download without waiting, remove only `ggml-large-v3-turbo-q5_0.bin` and `ggml-silero-v5.1.2.bin`; the app downloads whatever is missing at launch.

**Windows** (PowerShell, with the app closed)

```powershell
Remove-Item -Recurse -Force "$env:APPDATA\Muesli"        # config, state, meetings
Remove-Item -Recurse -Force "$env:LOCALAPPDATA\Muesli"   # models (forces the download again)
```

Windows has no system calendar and needs no system-audio permission, so there are no permissions to reset; only the microphone setting under Settings → Privacy & security → Microphone remembers the app. This was not run on Windows.

## Conventions

- **Brand.** Follow [assets/brand/BRAND.md](assets/brand/BRAND.md) for the logo, colours, icons and tone. Use the asset files as they are; do not redraw or recolour them.
- **Minimal network.** The only requests are the downloads of the Whisper model and the voice activity model from Hugging Face (the Rust `ModelManager`). Nothing listens on a port and `muesli-mcp` speaks stdio only.
- **Audio is never written to disk.**
- **Dependencies.** Swift has none: everything else is the Rust workspace (`Cargo.lock` pins it). Avoid adding more.
- **Pull requests.** Keep them focused, add tests for logic in `MuesliCore`, and run `swift test` before opening one.
