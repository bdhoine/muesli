# Muesli

**Muesli: Granola's raw, unbaked cousin.** A macOS menu bar app that detects meetings, suggests transcribing, and writes the transcript to a folder. It has no network calls, no accounts and no dependency on any dashboard or other tool; anything downstream reads the files. Optionally, Claude can read its status and transcripts through a bundled read-only MCP server.

## How it works

```
 Detector (every 5 s)                 Capture                                  Output
 ├ Calendar (EventKit)  ─┐        ┌ Mic via AVAudioEngine ("You")
 ├ Teams process         ├─► Notification ─► Start ─┤                 ─► 30 s chunks ─► whisper.cpp (on-device) ─► ~/Documents/MeetingTranscripts/
 └ Mic in use (CoreAudio)┘   "Start transcribing?"  └ System audio via ScreenCaptureKit ("Others")
```

- Always on: menu bar only, registered as a login item on first launch.
- No meeting bot: system audio plus mic are captured locally. Speakers are labelled "You" vs "Others" (no per-person diarization).
- Audio never leaves the Mac and is not stored; only the transcript is written.
- Tell participants you are transcribing, and check your company policy.

## Output

One pair of files per meeting in `~/Documents/MeetingTranscripts/` by default. Pick another folder in **Preferences…** (**Reset to default** undoes it). The `TRANSCRIPT_DIR` environment variable overrides both:

```
2026-11-10_1430_Weekly-sync.md     readable transcript
2026-11-10_1430_Weekly-sync.json   same data, machine-readable
```

JSON fields: id, title, source (calendar | teams | audio | manual), startedAt, endedAt, attendees, state, transcript[] (start, end, speaker, text).

While a meeting is being recorded its transcript is also kept in `~/Library/Application Support/Muesli/meetings/`, updated every 30 seconds. If Muesli is quit or crashes mid-recording, the next launch saves what was transcribed and marks it partial.

## Detection

Muesli suggests transcribing when a calendar meeting starts or when another app has used the microphone for 10 seconds. It keeps watching during a recording: when the other apps release the microphone for a minute, it asks whether to stop, or stops by itself if you enable that in Preferences. Telling Muesli's own microphone use apart from other apps' needs macOS 14.2 or later; on 14.0 and 14.1 the end of a call is not detected.

## Claude

Off by default. **Preferences… → Claude** switches it on and selects the connection:

- **Local**: Claude Desktop and Claude Code on this Mac start the bundled `muesli-mcp` over stdio. Nothing is exposed to the network.
- **Remote**: for claude.ai in the browser and on mobile. Muesli runs `muesli-mcp` on `127.0.0.1` for a Cloudflare Tunnel to forward to, guarded by Cloudflare Access. Not yet tested end to end against claude.ai.

Claude gets three read-only tools: `get_status`, `list_transcripts` and `get_transcript`. Transcripts Claude reads are sent to Anthropic. Setup steps are in [docs/CLAUDE_SETUP.md](docs/CLAUDE_SETUP.md); the requirements behind the design are in [REQUIREMENTS.md](REQUIREMENTS.md).

## Build

Requires macOS 14+ and Xcode 15+.

```bash
mkdir -p ~/Library/Application\ Support/Muesli
curl -L -o ~/Library/Application\ Support/Muesli/ggml-small.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin
./build.sh
open Muesli.app
```

Grant Microphone, Calendar and Screen Recording access when asked. Screen Recording is what allows system audio capture; after enabling it, quit and reopen Muesli.

If recording says a permission is off, use **Open Privacy Settings** in the menu. Without a signing certificate `build.sh` signs ad hoc, and macOS ties permissions for ad-hoc apps to the exact build, so each rebuild can invalidate them. If a permission looks enabled but still fails, remove Muesli from that list (or run `tccutil reset Microphone com.github.bdhoine.muesli` and `tccutil reset ScreenCapture com.github.bdhoine.muesli`), relaunch and grant again. Signing with a real certificate avoids this. `build.sh` uses the first "Apple Development" or "Developer ID Application" certificate in your keychain automatically (override with `SIGN_IDENTITY="..." ./build.sh`) and falls back to ad hoc when there is none. To get a free Apple Development certificate: Xcode → Settings → Accounts → add your Apple ID → Manage Certificates → + → Apple Development. After the first signed build, reset and grant the permissions once more; they then survive rebuilds.

## Layout

| Path | Purpose |
| --- | --- |
| `Sources/Muesli/App.swift` | Menu bar UI, notifications, record/stop flow, `state.json` heartbeat |
| `Sources/Muesli/MeetingDetector.swift` | Calendar, Teams and mic-activity detection; end-of-call detection |
| `Sources/Muesli/AudioCapture.swift` | Mic and system audio capture, 30 s chunking |
| `Sources/Muesli/Transcriber.swift` | whisper.cpp via SwiftWhisper |
| `Sources/Muesli/PreferencesView.swift` | Preferences window: folder, auto-stop, Claude connection |
| `Sources/Muesli/Bridge.swift` | Registers `muesli-mcp` with Claude Desktop (local) or runs it (remote) |
| `Sources/Muesli/Permissions.swift` | Microphone and Screen Recording checks before recording |
| `Sources/MuesliCore/` | Shared by the app and `muesli-mcp`, free of macOS frameworks so it is testable: data types, paths, transcript files, meeting store, preferences, audio chunking, detection rules, Claude Desktop registration |
| `Sources/MuesliCore/MCP/` | The MCP server: tools, stdio and HTTP transports, Cloudflare Access token check, `doctor` |
| `Sources/muesli-mcp/` | Entry point of the `muesli-mcp` executable |
| `Tests/MuesliCoreTests/` | Unit tests for everything in `MuesliCore` (`swift test`) |
| `Info.plist`, `build.sh` | App bundle metadata and build script (builds `Muesli.icns` from the iconset and bundles the menu bar icon) |
| `assets/brand/` | Logo, app icon and menu bar icon. Follow `assets/brand/BRAND.md`; use the files as they are |

## Status

Transcription is verified end to end in local testing. The Claude integration, the Preferences window and the reworked detection were written without a Mac at hand: `MuesliCore` and `muesli-mcp` are compiled and tested on Linux (`swift test`, 79 tests), but the app target has not been compiled since, and the per-process microphone check and the Cloudflare token signature check (macOS Security framework) have not been run.

SwiftWhisper is pinned to an exact commit in `Package.swift`; bump it deliberately.
