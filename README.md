<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/logo/lockup-on-dark.svg">
    <img src="assets/brand/logo/lockup.svg" alt="muesli" width="360">
  </picture>
</p>

<p align="center"><strong>Granola's raw, unbaked cousin</strong></p>

<p align="center">An open-source meeting transcriber for macOS and Windows that runs entirely on your computer.</p>

---

Muesli sits in your menu bar (macOS) or system tray (Windows), notices when a call or meeting starts, and offers to transcribe it. When the meeting is over you have a plain text transcript in a folder. No bot joins your call, no audio leaves your computer, and there is no account to create.

## What it does

- **Notices meetings.** Muesli watches your microphone, and on macOS your calendar too. When a meeting starts or a call is under way, it asks whether to transcribe.
- **Transcribes on your computer.** Speech is turned into text by Whisper running locally. Every 30 seconds it picks the language being spoken from the languages you chose, so a meeting that drifts between Dutch, English and French is fine.
- **Captures both sides without a bot.** Your microphone is labelled "You" and the call audio is labelled "Others". Nobody sees a recorder join.
- **Writes a file and steps aside.** Each meeting becomes a readable Markdown file and a JSON file with the same content. What you do with them is up to you.
- **Knows when the call ends.** Muesli asks whether to stop, or stops by itself if you prefer.
- **Works with Claude, if you want.** Claude can check whether Muesli is recording, see which transcripts are ready and read them. This is off until you switch it on.

## Private by design

- Audio is never stored. Only the transcript is written.
- Transcription happens on your computer. Muesli goes online only to download the speech model, once.
- No account, no sign-up, no telemetry.
- Transcripts are ordinary files in a folder you choose.

The one exception is the optional AI assistant connection: a transcript an assistant reads is sent to its provider (Anthropic, for Claude), as with anything else you share with it.

Please tell the people you meet with that you are transcribing, and check your company's policy first.

## Audio files

Besides live calls, Muesli transcribes recordings you already have (mp3, m4a, wav, flac, ogg). Use **Transcribe Audio File…** in the menu, or choose a **drop folder** (in setup or Settings): every audio file you put in it is transcribed, and you can have the file moved to the Trash or deleted afterwards. Nothing leaves your computer.

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
3. Stop, and the transcript appears in your folder and under **Recent Transcripts** in the menu (5 by default; choose 0, 3, 5 or 10 in Settings → General).

**Settings…** (⌘,) in the menu has three pages: **General** (transcripts folder, how many recent transcripts the menu shows, whether Muesli stops automatically when a call ends and after how long), **Transcription** (languages) and **AI Assistants** (access for Claude and other assistants).

By default Muesli listens for your Mac's language plus English. Add the other languages your meetings use under **Settings… → Transcription**. Keep the list short: the fewer languages, the fewer mistakes.

## Muesli and AI assistants

Muesli speaks the Model Context Protocol (MCP), so Claude and other assistants (Claude Code, Codex, Gemini CLI, VS Code, Cursor) can read it. Switch it on under **Settings… → AI Assistants**, then ask things like:

- "Is Muesli recording right now?"
- "Is the transcript of my last meeting ready?"
- "Summarise this morning's weekly sync and list the action items."

Everything stays on your Mac: the assistant starts Muesli's MCP server itself and talks to it directly, and nothing is exposed to the network. Claude Desktop gets Muesli as an extension, with its name and icon, in one click. For the other assistants, Settings shows the command or config to copy. On your phone there is no direct connection, but a Claude conversation started on your Mac syncs to the Claude app.

Assistants can read transcripts and manage finished meetings: rename them, set attendees, rename speakers and delete them (deleted files go to the Trash). They cannot start or stop a recording, and they have to ask: Muesli cannot notify a conversation when a transcript is ready. What an assistant reads is sent to its provider.

Setup steps are in [docs/CLAUDE_SETUP.md](docs/CLAUDE_SETUP.md).

**Plugin.** The `plugin/` folder is a Claude plugin (macOS only for now) that starts the `muesli-mcp` server of the installed app and teaches Claude how to use its tools. In Claude Code: `/plugin install muesli --marketplace bdhoine/muesli`. See [plugin/README.md](plugin/README.md). Anthropic's plugin directory does not list local MCP servers from unverified organizations, so the plugin is installed from this repository.

## Install

Muesli supports **macOS 14.4 or later** and **Windows 10/11 (64-bit)**. There are no downloads yet, so for now you build it yourself.

| | macOS | Windows |
|---|---|---|
| App | Native menu bar app (Swift) | System tray app (Rust and Tauri) |
| Other side of the call | System audio recording (one permission) | System audio loopback (no permission) |
| Notices meetings from | Calendar, microphone in use, Teams | Microphone in use, Teams (Windows has no system calendar) |
| Deleted transcripts go to | Trash | Recycle Bin |

### macOS

You need Xcode 15 or later.

```bash
git clone https://github.com/bdhoine/muesli.git
cd muesli
macos/build.sh
```

Move `macos/Muesli.app` to Applications and open it.

### Windows

The installer is built by GitHub Actions: on the repository's **Actions** tab, run the **Build app** workflow and download the `muesli-x86_64-pc-windows-msvc` artifact. It is not code-signed yet, so Windows SmartScreen asks you to confirm. To build it yourself you need [Rust](https://rustup.rs), the Visual Studio C++ build tools and CMake:

```bash
git clone https://github.com/bdhoine/muesli.git
cd muesli/windows
cargo install tauri-cli --version "^2" --locked
cargo tauri build
```

The Windows app is new and has had far less use than the macOS app; [docs/PLATFORMS.md](docs/PLATFORMS.md) lists how the two differ. Please report what breaks.

### First start

A short setup asks one thing at a time: your languages, whether to stop and save automatically when a call ends, and whether AI assistants may read your transcripts. Then it asks for each permission on its own screen. Muesli uses one speech model, Whisper Large v3 Turbo (574 MB), and a small voice activity model (under 1 MB) that keeps Whisper from inventing text for silence and noise. It downloads both in the background after the first start, and until they are ready transcribing is switched off. In Settings you can point Muesli at your own whisper.cpp `.bin` file instead, and remove model files left by earlier versions. The **Permissions** page in Settings shows what the system currently allows.

On macOS these permissions are asked for:

| Permission | Why |
|---|---|
| Microphone | To transcribe your side of the conversation |
| System Audio Recording | To capture the other side of the call. This is the "System Audio Recording Only" permission; Muesli never records your screen or video. |
| Calendar | To notice meetings as they start, and to name the transcript |
| Notifications | To ask whether to transcribe when a meeting starts |

On Windows only the microphone needs permission (Settings → Privacy & security → Microphone).

If recording reports that a permission is off, choose **Open Privacy Settings** in the menu. More help is in [CONTRIBUTING.md](CONTRIBUTING.md#permissions-and-code-signing).

## Good to know

- Speakers are "You" and "Others". Muesli does not tell the other participants apart.
- Text appears in 30-second steps, so the transcript trails the conversation slightly.
- Meetings in your calendar are recognised when they have other attendees or a Teams, Zoom, Google Meet or Webex link.
- On Windows, meetings are noticed from other apps using the microphone and from Teams. There is no calendar detection, because Windows has no system calendar that apps can read; an in-person meeting has to be started by hand. [Why the two apps differ](docs/PLATFORMS.md).
- Muesli transcribes. It does not summarise or write notes; that is what the files, and Claude, are for.

## Status

Muesli is young. Transcription works and has been tested on real meetings. The Settings window, end-of-call detection and the AI assistant connection are new and have had less testing. Details are in [CONTRIBUTING.md](CONTRIBUTING.md#what-is-verified).

## Changelog

What changed in each version is in [CHANGELOG.md](CHANGELOG.md).

## Contributing

Bug reports and pull requests are welcome. [CONTRIBUTING.md](CONTRIBUTING.md) covers building, testing and how the code is organised.

## License

[MIT](LICENSE).
