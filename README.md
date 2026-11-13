<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/logo/lockup-on-dark.svg">
    <img src="assets/brand/logo/lockup.svg" alt="muesli" width="360">
  </picture>
</p>

<p align="center"><strong>Granola's raw, unbaked cousin</strong></p>

<p align="center">An open-source meeting transcriber that runs entirely on your Mac.</p>

---

Muesli sits in your menu bar, notices when a call or meeting starts, and offers to transcribe it. When the meeting is over you have a plain text transcript in a folder. No bot joins your call, no audio leaves your Mac, and there is no account to create.

## What it does

- **Notices meetings.** Muesli watches your calendar and your microphone. When a meeting starts or a call is under way, it asks whether to transcribe.
- **Transcribes on your Mac.** Speech is turned into text by Whisper running locally. It handles mixed languages, so a meeting that drifts between Dutch, English and French is fine.
- **Captures both sides without a bot.** Your microphone is labelled "You" and the call audio is labelled "Others". Nobody sees a recorder join.
- **Writes a file and steps aside.** Each meeting becomes a readable Markdown file and a JSON file with the same content. What you do with them is up to you.
- **Knows when the call ends.** Muesli asks whether to stop, or stops by itself if you prefer.
- **Works with Claude, if you want.** Claude can check whether Muesli is recording, see which transcripts are ready and read them. This is off until you switch it on.

## Private by design

- Audio is never stored. Only the transcript is written.
- Transcription happens on your Mac. Muesli makes no network connections.
- No account, no sign-up, no telemetry.
- Transcripts are ordinary files in a folder you choose.

The one exception is the optional Claude connection: a transcript Claude reads is sent to Anthropic, as with anything else you share with Claude.

Please tell the people you meet with that you are transcribing, and check your company's policy first.

## What you get

One pair of files per meeting, in `~/Documents/MeetingTranscripts/` unless you choose another folder:

```
2026-11-10_1430_Weekly-sync.md
2026-11-10_1430_Weekly-sync.json
```

```markdown
# Weekly sync

- Started: 2026-11-10T14:30:00Z
- Ended: 2026-11-10T15:02:00Z
- Duration: 32 min
- Source: calendar
- Attendees: Ann, Bob

## Transcript

[00:00] **You**: Shall we start with the roadmap?

[00:04] **Others**: Yes. Two things changed since last week.
```

If Muesli is quit or your Mac restarts in the middle of a meeting, the part that was already transcribed is saved the next time Muesli opens.

## Using Muesli

Muesli lives in the menu bar and starts when you log in.

1. A notification appears when a meeting starts: **Start transcribing**.
2. The menu bar icon changes while Muesli records. You can also start and stop from the menu at any time.
3. Stop, and the transcript appears in your folder and in the menu's recent list.

**Preferences…** in the menu lets you choose the transcripts folder, decide whether Muesli stops automatically when a call ends, and set up the Claude connection.

## Muesli and Claude

Switch it on under **Preferences… → Claude**, then ask Claude things like:

- "Is Muesli recording right now?"
- "Is the transcript of my last meeting ready?"
- "Summarise this morning's weekly sync and list the action items."

There are two ways to connect:

| | Works in | How |
|---|---|---|
| **Local** | Claude Desktop and Claude Code on the same Mac | Nothing is exposed to the network. Muesli registers itself with Claude Desktop. |
| **Remote** | claude.ai in the browser and on your phone | Through your own Cloudflare Tunnel, protected by Cloudflare Access. Your Mac must be awake. |

Claude can only read. It cannot start or stop a recording, and it has to ask: Muesli cannot notify a conversation when a transcript is ready.

Setup steps for both are in [docs/CLAUDE_SETUP.md](docs/CLAUDE_SETUP.md).

## Install

There is no download yet, so for now you build Muesli yourself. You need macOS 14 or later and Xcode 15 or later.

```bash
git clone https://github.com/bdhoine/muesli.git
cd muesli

# The speech model (about 470 MB), downloaded once
mkdir -p ~/Library/Application\ Support/Muesli
curl -L -o ~/Library/Application\ Support/Muesli/ggml-small.bin \
  https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin

./build.sh
```

Move `Muesli.app` to Applications and open it.

On first use macOS asks for three permissions:

| Permission | Why |
|---|---|
| Microphone | To transcribe your side of the conversation |
| Screen Recording | macOS requires it for capturing call audio. Muesli records no video. After granting it, quit and reopen Muesli. |
| Calendar | To notice meetings as they start, and to name the transcript |

If recording reports that a permission is off, choose **Open Privacy Settings** in the menu. More help is in [CONTRIBUTING.md](CONTRIBUTING.md#permissions-and-code-signing).

## Good to know

- Speakers are "You" and "Others". Muesli does not tell the other participants apart.
- Text appears in 30-second steps, so the transcript trails the conversation slightly.
- Meetings in your calendar are recognised when they have other attendees or a Teams, Zoom, Google Meet or Webex link.
- Detecting the end of a call needs macOS 14.2 or later.
- Muesli transcribes. It does not summarise or write notes; that is what the files, and Claude, are for.

## Status

Muesli is young. Transcription works and has been tested on real meetings. The Preferences window, end-of-call detection and the Claude connection are new and have had less testing; the remote connection to claude.ai has not yet been tried end to end. Details are in [CONTRIBUTING.md](CONTRIBUTING.md#what-is-verified).

## Contributing

Bug reports and pull requests are welcome. [CONTRIBUTING.md](CONTRIBUTING.md) covers building, testing and how the code is organised.
