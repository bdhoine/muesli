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
- **Transcribes on your computer.** Speech is turned into text by Whisper running locally. For each stretch of speech it picks the language being spoken from the languages you chose, so a meeting that drifts between Dutch, English and French is fine.
- **Captures both sides without a bot.** Your microphone is labelled "You" and the call audio is labelled "Others". Nobody sees a recorder join.
- **Writes a file and steps aside.** Each meeting becomes a readable Markdown file and a JSON file with the same content. What you do with them is up to you.
- **Knows when the call ends.** When the microphone has been quiet for a minute, Muesli asks whether to stop and save. If you do not answer within 5 minutes, it stops, saves and tells you.
- **Shows what it wrote.** Muesli's window lists your transcripts, newest first, and shows the text, also live while a meeting is being transcribed. Rename or delete a transcript there.
- **Works with Claude, if you want.** Claude can check whether Muesli is transcribing, see which transcripts are ready and read them. This is off until you switch it on.

## Private by design

- Audio is never stored. Only the transcript is written.
- Transcription happens on your computer. Muesli goes online only to download the speech model and to check for updates.
- No account, no sign-up, no telemetry.
- Transcripts are ordinary files in a folder you choose.

The one exception is the optional AI assistant connection: a transcript an assistant reads is sent to its provider (Anthropic, for Claude), as with anything else you share with it.

Please tell the people you meet with that you are transcribing, and check your company's policy first.

## Audio files

Besides live calls, Muesli transcribes recordings you already have (mp3, m4a, wav, flac, ogg). Use **Transcribe Audio File…** in the menu (also in the window's toolbar on Windows), or drop the files on Muesli's window. Their progress shows at the top of the list, with Cancel. Muesli never moves or deletes your audio files, and nothing leaves your computer.

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

Muesli lives in the menu bar (macOS) or the system tray (Windows) and starts when you log in.

1. When a meeting starts, a notification asks: **Looks like a meeting: Weekly sync. Start transcribing?** Muesli never starts by itself. If notifications are off, a small Muesli window asks instead.
2. While Muesli transcribes, its icon shows a red dot and the menu shows **Transcribing · Weekly sync** with **Stop and Save**. Muesli's window shows the text as it grows, a few seconds after each sentence, with "Listening" below the last line.
3. When the call is over, Muesli asks whether to stop and save. The transcript appears in your folder, in Muesli's window and among the three latest transcripts in the menu.

The menu: **Start Transcribing** (or **Stop and Save**), **Open Muesli**, **Transcribe Audio File…**, your three latest transcripts under **Recent Transcripts**, each with its length and day ("32 min · Today 14:30"; while transcribing, **Stop and Save** shows how long it has run, on a Mac), **Settings…** (⌘,) and **Quit Muesli** (⌘Q); on a Mac each item has an icon. On Windows a left click on the tray icon opens Muesli's window and a right click shows the menu. On a Mac, Muesli has a Dock icon only while one of its windows is open.

**Muesli's window** lists your transcripts, newest first, with the one being transcribed at the top. Choose one to read it. A finished transcript can be renamed (its files are renamed too) or deleted: it goes to the Trash (Recycle Bin on Windows), and **Undo** brings it back for a few seconds. On a Mac, a round **Start transcribing** button sits in the corner of the window; on Windows the toolbar has **Start Transcribing**, **Transcribe Audio File…** and **Show in Explorer**.

**Settings** is one page: the transcripts folder, start at login, the languages, AI assistants and, folded away under Advanced, the speech model and the permissions.

By default Muesli listens for your computer's language plus English. Add the other languages your meetings use in Settings, and **Reset** goes back to the default. Keep the list short: the fewer languages, the fewer mistakes.

## Muesli and AI assistants

Muesli speaks the Model Context Protocol (MCP), so Claude and other assistants (Claude Code, Codex, Gemini CLI, VS Code, Cursor) can read it. Switch on **Let AI assistants use Muesli** in Settings, then ask things like:

- "Is Muesli transcribing right now?"
- "Is the transcript of my last meeting ready?"
- "Summarise this morning's weekly sync and list the action items."

Everything stays on your Mac: the assistant starts Muesli's MCP server itself and talks to it directly, and nothing is exposed to the network. Claude Desktop gets Muesli as an extension, with its name and icon, in one click, and when a new version of Muesli comes with a newer extension, a notification and a banner in Muesli's window offer the update. For the other assistants, Settings shows the command or config to copy. On your phone there is no direct connection, but a Claude conversation started on your Mac syncs to the Claude app.

Assistants can read transcripts and manage finished meetings: rename them, set attendees, rename speakers and delete them (deleted files go to the Trash). They cannot start or stop transcribing, and they have to ask: Muesli cannot notify a conversation when a transcript is ready. What an assistant reads is sent to its provider.

Setup steps are in [docs/CLAUDE_SETUP.md](docs/CLAUDE_SETUP.md).

**Plugin.** The `plugin/` folder is a Claude plugin (macOS and Windows; it needs Node.js) that starts the `muesli-mcp` server of the installed app and teaches Claude how to use its tools. In Claude Code: `/plugin install muesli --marketplace bdhoine/muesli`. See [plugin/README.md](plugin/README.md). Anthropic's plugin directory does not list local MCP servers from unverified organizations, so the plugin is installed from this repository.

## Install

Muesli supports **macOS 14.4 or later** (Apple Silicon) and **Windows 10/11 (64-bit)**. It is one app, built with Rust and Tauri, for both. There are no downloads yet, so for now you build it yourself.

| | macOS | Windows |
|---|---|---|
| Lives in | The menu bar | The system tray |
| Other side of the call | System audio recording (one permission) | System audio loopback (no permission) |
| Notices meetings from | Calendar, microphone in use, Teams | Microphone in use, Teams (Windows has no system calendar) |
| Deleted transcripts go to | Trash | Recycle Bin |

Releases appear on the repository's [Releases page](https://github.com/bdhoine/muesli/releases) once the first one is published. Test builds come from GitHub Actions: on the repository's **Actions** tab, run the **Release** workflow by hand and download the `muesli-aarch64-apple-darwin` (macOS, a `.dmg`) or `muesli-x86_64-pc-windows-msvc` (Windows) artifact. Neither is signed with a certificate yet, so macOS and Windows SmartScreen ask you to confirm.

To build it yourself you need [Rust](https://rustup.rs) and CMake (on Windows also the Visual Studio C++ build tools; on macOS the Xcode command line tools):

```bash
git clone https://github.com/bdhoine/muesli.git
cd muesli/apps/desktop
cargo install tauri-cli --version "^2" --locked
cargo tauri build
```

[docs/PLATFORMS.md](docs/PLATFORMS.md) lists how macOS and Windows differ. The Mac version of this app is new (earlier versions were a separate Swift app); please report what breaks.

### First start

A short setup in four screens: a welcome, your languages, the permissions (all on one screen) and done. Muesli uses one speech model, Whisper Large v3 Turbo (574 MB), and a small voice activity model (under 1 MB) that keeps Whisper from inventing text for silence and noise. Both start downloading on the welcome screen, and until they are ready transcribing is switched off and the menu says **Getting Ready**. If the download fails, Settings shows why and the address it tried, with **Download again**.

On macOS these permissions are asked for, each only when you press **Allow…**:

| Permission | Why | |
|---|---|---|
| Microphone | To transcribe your side of the conversation | Required |
| System Audio Recording | To capture the other side of the call. This is the "System Audio Recording Only" permission; Muesli never records your screen or video. | Required |
| Calendar | To notice meetings as they start, and to name the transcript | Optional |
| Notifications | To ask whether to start and stop transcribing (without them a small Muesli window asks) | Optional |

On Windows only the microphone needs permission (Settings → Privacy & security → Microphone). Settings → Advanced shows what the system currently allows, with a button to the system settings.

### Updates

Muesli keeps itself up to date. Thirty seconds after it starts, and once a day after that, it asks GitHub whether a newer version of Muesli has been released, and downloads it in the background. Only finished releases count, never test versions, and Muesli never goes back to an older version. An update is installed only if it carries Muesli's signature; anything else is thrown away. GitHub sees your IP address when Muesli checks; nothing else is sent.

When an update is ready, the menu's first item says **Restart to Update to <version>**, the menu bar or tray icon gets a small green dot, Muesli's window shows a banner (with **Restart**, **What's new** and **Not now**) and you get one notification. While Muesli transcribes a meeting, imports an audio file or downloads the speech model, all of that waits until it is done. **Restart** installs the update and opens the new version; quitting Muesli with an update ready installs it too. On Windows the update runs the installer for your user account, without asking for an administrator.

Settings → General shows your version with **What's new**, **Check for Updates** with when Muesli last checked, and the switch **Check for updates automatically**. After an update Muesli tells you once, with **What's new**. On a Mac, macOS may ask for the microphone and system audio permissions again after an update (Muesli is not signed with an Apple certificate yet); if so, Muesli opens the permissions screen for you.

## Good to know

- Speakers are "You" and "Others". Muesli does not tell the other participants apart.
- Headphones are not needed. Without them your microphone also hears the call from the speakers; Muesli removes that copy before transcribing, so the others' words appear once, as Others, even when you talk at the same time.
- Text appears a few seconds after each sentence. During a long stretch without a pause it comes every ten seconds or so, and on a slower computer in larger steps, up to thirty seconds.
- Meetings in your calendar are recognised when they have other attendees or a Teams, Zoom, Google Meet or Webex link.
- On Windows, meetings are noticed from other apps using the microphone and from Teams. There is no calendar detection, because Windows has no system calendar that apps can read; an in-person meeting has to be started by hand. [Why the two apps differ](docs/PLATFORMS.md).
- Muesli transcribes. It does not summarise or write notes; that is what the files, and Claude, are for.

## Status

Muesli is young. Transcription works and has been tested on real meetings. The shared app for macOS and Windows, its window, setup and Settings are new and have had little testing on real machines. Details are in [CONTRIBUTING.md](CONTRIBUTING.md#what-is-verified).

## Changelog

What changed in each version is in [CHANGELOG.md](CHANGELOG.md).

## Contributing

Bug reports and pull requests are welcome. [CONTRIBUTING.md](CONTRIBUTING.md) covers building, testing and how the code is organised.

## License

[MIT](LICENSE).

Muesli bundles [LocalVQE](https://github.com/localai-org/LocalVQE) (Apache-2.0) for echo cancellation; its licence ships with the app.
