# Contributing to Muesli

Thanks for helping. This file is the developer documentation: how to build, test and find your way around. The product description is in [README.md](README.md); the requirements are in [docs/REQUIREMENTS.md](docs/REQUIREMENTS.md).

## Repository layout

Muesli is one app for macOS and Windows: a Tauri shell around a Rust engine, with a web UI.

```
Cargo.toml        The Rust workspace
crates/           core (data, rules, ports), engine (behaviour, view states, intents), mcp, whisper,
                  aec (LocalVQE, a git submodule), audio, import, platform-common, platform-macos,
                  platform-windows, platform-fake, port-contracts
apps/desktop/     The Tauri app: picks the adapters, draws the tray and the windows. Thin.
apps/sim/         muesli-sim: the engine with fakes and the real UI in a browser; the UI tests
ui/               The web UI (plain JS and CSS, no build step), the same look on macOS and Windows
fixtures/         Frozen file formats and MCP replies, audio samples, simulator scenarios
assets/brand/     The one source for logo, icons and colours
scripts/          build-mcpb.py (Claude Desktop extension), fetch-aec-model.py, fetch-vulkan-loader.py,
                  sync-icons.py, sync-plugin.py
plugin/           The Claude plugin
docs/             REQUIREMENTS.md, PLATFORMS.md, CLAUDE_SETUP.md
macos/            The retired SwiftUI app. Kept until the Tauri app is verified on a Mac; not built.
```

## Architecture

```
            ui/ (web)                     view states ◄──┐   ┌── intents
                │                                        │   │
apps/desktop (Tauri)  ── Host, Tray, Updater ──►  crates/engine (muesli-engine) ── SpeechModel ──► crates/whisper
                                              │ ports (crates/core/src/ports.rs)
                 ┌────────────────────────────┼────────────────────────────┐
         platform-macos               platform-windows               platform-fake
   (EventKit, UNUserNotification,   (WASAPI, toasts, registry)   (tests, simulator, Linux dev)
    AVFoundation, SMAppService)
                 └──── platform-common (clock, folders, trash, Teams scan, cpal) ────┘
```

- **The core never calls the OS.** Ports in `crates/core/src/ports.rs`: `Clock`, `MicCapture`, `SystemAudioCapture`, `MeetingDetector`, `Notifier`, `Permissions`, `Tray`, `Trash`, `PlatformPaths`, `LoginItem`, plus `PlatformWords` (Trash/Recycle Bin, Finder/Explorer, Mac/PC, menu bar/system tray, Start at login/Start when I sign in). `Platform` bundles one adapter of each. `Updater` is the one port outside the bundle: the shell provides it (`apps/desktop/src/updater.rs`, `tauri-plugin-updater`, the same on both platforms) and passes it in `Options`; debug builds pass none and never check for updates.
- **The engine decides.** `crates/engine` owns what Muesli does: detection prompts, transcribing, the end-of-call question with its 5-minute timeout, the prompt window when notifications cannot ask, audio-file imports, the main window (list, reader, rename, delete with undo), Settings and setup. `Engine::view(window)` gives a window its view state as JSON; `Engine::intent` takes what the user did; `Engine::menu` is the tray menu; `Engine::tick` runs the timers once a second. `Host` is what the shell does for it (windows, pickers, reveal, quit; implemented by `apps/desktop/src/host.rs` and the simulator).
- **The UI renders.** `ui/app.js` draws the view it gets and sends intents; `ui/bridge.js` talks to Tauri (`view`, `intent`, the `state-changed` event) or to the simulator (HTTP). Both platforms look the same; `words.skin` ("mac" or "windows") only picks the Settings shortcut (⌘, or Ctrl+,) and whether the page draws the Mac title bar strip. Dark mode follows the system.
- **Contracts.** `crates/port-contracts` has one suite per port. `tests/fakes.rs` runs them against `platform-fake`; `tests/native.rs` against the native adapters of the OS it runs on.

| Crate | What it holds |
|---|---|
| `crates/core` | Models, preferences (with the migration of old keys), `state.json`, languages, chunker (fixed blocks) and segmenter (live blocks that follow pauses, `SpeechDetector`, `Pacer`), echo stage (`EchoCanceller`, `EchoStage`: lines the system audio up with the microphone, passthrough and fail-safe), detection rules, store (append, finish, recover), editor, assistant setup and Claude Desktop config, the ports |
| `crates/engine` | The behaviour, the view states (`views.rs`), the intents, the live audio worker (`live.rs`: capture callbacks hand samples to it, it owns the echo stage and the segmenters), fakes for the speech model and the host |
| `crates/mcp` | `muesli-mcp`, the MCP server (stdio) |
| `crates/audio` | Resampling (`rubato`) and `decode_file` (any audio file to 16 kHz mono, with `symphonia`). No device code. |
| `crates/import` | `import_file`: decode, 30 s chunks, transcribe, save as a meeting with source `file` |
| `crates/aec` | `muesli-aec`: LocalVQE (`vendor/LocalVQE`, a git submodule at a pinned commit) as the `EchoCanceller`. Feature `localvqe` builds its shared library with CMake; `LocalVqe::load` opens it at run time, never linked, because its ggml would clash with whisper.cpp's. The app turns the feature on for macOS and Windows. |
| `crates/whisper` | Model download and validation (`ModelManager`, deletes old model files), whisper.cpp transcriber with voice activity detection, `SileroDetector` for the live segmenter |
| `crates/platform-common` | Clock, folders (`dirs`), trash (`trash`), Teams process scan (`sysinfo`), microphone and system audio (`cpal`, feature `capture`: WASAPI loopback on Windows, Core Audio tap on macOS) |
| `crates/platform-windows` | WASAPI sessions (mic in use), toasts with buttons, privacy switches, the Run key for start at login. Compiles to nothing elsewhere. |
| `crates/platform-macos` | Core Audio process list and EventKit (detection), UNUserNotificationCenter with a delegate, permissions (AVFoundation, EventKit, notification settings), NSFileManager trash, SMAppService, SF Symbols on the menu bar menu (`menu_icons.rs`, added when the menu opens because Tauri menus cannot show template images). Compiles to nothing elsewhere. |
| `crates/platform-fake` | A drivable fake for every port: manual clock, scripted detector, fed or file-played audio, answerable notifications, folder trash, an updater with a fake GitHub (no release, a newer version, progress, a bad signature, a network error) |
| `crates/port-contracts` | The contract suites |
| `apps/desktop` | Tauri: adapters for the tray (`tray.rs`), windows (`windows.rs`, incl. the macOS Dock policy), dialogs and opening files (`host.rs`), whisper and the bundled echo canceller (`speech.rs`), updates (`updater.rs`); wiring in `lib.rs` |
| `apps/sim` | The simulator and the UI tests |

## Build and run

You need [Rust](https://rustup.rs) and CMake (whisper.cpp, LocalVQE). Clone with `git clone --recursive`, or run `git submodule update --init --recursive` once: LocalVQE is a submodule (its own build patches its ggml in place; `.gitmodules` ignores that). Run `python3 scripts/fetch-aec-model.py` once for the echo cancellation model (2.9 MB, checked by SHA-256, never committed); without it a debug build warns and transcribes the microphone as it is, and a release build stops. On Windows also the Visual Studio C++ build tools and the [Vulkan SDK](https://vulkan.lunarg.com/sdk/home) (`VULKAN_SDK` set; whisper.cpp builds its Vulkan backend), and run `python3 scripts/fetch-vulkan-loader.py` once: the app bundles `vulkan-1.dll` next to `muesli.exe` (`tauri.windows.conf.json`), so it also starts on a PC without a GPU driver and then transcribes on the CPU; on macOS the Xcode command line tools; on Linux WebKitGTK (`libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev`).

```bash
cargo run -p muesli-app                      # the app (on Linux: real folders, fakes for audio and the OS)
cd apps/desktop && cargo tauri build         # an installer (.dmg on macOS, .msi/.exe on Windows)
scripts/install-mac.sh                       # macOS: build a signed release app and install it in ~/Applications
cargo run -p muesli-sim -- --skin windows --scenario call-in-progress   # the simulator
```

On first launch Muesli downloads the speech model (Large v3 Turbo, q5_0, 574 MB) and the voice activity model into `~/Library/Application Support/Muesli/` (macOS) or `%LOCALAPPDATA%\Muesli` (Windows). Set `MUESLI_SUPPORT_DIR` to keep a test copy elsewhere. On macOS notifications need an app bundle: under `cargo run` the questions appear in Muesli's prompt window.

`cargo tauri build` for release uses `--config tauri.release.conf.json`, which bundles LocalVQE's library, its model and licence (`apps/desktop/resources/aec/`, filled by `apps/desktop/build.rs`), `muesli-mcp` (from `apps/desktop/binaries/muesli-mcp-<target>`) and the Claude Desktop extension (`dist/Muesli.mcpb`, from `python3 scripts/build-mcpb.py target/<target>/release/muesli-mcp`) and signs ad hoc on macOS. It also holds the updater's settings (endpoint, public key, Windows install mode) and makes the signed updater bundles, which need `TAURI_SIGNING_PRIVATE_KEY`; for a local build without the key add `--config '{"bundle":{"createUpdaterArtifacts":false}}'`. The **Release** workflow (`.github/workflows/release.yml`) does all of it for both platforms.

## Test

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build -p muesli-sim && cd apps/sim && npm ci && npx playwright install chromium && node --test tests/*.test.mjs
```

- The engine tests (`crates/engine/tests`) drive the fakes with a manual clock: prompts and their wording, the 60 s and 5 minute timings, the prompt-window fallback, the live transcript, rename/delete/undo, imports, setup on both skins, Settings, the config migration.
- The UI tests (`apps/sim/tests`) run Chromium against the simulator for both skins. The simulator serves `ui/` with the app's Content Security Policy (read from `apps/desktop/tauri.conf.json`), and a test fails on any violation, so something the app would block (a `data:` image, an inline script) cannot pass in the browser and break in the app. `design.test.mjs` measures the macOS skin against the design canvas ("Muesli – macOS app", a claude.ai design artifact): sizes, colours and what each window shows for setup, the main window (idle and transcribing) and Settings. When the design changes, update the numbers there along with the UI.
- On a Mac or PC: `cargo test -p muesli-port-contracts --test native`, and `-- --ignored` for the contracts that need a microphone, something playing, a notification or that change the login item.
- **Tests** (`.github/workflows/tests.yml`) runs clippy and the Rust tests on macOS, Windows and Linux, the UI tests, and the checks that generated files (icons, plugin) are current. It runs on every push and pull request (and by hand: Actions tab, or `gh workflow run Tests`). **Release** (`release.yml`), started by hand from `main`, tags the version in `Cargo.toml`, builds the Windows installers and the macOS disk image into a draft release and moves `main` to the next version; with `test_build` it only builds installers as artifacts (see [Releasing](#releasing)). A macOS job can wait a long time for a runner and is cancelled after 15 minutes in the queue; run it again.
- Check capture on a real machine: `cargo run -p muesli-platform-common --features capture --example probe -- 8`. Check the resampler by ear or with Whisper: `cargo run -p muesli-audio --example resample_wav -- in.wav out.wav`. Real speech: the ignored tests in `crates/whisper/tests/transcribe.rs` and `crates/import/tests` (`MUESLI_TEST_MODEL`, `MUESLI_TEST_VAD`, `MUESLI_TEST_AUDIO`). Echo cancellation with the real model: `MUESLI_AEC_NEAR_WAV=near.wav MUESLI_AEC_FAR_WAV=far.wav cargo test --release -p muesli-aec --features localvqe -- --ignored` (two voices, 16 kHz mono WAV; on a Mac `say -o near.wav --data-format=LEI16@16000 "…"`).

### The simulator

`cargo run -p muesli-sim -- --skin mac --scenario a-day-of-meetings`, then open the address it prints. It runs the real engine with the fake adapters and shows the real `ui/` pages, next to a fake menu bar or tray (with the red dot), its notifications (answerable) and buttons for the outside world: a call starts, people talk, the call ends, time passes, a file is dropped on Muesli's window, the model download progresses or fails, notifications are switched off. Muesli's windows open as frames. Switch the skin or the scenario at the top. Scenarios are JSON files in `fixtures/scenarios/` (`setupDone`, `model`, `permissions`, `release` and `updateError` for the fake GitHub, `lastRunVersion`, `echoCancelling` for the stand-in echo canceller, and a list of steps; the steps are the same actions as the buttons, see `apps/sim/src/world.rs`; a `talk` step with `"speakers": true` lets the microphone hear the others, as without headphones).

## How it works

### Detection

`DetectionLogic` (`crates/core/src/detection.rs`) holds the rules; the engine samples the `MeetingDetector` port every 5 seconds and feeds it.

- A calendar event (macOS) counts as a meeting when it has other attendees or mentions a known meeting host (Teams, Zoom, Google Meet, Webex). Each occurrence is suggested once.
- Mic activity counts as a call once another app has used a microphone for more than 10 seconds. Audio-only suggestions are at most 10 minutes apart, and a call is suggested once however long it lasts: it counts as handled once suggested (or transcribed, or covered by a suggested calendar event), and only a microphone that stays free for 60 s (so a mute does not count) makes the next call a new one.
- While transcribing, once another app has used the mic and then released it for 60 s, the call counts as ended and Muesli asks whether to stop. No answer within 5 minutes: it stops, saves and says so. A transcription in which no other app used the mic never ends by itself.
- Telling Muesli's own mic use from other apps' uses the Core Audio process list (macOS 14.2+) or the WASAPI sessions of the default capture device (Windows).

### Meeting lifecycle

`recording → finalizing → ready`, or `failed` (the values in the files keep these names). The meeting is saved to the store when transcribing starts and again after every transcribed chunk, so a crash loses at most the audio not yet transcribed. On launch, `MeetingStore::recover_interrupted` turns leftovers into `ready` with `partial: true`, or `failed` when nothing was transcribed. It then runs `MeetingStore::import_transcripts`, which copies any `.json` in the transcripts folder whose id is not in the store yet; it never overwrites a stored meeting.

### End-of-meeting pass and vocabulary

The live consumer keeps every block it transcribed (16-bit, in memory) and hands back the loaded model when the meeting stops. `final_pass.rs` regroups the blocks per speaker into chunks of up to 30 s and transcribes them again with beam search and a prompt; the result replaces the live text. The engine's `Activity::Improving` shows progress and lets Start and Quit cancel it. Every chunk carries its prompt (`AudioChunk::prompt`) and whether to decode carefully (`best`): `vocabulary::whisper_prompt` builds it from `vocabulary.json` and the meeting's title and attendees. The fake transcriber answers `best` chunks with "Improved: …" sentences and records every prompt, so the engine tests see both.

### Echo cancellation

Without headphones the microphone hears the call. The live worker sends both streams through `EchoStage` (`crates/core/src/aec.rs`) before the microphone's segmenter: the system audio is the reference. The stage estimates the echo delay (cross-correlation of 10 ms loudness envelopes, −300 to +800 ms, re-checked every second; a changed delay must win three times), shifts the reference by it (LocalVQE only aligns the last few tens of milliseconds itself), fills gaps where the system audio delivered nothing (Windows loopback is silent between sounds), and restarts the canceller after a change. It passes the microphone through untouched while the far end has been quiet for 2 s, and drops the canceller for the rest of the meeting on an error or when it uses more than half of real time. The canceller is LocalVQE (`crates/aec`, one CPU thread, about 12% of an M1 core); measurements are in `docs/AUDIO-SPIKE.md`. The text filter (`echo.rs`) still removes what slips through. Tests use `StandInCanceller`, which subtracts the reference at the lag and gain it learns.

### Audio files

One code path (`muesli-import`). Files come from **Transcribe Audio File…** (menu or main window) or are dropped on a Muesli window. A queue runs one file at a time, after the speech model is ready and while nothing is being transcribed live; progress shows in the main window's list, with Cancel. Files are never moved or deleted. An import that is cut off (the app quits) is resumed by the next import of that file, in the same meeting. The meeting has `source: "file"`, a single speaker `Speaker`, the file name as title and the file's modified time as its start; `state.json` says `transcribingFile` with a `fileJob` while one runs. Test audio is in `fixtures/audio/`.

## Files on disk

| Path | Written by | Contents |
|---|---|---|
| `~/Documents/MeetingTranscripts/` (configurable) | app | The `.md` and `.json` pair per finished meeting |
| `<support>/ggml-*.bin` (`<models>` on Windows) | app (model download) | Whisper speech model and the Silero voice activity model. Model files of earlier versions are deleted on start. |
| `<support>/ggml-large-v3-turbo-encoder.mlmodelc/` (macOS) | app (model download, unpacked from a zip) | The Core ML encoder: whisper.cpp runs the encoder on the Neural Engine when this folder sits next to the speech model, Metal otherwise |
| `<support>/meetings/<id>.json` | app | Canonical copy of every meeting, rewritten while transcribing |
| `<support>/state.json` | app | Current state and heartbeat, rewritten on change and every 10 s |
| `<support>/config.json` | app | Preferences shared with `muesli-mcp`: `mcpEnabled`, `claudeDesktopConfigEntry`, `languages`, `onboardingDone`, `transcriptsFolder`, `extensionUpdateNotified`, `extensionUpdateDismissed`, and for updates `autoUpdateCheck` (default true), `updateLastChecked`, `updateNotified`, `lastRunVersion`. Keys of earlier versions are dropped on start (`Preferences::migrate`). |
| `<support>/vocabulary.json` | app (Settings) and `muesli-mcp` (`update_vocabulary`) | `schemaVersion`, `terms`: names and terms for Whisper's prompt. Read on every use, never cached, because both write it |
| `<support>/mcp-path` | app | Where `muesli-mcp` is, for the Claude plugin |
| `<support>/audit.log` | `muesli-mcp` | One JSON line per management tool call (time, tool, meetingId); read calls are not logged |

`<support>` is `~/Library/Application Support/Muesli` (macOS) or `%APPDATA%\Muesli` (Windows); models on Windows are in `%LOCALAPPDATA%\Muesli`.

Transcript JSON fields: `id`, `title`, `source` (calendar | teams | audio | manual | file), `startedAt`, `endedAt`, `attendees`, `state`, `partial`, `transcript[]` (`start`, `end`, `speaker`, `text`).

Environment variables: `TRANSCRIPT_DIR` overrides the transcripts folder (Settings then cannot change it); `MUESLI_SUPPORT_DIR` overrides the support folder (also used for the models).

## The MCP server

```bash
target/release/muesli-mcp       # stdio (cargo build --release -p muesli-mcp)
target/release/muesli-mcp doctor          # checks preferences, access switch, app heartbeat and meetings
```

It is a hand-written JSON-RPC server, speaking MCP protocol versions up to 2025-06-18. To try it by hand:

```bash
export MUESLI_SUPPORT_DIR=/tmp/muesli-dev
mkdir -p $MUESLI_SUPPORT_DIR && echo '{"mcpEnabled":true}' > $MUESLI_SUPPORT_DIR/config.json
echo '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"get_status","arguments":{}}}' | target/release/muesli-mcp
```

Rules to keep:

- No tool may start or stop transcribing or change settings; the vocabulary (`update_vocabulary`) is the one shared list it may change. Management tools only touch meetings in state `ready` or `failed`, never overwrite another meeting's files, and delete by moving to the Trash. Read tools are annotated `readOnlyHint`, delete as `destructiveHint`.
- The stdio server refuses tool calls unless access is switched on.
- Management tool calls are written to `audit.log`.

## Claude plugin

The Claude Desktop extension (`Muesli.mcpb`, one click from Settings → AI Assistants) is kept as the primary route for Claude Desktop Chat; the plugin below is the route for Claude Code and Cowork. Directory listings of `.mcpb` are deprecated by Anthropic, local installs are not.

`plugin/` is a Claude plugin bundle (`.claude-plugin/plugin.json`, `.mcp.json`, one skill, README, license, icon), the only way Anthropic's directory takes a local MCP server now (`.mcpb` listings are deprecated). `.claude-plugin/marketplace.json` at the repo root lists it, so `/plugin install muesli --marketplace bdhoine/muesli` works. Plugin MCP commands take no per-OS variants and no home-folder variables, so `.mcp.json` starts `node ${CLAUDE_PLUGIN_ROOT}/server/muesli-mcp.js`, a small launcher that finds the app's `muesli-mcp` on macOS and Windows: the `MUESLI_MCP` variable, then `mcp-path` in the support folder (the app writes it on every start, `record_helper_path` in `crates/core/src/assistants.rs`), then the usual install folders. It needs Node.js. Test it with `node --test scripts/plugin-launcher.test.js` (CI runs it on Linux). `scripts/sync-plugin.py` keeps the plugin's version (the Cargo workspace version), icon and license in step; CI runs it with `--check`. Run `python3 scripts/sync-plugin.py` after bumping the version, and check with `claude plugin validate ./plugin --profile directory --strict` (and `claude plugin validate .` for the marketplace file). When a tool is added to or removed from `muesli-mcp`, update `plugin/skills/muesli/SKILL.md` and `plugin/README.md`.

The plugin is installed from this repo (`/plugin install muesli --marketplace bdhoine/muesli`), not from Anthropic's directory. The directory's portal (https://claude.ai/directory/manage, "Plugin bundle", repository `bdhoine/muesli`, plugin path `plugin`) was tried on 2026-11-17 and blocks the plugin: "Local MCP servers accepted only from Verified organizations". Anthropic documents no way to apply for that status, and the Verified badge cannot be requested, so the listing is parked. Its other findings were warnings (the directory-only listing fields in `plugin.json`, a note that `muesli` is also a GitHub account name). If Anthropic ever opens local servers to everyone, re-run Validate at that URL.

## Changelog

`CHANGELOG.md` follows Keep a Changelog and lists only what a user can see or use. Add a line under `[Unreleased]` with every such change. The Release workflow turns `[Unreleased]` into the version being released with its date and updates the compare links (`scripts/release.py changelog`); nothing else in the repository carries the version by hand (tests read it from `Cargo.toml`, `fixtures/mcp/expected.jsonl` says `VERSION`).

## Releasing

Muesli updates itself from the GitHub release marked Latest (`latest.json`, see `docs/REQUIREMENTS.md` UP1 to UP9). Updates are signed with a Tauri updater key; the app checks the signature against the public key in `apps/desktop/tauri.release.conf.json`.

`scripts/release.sh` does the steps below from your Mac: `setup` (once: updater key, its secrets and public key, and the macOS certificate secrets from `--certificate FILE.p12` or `~/muesli.p12`: it checks the password and that the file holds the private key, takes the signing identity from the certificate, and offers to delete the file; steps already done are skipped, so run it again to replace the certificate), `run` (checks that `main` is pushed and clean, the key and secrets are in place and the version is not tagged yet, asks, starts **Release**, follows it and lists the draft's files), `rebuild VERSION` and `test-build`. By hand:

Once, before the first release (the pubkey there is a `TODO(barry)` placeholder, and the Release workflow refuses to run until it is replaced):

1. `cargo tauri signer generate -w ~/.tauri/muesli.key` and choose a password. Keep the private key and password safe (a password manager); losing them means existing installs can never be updated again.
2. Add the secrets `TAURI_SIGNING_PRIVATE_KEY` (the contents of `~/.tauri/muesli.key`) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (Settings → Secrets and variables → Actions, or `gh secret set TAURI_SIGNING_PRIVATE_KEY < ~/.tauri/muesli.key` and `gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD`).
3. Replace the `pubkey` value in `apps/desktop/tauri.release.conf.json` with the contents of `~/.tauri/muesli.key.pub` and commit it.
4. Optional, for the macOS signature: the certificate secrets (see [Permissions and code signing](#permissions-and-code-signing)).

Each release:

1. Make sure `[Unreleased]` in `CHANGELOG.md` says what is new, and that **Tests** passed on `main`.
2. Run **Release** from `main` (Actions tab, or `gh workflow run Release --ref main`). It first runs the whole **Tests** workflow on `main` and stops if anything fails. Then it dates the changelog section and commits it ("Release <version>"), tags `main` with `v<version>` (the version in `Cargo.toml`), creates a draft release titled `v<version>` with that section as its notes, builds Windows (`.exe`, `.msi`) and macOS (`.dmg`) with the signed updater bundles and `latest.json`, attaches them to the draft, checks that the draft has `latest.json` and the signed bundles, and then moves `main` to the next patch version ("Start <next>"). If a build fails, nothing is bumped: fix it, push, and run Release again; it sees the tag and the draft and builds again for them (re-running the failed jobs would use the old workflow file). The Windows app builds without `--target` and in `C:\t` (`CARGO_TARGET_DIR`): longer paths take whisper.cpp's Vulkan shader build past Windows' 260-character path limit. Both platforms build whisper.cpp with `GGML_NATIVE=OFF`, so the installers do not depend on the build machine's processor.
3. Check the draft (try the installers), then publish it as Latest. From then on every Muesli that checks gets the update. Never mark a test build as Latest; pre-releases are ignored by the updater.

If a release ended up without its files (0.0.2 did: the draft's id was looked up before GitHub listed it, and the builds uploaded nothing), run **Release** with **rebuild** set to its version (`scripts/release.sh rebuild 0.0.2`, or `gh workflow run Release --ref main -f rebuild=0.0.2`): it builds that tag again and attaches the files to its release, draft or published, without tagging or bumping. Until a published release has `latest.json`, installed apps find no update.

Run with **test_build** (`gh workflow run Release -f test_build=true`), the workflow only builds installers as artifacts, without updater bundles, tag or release. The updater is off in debug builds (`cargo run`), so test it with release builds: install version N, publish N+1, start N and wait 30 s (or use Check for Updates in Settings).

## Permissions and code signing

macOS asks for Microphone, System Audio Recording, Calendar and Notifications, each only when the user presses **Allow…** in setup or Settings; when that does not end in "allowed", the engine opens the matching System Settings pane. The app is signed with hardened runtime, which refuses the microphone and the calendar unless the signature carries their entitlements: `apps/desktop/Entitlements.plist` (`bundle.macOS.entitlements` in `tauri.conf.json`). Check a build with `codesign -d --entitlements - Muesli.app`. System Audio Recording (the "System Audio Recording Only" pane) allows capturing the other side's audio through a Core Audio tap; there is no public API to read its status, so `platform-macos/src/tcc.rs` reads it through the private TCC framework (`TCCAccessPreflight`, `TCCAccessRequest`), as the Swift app did, and falls back to opening the tap when that is unavailable. This is the one private API in Muesli: re-check it on every major macOS release.

CI and `cargo tauri build` sign ad hoc, and macOS ties permissions for ad-hoc apps to the exact build, so each rebuild can invalidate them. If a permission looks enabled but still fails, reset it, relaunch and grant again:

```bash
tccutil reset Microphone com.github.bdhoine.muesli
tccutil reset AudioCapture com.github.bdhoine.muesli
tccutil reset Calendar com.github.bdhoine.muesli
```

Signing with a real certificate avoids this: permissions are then tied to the certificate, not the build. The Release workflow signs the macOS app with the certificate in the secrets `APPLE_CERTIFICATE` (the `.p12`, base64), `APPLE_CERTIFICATE_PASSWORD` and `APPLE_SIGNING_IDENTITY` (its name, as `security find-identity -v -p codesigning` prints it), and ad hoc when they are not set. To add them: in Keychain Access, My Certificates, export the certificate with its private key as a `.p12` with a password, then `base64 -i muesli.p12 | gh secret set APPLE_CERTIFICATE`, `gh secret set APPLE_CERTIFICATE_PASSWORD` and `gh secret set APPLE_SIGNING_IDENTITY`, and delete the `.p12`. An Apple Development certificate keeps permissions across updates; only a Developer ID Application certificate with notarisation (the paid Apple Developer Program) also stops macOS from blocking the downloaded app on first open. For a local build use your own from the keychain (`security find-identity -v -p codesigning` lists them):

```bash
APPLE_SIGNING_IDENTITY="Apple Development: you@example.com (TEAMID)" cargo tauri build --bundles app --config tauri.release.conf.json --config '{"bundle":{"createUpdaterArtifacts":false}}'
``` The same applies to updates: each update is a new ad-hoc signed build, so macOS probably asks for the microphone and system audio again; Muesli notices on the first start after an update and opens the permissions screen (UP7).

## Resetting the app for testing

Quit Muesli first. Every step is optional.

**macOS**

```bash
# 1. Preferences (incl. the onboarding flag), state and meetings; keeps the models
find ~/Library/Application\ Support/Muesli -mindepth 1 -maxdepth 1 ! -name 'ggml-*.bin' -exec rm -rf {} +
# 2. Also forget the models (forces the download again)
rm -rf ~/Library/Application\ Support/Muesli
# 3. Every privacy grant, so macOS asks again
tccutil reset All com.github.bdhoine.muesli
```

Notifications (System Settings → Notifications → Muesli) and the login item (System Settings → General → Login Items) cannot be reset from the command line.

**Windows** (PowerShell)

```powershell
Remove-Item -Recurse -Force "$env:APPDATA\Muesli"        # config, state, meetings
Remove-Item -Recurse -Force "$env:LOCALAPPDATA\Muesli"   # models (forces the download again)
```

Finished transcripts in the transcripts folder are not touched by either; they are imported again on the next start. For a throwaway run, set `MUESLI_SUPPORT_DIR` and `TRANSCRIPT_DIR` to a temporary folder, or use the simulator.

## Conventions

- **Brand.** Follow [assets/brand/BRAND.md](assets/brand/BRAND.md) for the logo, colours, icons and tone. Use the asset files as they are; `apps/desktop/icons/` and `ui/brand/` are generated by `scripts/sync-icons.py`.
- **Words.** The UI says "transcribe", never "record". Platform words come from `PlatformWords`, never from the UI.
- **Minimal network.** The only requests are the downloads of the Whisper model and the voice activity model from Hugging Face (`ModelManager`), and the update check to GitHub (`latest.json` of the latest release, and the update it names; `apps/desktop/src/updater.rs`). Nothing else is sent; GitHub sees the IP address. Nothing listens on a port (the simulator listens on 127.0.0.1 and is never shipped) and `muesli-mcp` speaks stdio only.
- **Audio is never written to disk.**
- **Formatting.** `cargo fmt` with the repo's `rustfmt.toml` (140 columns).
- **Pull requests.** Keep them focused, add tests for logic in `crates/core` or `crates/engine` (against the fakes), and run the commands under [Test](#test) before opening one.
