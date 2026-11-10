# audio-transcriber

A macOS menu bar app that detects meetings, suggests transcribing, and writes the transcript to a folder. That is all it does. It has no network calls, no accounts and no dependency on any dashboard or other tool; anything downstream reads the files.

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

One pair of files per meeting in `~/Documents/MeetingTranscripts/` (override with the `TRANSCRIPT_DIR` environment variable):

```
2026-11-10_1430_Weekly-sync.md     readable transcript
2026-11-10_1430_Weekly-sync.json   same data, machine-readable
```

JSON fields: id, title, source (calendar | teams | audio | manual), startedAt, endedAt, attendees, transcript[] (start, end, speaker, text).

## Build

Requires macOS 14+ and Xcode 15+.

```bash
mkdir -p ~/Library/Application\ Support/MeetingScribe
curl -L -o ~/Library/Application\ Support/MeetingScribe/ggml-small.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin
./build.sh
open MeetingScribe.app
```

Grant Microphone, Calendar and Screen Recording access when asked. Screen Recording is what allows system audio capture.

## Layout

| Path | Purpose |
| --- | --- |
| `Sources/MeetingScribe/App.swift` | Menu bar UI, notifications, record/stop flow |
| `Sources/MeetingScribe/MeetingDetector.swift` | Calendar, Teams and mic-activity detection |
| `Sources/MeetingScribe/AudioCapture.swift` | Mic and system audio capture, 30 s chunking |
| `Sources/MeetingScribe/Transcriber.swift` | whisper.cpp via SwiftWhisper |
| `Sources/MeetingScribe/Store.swift` | Writes the .md and .json transcript files |
| `Sources/MeetingScribe/Models.swift` | Data types and folder paths |
| `Info.plist`, `build.sh` | App bundle metadata and build script |

## Status

Not yet compiled on a Mac. Expect a few fixes on first build, in particular SwiftWhisper segment units (milliseconds assumed) and ScreenCaptureKit buffer handling.
