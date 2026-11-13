# Muesli

**Muesli: Granola's raw, unbaked cousin.** A macOS menu bar app that detects meetings, suggests transcribing, and writes the transcript to a folder. That is all it does. It has no network calls, no accounts and no dependency on any dashboard or other tool; anything downstream reads the files.

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

One pair of files per meeting in `~/Documents/MeetingTranscripts/` by default. Pick another folder with **Choose transcripts folder…** in the menu bar (saved in the app's preferences; **Reset to default folder** undoes it). The `TRANSCRIPT_DIR` environment variable overrides both:

```
2026-11-10_1430_Weekly-sync.md     readable transcript
2026-11-10_1430_Weekly-sync.json   same data, machine-readable
```

JSON fields: id, title, source (calendar | teams | audio | manual), startedAt, endedAt, attendees, transcript[] (start, end, speaker, text).

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
| `Sources/Muesli/App.swift` | Menu bar UI, notifications, record/stop flow |
| `Sources/Muesli/MeetingDetector.swift` | Calendar, Teams and mic-activity detection |
| `Sources/Muesli/AudioCapture.swift` | Mic and system audio capture, 30 s chunking |
| `Sources/Muesli/Transcriber.swift` | whisper.cpp via SwiftWhisper |
| `Sources/Muesli/Store.swift` | Writes the .md and .json transcript files |
| `Sources/Muesli/Models.swift` | Data types and folder paths |
| `Sources/Muesli/Permissions.swift` | Microphone and Screen Recording checks before recording |
| `Info.plist`, `build.sh` | App bundle metadata and build script (builds `Muesli.icns` from the iconset and bundles the menu bar icon) |
| `assets/brand/` | Logo, app icon and menu bar icon. Follow `assets/brand/BRAND.md`; use the files as they are |

## Status

Compiles and launches on macOS 26 / Xcode 26. Transcription is verified end to end in local testing.

SwiftWhisper is pinned to an exact commit in `Package.swift`; bump it deliberately.
