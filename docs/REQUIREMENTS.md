# Muesli requirements: AI assistant integration

Status: implemented, except where section 10 says otherwise. Scope: what must be built so Claude and other MCP clients on the same Mac (Claude Desktop, Claude Code, Codex CLI, Gemini CLI, VS Code, Cursor) can ask Muesli whether it is recording, whether a transcript is ready, fetch that transcript and manage finished meetings.

Change on 2026-11-13: remote access was removed in favour of local-only. Section 5 lists what went.

Change on 2026-11-20: Muesli updates itself from its GitHub releases (UP1 to UP9, section 3b). The update check is the only network request besides the model downloads.

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
Muesli.app                 network only for the model downloads and the update check; capture, detection, whisper.cpp
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

- A9. Settings is one scrolling page (section 3a), every section in view with its heading and nothing folded, in this order: General, Transcription, Languages, Vocabulary, Permissions (optional ones with a small "Optional" badge), AI assistants, Updates (release builds), then the version at the foot. It holds the transcripts folder, start at login, the transcription languages (default: the OS language plus English; Whisper picks the most likely of these per chunk), and one switch, "Let AI assistants use Muesli" (off by default). It installs the Claude Desktop extension, or adds a config-file entry instead (M9), and shows the setup command for other assistants with a Copy button. Settings persist across launches in `config.json`; keys of earlier versions are dropped on start.
- A9c. The speech model. There is one (Large v3 Turbo) and nothing to pick; a failed download can be retried ("Download again") and shows the reason and the address it tried. Connecting to the model server gives up after 15 s instead of hanging. Model files of earlier versions are deleted automatically. Settings > Transcription shows, in plain words and one line each, what Muesli has ready: speech recognition, the extra speed of the Neural Engine (macOS) and working without headphones (the echo canceller); the model's name and size only on hover, and "Download again" as a quiet button. Settings > Permissions shows what the operating system currently allows, with a button to the system settings. The Claude Desktop row says what the connection does, shows its status as a badge, and offers Connect, Update or Disconnect: Disconnect removes the settings-file entry Muesli wrote, or, for the extension (which only Claude Desktop can uninstall), opens Claude Desktop and says where to uninstall it.
- A9a. On the first launch a setup window shows four screens with a dot indicator: Welcome ("Meet Muesli", three promises, "Get started", which starts the model download), Languages, Permissions (one screen; macOS: microphone and system audio required, calendar and notifications optional; Windows: microphone only; Continue waits for the required ones; the status is read again when the window comes back to the front) and Done ("Muesli is in your menu bar / system tray", the model progress, the notice to tell people, "Open Muesli"). It is shown until finished (`onboardingDone` in `config.json`); a `config.json` from before onboarding counts as finished. Permissions are asked only from this screen and Settings, never at launch. When an "Allow…" press does not end in the permission being allowed (refused before, or the system does not ask), Muesli opens the matching pane of the system settings instead of doing nothing. macOS: the app is signed with the microphone and calendar entitlements (`apps/desktop/Entitlements.plist`); hardened runtime refuses both without them.
- A9b. Muesli is one app for macOS and Windows (Tauri, `apps/desktop`), with platform adapters behind ports. Windows: models in `%LOCALAPPDATA%\Muesli`; system audio through WASAPI loopback (no permission), microphone through WASAPI; calls detected from other apps using the microphone and from Teams; no calendar detection (Windows has no system calendar, see `PLATFORMS.md`); deleted transcripts go to the Recycle Bin. `PLATFORMS.md` lists every difference.
- A9d. Audio files. "Transcribe Audio File…" (menu) picks one or more files (mp3, m4a/aac, wav, flac, ogg, alac); files dropped on a Muesli window are transcribed too. Progress shows in the main window's list, with Cancel. One file at a time, after the model is ready; a live transcription always goes first. Files are never moved or deleted. The result is a normal meeting with source `file`, speaker `Speaker`, the file name as title and the file's date; a file that cannot be read, or fails part-way, is reported and keeps what was transcribed. If the app quits during an import, the next import of that file resumes after the last transcribed chunk in the same meeting. There is no drop folder.
- A9e. Claude plugin. `plugin/` is a plugin bundle (license, README, manifest, one skill, the `muesli-mcp` server of the installed app as its MCP server), installed from this repository; Anthropic's directory accepts local MCP servers only from Verified organizations, so it is not listed there. A plugin's MCP command cannot differ per OS, so it starts a Node launcher that finds `muesli-mcp` on macOS and Windows (A9f). It passes `claude plugin validate ./plugin --profile directory --strict`, and its version, icon and license are kept in step with the app by `scripts/sync-plugin.py` (checked in CI).
- A9f. The app records the absolute path of its `muesli-mcp` in `mcp-path` in the support folder on every start (`record_helper_path`), so the plugin launcher finds the server wherever Muesli is installed. The `.mcpb` extension and the config-file entry are separate routes.
- A10. The app itself opens no listening sockets. Its only network requests are the model downloads, the Whisper speech model (Large v3 Turbo, one fixed model) and the small Silero voice activity model (on macOS also the Core ML encoder, below), and the update check to GitHub with the download of an update it finds (UP1). Transcribing is disabled, and meeting prompts are not shown, until both are ready; progress is shown in setup, Settings and the menu ("Getting Ready") while they download. Audio without speech is never transcribed: the voice activity model finds the speech first, because Whisper invents text for silence and noise. System audio is captured with a Core Audio tap (macOS 14.4 or later), so only the System Audio Recording permission is needed. Echo cancellation (A10a) runs locally from a model bundled with the app (LocalVQE, 2.9 MB), so it adds no download. Whisper runs on the GPU: on macOS Metal, and the encoder on the Neural Engine once a third download is in: the Core ML encoder (1.2 GB zip, unpacked next to the speech model; `COREML_ENCODER` in `crates/core/src/whisper_model.rs`). It follows the two required models and never blocks transcribing; it is loaded once in the background after the download and at each start (`SpeechModel::prepare`), because the first load compiles it for the device (about 90 s) and a meeting should not wait for that; Settings → Advanced shows its state. Then on Windows Vulkan when the graphics driver supports it and the CPU otherwise, in one installer that bundles the Vulkan loader.
- A10b. Live blocks follow pauses (`crates/core/src/segmenter.rs`, constants at the top). Each speaker's stream has its own voice activity detector (the Silero model already downloaded; a loudness detector stands in if it cannot load) and a speech or pause state, judged every half second in a worker thread, never in the capture callback. A block goes to Whisper when a pause of 1 s follows speech, and at the latest after 10 s, then cut at the quietest moment of its last 2 s; 200 ms is kept around speech. Speech under 1 s waits to join the next block (or goes alone after 5 s of silence). Audio without speech never reaches Whisper. When Whisper takes longer than the audio it transcribes, blocks grow towards 30 s, and shrink again when it keeps up. On stop, the speech still in progress is sent; a tail of 1 s or less is dropped. A block under 5 s of speech keeps the speaker's previous language unless one language has at least 80% of the detection. File imports keep fixed 30 s chunks. No setting.
- A10c. End-of-meeting pass (`crates/engine/src/final_pass.rs`). The live blocks that were transcribed (speech only, the microphone without the call's echo) are kept in memory as 16-bit samples, never on disk. After Stop they are regrouped per speaker into chunks of up to 30 s (blocks less than 2 s apart joined with the pause between them) and transcribed again with beam search (5 beams), the meeting's prompt (A10d) and the end of the same speaker's previous text (about 200 characters). The result replaces the live text and is exported. While it runs the meeting stays `finalizing`; the menu shows "Improving the transcript… 40%", the main window shows the live text read-only with that progress, and Start Transcribing stays available. Start, a meeting prompt answered with Start, or Quit stops the pass and keeps the live text as the transcript (`ready`, not partial); so does a failure. A meeting without speech has nothing to improve. File imports are not passed twice.
- A10d. Vocabulary (`crates/core/src/vocabulary.rs`). Names and terms in `vocabulary.json` in the support folder (not `config.json`, because the app does not reread that and `muesli-mcp` also writes the vocabulary): at most 200 terms of up to 60 characters, trimmed to one line, no case-insensitive duplicates, newest last. It is edited in Settings (a Vocabulary card: chips with ×, a field with Add) and by AI assistants (`get_vocabulary`, `update_vocabulary`, M3; the plugin's skill tells Claude to add names and terms the user corrects). Whisper gets "Glossary: term, term, …" as its initial prompt, for live blocks, the end-of-meeting pass and file imports; live meetings and their pass add the meeting's title (unless generic: Meeting, Call, Teams call) and attendee names, for that meeting only. whisper.cpp keeps the end of a long prompt, so the newest terms and the meeting's names are kept.
- A10a. Speaker echo. Without headphones the microphone also hears the call, so the same words come in as "Others" and as "You". First, during live transcribing, the microphone is cleaned of the call before it reaches the segmenter (A10b): LocalVQE, a neural echo canceller, with the system audio as its reference (`crates/aec`, `EchoStage` in `crates/core/src/aec.rs`). Muesli estimates the echo delay itself (−300 to +800 ms, re-checked every second) and lines the system audio up with it; it fills gaps where the system audio delivered nothing; while the far end has been quiet for 2 s (or with headphones, where nothing echoes) the microphone passes untouched. On an error, a missing library or model, or when cancelling takes more than half of real time, the raw microphone is used for the rest of the meeting, without a message. Imports are not affected. Then, on the text: a "You" line of three words or more is dropped when at least 60% of its words are in the "Others" lines within two seconds of it (`crates/core/src/echo.rs`, checked each time new text comes in). Shorter lines ("Hello.") are dropped only when every word is in an "Others" line within half a second.

## 3a. User-facing behaviour (2026-11 refinement)

The UI says "transcribe / transcribing", never "record"; JSON and MCP values keep their names.

- U1. Start: a question "Looks like a meeting: <title>" / "Detected via <source>. Start transcribing?" [Start transcribing] [Not now], as a notification; clicking the notification itself also starts transcribing. When notifications are off or cannot be shown, a small Muesli window asks the same (both platforms).
- U2. End: "The call seems to have ended" / "Muesli is still transcribing. Stop and save the transcript?" [Stop and save] [Keep transcribing]; clicking the notification itself only opens the Muesli window (stopping stays a button press, and the question stays open); no answer in 5 minutes: stop, save, notice "Saved: <title>" / "No answer for 5 minutes, so Muesli stopped and saved the transcript." [Open].
- U3. Menu (menu bar / tray): Start Transcribing, or a header "Transcribing · <title>" with Stop and Save; Open Muesli; Transcribe Audio File…; a "Recent Transcripts" header and the 3 latest transcripts with length, day and start time ("32 min · Today 14:30", "25 min · Yesterday 10:21", "Mon 10:00", "17 Nov"; open in the main window); while transcribing, Stop and Save shows how long it has run (macOS); Settings… (⌘,); Quit Muesli (⌘Q). macOS: an SF Symbol beside each item. While the model downloads, a "Getting Ready" section with progress, and Start and file import disabled. A red dot on the icon while transcribing. macOS: a click shows the menu. Windows: a left click opens the main window, a right click shows the menu.
- U4. Main window: list on the left, newest first, with the live meeting and file imports at the top (imports with progress and Cancel); reader on the right with title, date, duration, source, attendees and the text (`[mm:ss] **Speaker**: text`). The live meeting updates a few seconds after each pause in speech (A10b), read-only, with a "Transcribing · live, read-only" badge and Stop and Save; its newest line is faded and "Listening · text follows each pause" follows the text. A finished meeting can be renamed (also its exported files; the same logic as `rename_transcript`) and deleted (to the Trash / Recycle Bin, no confirmation, an Undo toast for about 6 s; the same logic as `delete_transcript`). A right-click on a finished meeting in the list shows a menu with "Delete…", which asks "Delete "<title>"?" (Cancel / Delete) before the same delete with Undo; right-click does nothing anywhere else in Muesli's windows (no browser menu). Both platforms, as in the design canvas: no toolbar; a quiet Settings gear at the right of the list header ("Settings… (⌘,)" or "Settings… (Ctrl+,)" on hover), and ⌘, (macOS) or Ctrl+, (Windows) opens Settings from the window; a round "Start transcribing" button in the bottom right corner while idle. Windows: the title bar takes the window's colour (milk, or dark roast in dark mode) and keeps Windows' own buttons. No search, no speaker or attendee editing (assistants keep `rename_speaker` and `set_attendees`). macOS: a Dock icon only while a Muesli window is open.
- U4a. macOS: when a person starts Muesli (Applications, Spotlight, `open`) and setup is done, the main window opens. The automatic start at login stays in the menu bar, told apart by the "open application" Apple event's `lgit` property (`crates/platform-macos/src/launch.rs`). Opening Muesli again while it runs, with no window open, opens the main window too. A first start shows setup first. Windows does the same: the "Start when I sign in" Run value passes `--autostart`, and only that start stays in the tray. On both, "Open Muesli" (the tray, a second start) shows setup until it is done.
- U4b. Cancel on an audio file stops the speech model within moments (the row says "Cancelling…" at once) and discards everything: no meeting, no exported files, also not the text of an earlier cut-off run of the same file. When Muesli quits, a running import is stopped but what was transcribed stays, so it carries on at the next start.
- U5. Settings, setup and the model: A9, A9a and A9c; the vocabulary: A10d.
- U6. The UI decides nothing: the engine (`crates/engine`) gives each window its view state and takes intents; platform words come from the view. Dark mode follows the system; colours and type follow `assets/brand/BRAND.md`.

## 3b. Updates

- UP1. Updates come from the GitHub releases of `bdhoine/muesli`, stable only: the release marked Latest, read through `https://github.com/bdhoine/muesli/releases/latest/download/latest.json`. Pre-releases and drafts are ignored. No release, or a release without `latest.json`, means up to date. Muesli never downgrades.
- UP2. The first check runs 30 s after start, then every 24 h after the last check, with a 15 s connection timeout. On by default; Settings has the switch "Check for updates automatically" (`autoUpdateCheck` in `config.json`, default true). `updateLastChecked` records the last successful check. A failed check by itself is silent and tried again at the next one. Debug builds never check.
- UP3. An update downloads in the background and is installed only if its Tauri updater signature matches the public key built into the app (and the version it was signed for); otherwise it is discarded. Windows updates through the per-user NSIS installer in passive mode (no administrator prompt); the MSI stays a manual download. macOS replaces the app bundle.
- UP4. When an update is ready and Muesli is idle: the menu's first item "Restart to Update to <version>", and a small dot in Pumpkin seed (#5B7A3A) on the menu bar / tray icon (the red transcribing dot wins). The main window shows a banner "Muesli <version> is ready to install" with Restart, What's new and Not now (styled like the extension banner, no coloured stripes on card edges). One system notification per version with Restart (`updateNotified` in `config.json`). Settings shows the status beside the version. Not now hides the banner until the next start; the menu item, the dot and Settings stay.
- UP5. While transcribing a meeting, importing an audio file or downloading the speech model, every update prompt is hidden and Restart is unavailable; the download may go on. The prompts appear once Muesli is idle.
- UP6. Restart installs the update and starts the new version. Quit Muesli with an update ready installs it and quits without starting again.
- UP7. On the first start of a new version (`lastRunVersion` in `config.json`; a missing value, as on a first start, says nothing) a one-time notification "Muesli updated to <version>" with What's new. On macOS Muesli then reads the permissions; if the microphone or system audio are no longer allowed, the setup window opens on the Permissions screen alone with the line "Muesli was updated, and macOS asks for these permissions again."
- UP8. The foot of Settings: a small line "Muesli <version> · What's new". Settings > General: Updates with the status, Check for Updates with "Last checked: <when>"; the result of a check from Settings inline: "Muesli is up to date", "Downloading <version>… n%", "<version> is ready" with Restart, "Couldn't check for updates: <reason>" with Try again; and the switch. What's new opens `https://github.com/bdhoine/muesli/releases/tag/v<version>`.
- UP9. Release pipeline: the Release workflow is started by hand from `main` (or with `test_build` for test installers only). It dates the CHANGELOG section of the workspace version in `Cargo.toml`, tags `main` with `v<version>`, builds Windows and macOS (macOS signed with the certificate in the `APPLE_CERTIFICATE` secrets when set, ad hoc otherwise), signs the updater bundles with the secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`, writes `latest.json` (`tauri-apps/tauri-action`), attaches everything to a draft GitHub release, and then moves `main` to the next patch version. The maintainer publishes the draft. The updater's endpoint, `createUpdaterArtifacts` and the Windows install mode are in `apps/desktop/tauri.release.conf.json`.

## 4. MCP server requirements (muesli-mcp)

- M1. Reads from `~/Library/Application Support/Muesli/`. It never touches audio. It writes meeting data only through the management tools (M3), for meetings that are no longer recording, and `vocabulary.json` only through `update_vocabulary`.
- M2. Speaks MCP over stdio only. It is started by the assistant and never opens a socket.
- M3. Tools. `get_status`, `list_transcripts`, `get_transcript` and `get_vocabulary` are read-only and annotated as such; the management tools are annotated as writes, delete as destructive:

  | Tool | Input | Output |
  |---|---|---|
  | `get_status` | none | `appRunning` (heartbeat younger than 30 s), `appState`, `detected`, `current`, `lastReady`, `error` |
  | `list_transcripts` | `since?`, `limit?` (default 20), `state?` | id, title, source, startedAt, endedAt, state, durationMinutes, attendees |
  | `get_transcript` | `id`, `format?` (`markdown` \| `json`), `fromSeconds?`, `toSeconds?` | transcript text or segments, `state`, `partial`, `truncated`, `nextFromSeconds` |
  | `rename_transcript` | `id`, `title` | updated meeting summary; exported files renamed |
  | `set_attendees` | `id`, `attendees` | updated meeting summary |
  | `rename_speaker` | `id`, `from`, `to` | updated meeting summary |
  | `delete_transcript` | `id` | `deleted`, `title`, `movedToTrash` |
  | `get_vocabulary` | none | `terms`, `maxTerms` |
  | `update_vocabulary` | `add?`, `remove?` (lists of terms; at least one) | `terms`, `maxTerms`; refused past the limits (A10d) |

- M4. `get_transcript` works on a meeting that is still `recording` and returns what exists so far with `partial: true`.
- M5. A single response is capped (default about 60,000 characters). Longer transcripts are paged with `fromSeconds` and `nextFromSeconds`.
- M6. If the heartbeat is stale, `get_status` reports `appRunning: false` and never reports `recording`.
- M8. Each management tool call is appended to `~/Library/Application Support/Muesli/audit.log`, one JSON line with time, tool and meeting id. Read calls are not logged.
- M9. Claude Desktop gets the bundled Desktop Extension `Muesli.mcpb` (name and icon). The `.mcpb` stays the one-click route on both platforms: it is the only install that gives Claude Desktop Chat the tools (a plugin's local server does not load in Chat), and Anthropic has deprecated `.mcpb` only for directory listings, not for installing a local file (checked 2026-11-17; no sunset date is published, so re-check the MCPB guide from time to time). For organisations that block extensions, a switch adds a `Muesli` entry to `claude_desktop_config.json` instead, leaving other entries untouched; installing the extension removes that entry. The Claude Desktop row in Settings shows either route as connected ("Connected", or "Connected through Claude Desktop's settings file", with what to do next or why the file could not be changed) and offers Connect only when neither is in place. The server refuses tool calls unless access is switched on. When access is on and Claude Desktop has an older Muesli extension, Muesli announces the update: at start a system notification with Update, once per Muesli version (`extensionUpdateNotified` in `config.json`), and a banner in the main window with Update and Not now until the update is done or put away (`extensionUpdateDismissed`, per version). Update opens the bundled `.mcpb`, so Claude shows its own install dialog; Muesli never replaces Claude's extension files itself, because Claude keeps its own record of installed extensions.
- M10. `muesli-mcp doctor` checks the preferences file, whether access is switched on, whether the app is running (`state.json` heartbeat) and how many meetings it can see.

## 5. Remote access (removed)

Until 2026-11-13 Muesli could also be reached over the internet, for claude.ai in the browser and on mobile: `muesli-mcp --http <port>` behind a Cloudflare Tunnel, guarded by Cloudflare Access and checked again with the Access JWT in `muesli-mcp`. That mode is gone, together with its preferences, the remote provider picker, the remote checks in `doctor` and the remote audit by email. The requirements that described it (the HTTP half of M2, M7, the old M8, C1 to C8, S1, S2, R1, R2, R4 and acceptance criteria 3 and 5) were removed rather than rewritten.

Claude conversations sync across Claude apps, so work done in Claude Desktop on the Mac is visible in the Claude app on a phone.

## 6. Security and privacy

- S3. No tool can start or stop transcribing or change settings. Management tools change only finished meetings; delete moves files to the Trash. Meetings that are still recording cannot be changed.
- S4. The setup guide and Settings state plainly that transcript text an assistant reads is sent to its provider (Anthropic, for Claude).
- S5. The existing notice stays: tell participants you are transcribing and check company policy.
- S6. The update check sends nothing about the user: a plain request for `latest.json` (and the update file it names) to GitHub, which sees the IP address and nothing else. Updates are installed only with a valid signature made with Muesli's private key, which lives only in the repository's secrets.

## 7. Risks and open questions

- R3. Assistants can only poll. Nothing can notify a conversation that a transcript became ready.

## 8. Acceptance criteria

1. A network monitor shows no connections from Muesli or `muesli-mcp` during detection, recording, saving and tool calls, and no listening port, except the update check to GitHub (and the download of an update it found).
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

Implemented in the Rust workspace and tested on Linux: the engine against the fakes (prompts, timings, main window, Settings, setup, imports, migration, updates), the UI against the simulator in Chromium for both skins, M1 to M6, M8, M10, the stores and management tools, crash recovery (A3) and the detection rules behind A4 and A5.

Not yet run in the real app on either platform since the refinement: everything that touches the OS (capture, notifications, permissions, calendar, Trash, login item, tray). Updates: no real update between two signed releases has run yet, the release workflow has not run, and the permission behaviour after an update on macOS is unverified. The macOS adapters have not been compiled. `PLATFORMS.md` section 3 tracks what has been verified on real machines.

Deviations: A5 needs macOS 14.2 or later; on 14.0 and 14.1 Muesli cannot tell its own microphone use from another app's, so the end of a call is not detected. The server answers MCP protocol versions up to 2025-06-18.
