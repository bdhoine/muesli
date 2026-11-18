# Changelog

All notable changes to Muesli that you can see or use are listed here, newest first. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses [Semantic Versioning](https://semver.org/) (while the version starts with 0, anything can still change).

## [Unreleased]

### Changed
- A meeting or call is suggested only once. If you dismiss the prompt, ignore it or never join, Muesli does not ask again for the same call, however long it lasts and however often you mute. A call counts as new only after the microphone has been free for the call-end delay (60 seconds by default). Stopping a recording while the call goes on, and a call that runs past its calendar meeting, do not trigger a new prompt either.

## [0.0.2] - 2026-11-17

### Added
- Audio files. Choose **Transcribe Audio File…** in the menu (macOS menu bar, Windows tray) to transcribe recordings you already have (mp3, m4a, wav, flac, ogg), with a small window that shows each file's progress and lets you cancel.
- A drop folder. Put audio files in a folder you choose and Muesli transcribes them as they appear. It is off until you choose a folder, in setup or in Settings. A setting decides what happens to the file afterwards: keep it, move it to the Trash (Recycle Bin on Windows) or delete it. The menu and Settings show what is being transcribed and how many files wait. A recording always goes first, and a transcription that is cut off (the app quits) carries on where it stopped.
- A Claude plugin in this repository for Claude Code and Cowork, on macOS and Windows (it needs Node.js). It gives Claude a skill that gets Muesli's tools right and a template for summarising a meeting in a consistent shape. It also tells Claude to give a generic meeting a proper title and to set attendees and speaker names when the conversation makes them clear. Install it with `/plugin install muesli --marketplace bdhoine/muesli`.
- "Remove old model files" in Settings, for the speech models earlier versions downloaded.
- When the speech model cannot be downloaded, Settings shows why and the address it tried, with "Try again" and "Check connection" buttons (Windows), and the download gives up after 15 seconds instead of hanging.

### Changed
- One speech model for everyone: Whisper Large v3 Turbo (about 575 MB), downloaded together with a small voice activity model on first start. The model picker and the model step in setup are gone. You can still use your own model file.
- Muesli's window comes back to the front after a macOS permission dialog, and setup notices a permission you switch on in System Settings.
- The Claude Desktop extension's icon fills its box.

### Fixed
- Saving a recording could crash Muesli when the Large model was in use.
- Silence and background noise no longer turn into invented text such as "Thank you.": speech is found first and only that is transcribed.

## [0.0.1] - 2026-11-16

### Added
- Muesli, a meeting transcriber for the macOS menu bar. It notices meetings from your calendar, from other apps using the microphone and from Teams, and asks whether to transcribe.
- Transcription on your computer with Whisper, with your microphone labelled "You" and the call audio labelled "Others" (macOS 14.4 or later, using the System Audio Recording permission). The language is picked every 30 seconds from the languages you choose, by default your Mac's language plus English.
- Every meeting becomes a Markdown file and a JSON file in a transcripts folder you choose. The transcript is saved while recording, so a crash keeps what was transcribed, and older transcripts are picked up automatically.
- Muesli knows when the call ends and either asks whether to stop or stops by itself, as you prefer, after a delay you set.
- A first-start setup assistant, a Settings window (General, Transcription, AI Assistants, Permissions) and a menu with your recent transcripts.
- AI assistants: an MCP server with seven tools (status, list and read transcripts, rename a meeting, set attendees, rename a speaker, delete to the Trash). It cannot start or stop a recording. Settings has copy-and-paste setup for Claude Code, Codex CLI, Gemini CLI, VS Code and Cursor, and a one-click Claude Desktop extension, with a config-file entry for organisations that block extensions.
- A Windows app (system tray) with the same files, settings, setup and MCP tools. It records through the Windows audio loopback, which needs no permission.
- Speech model choice: Small by default, Medium, Large or your own model file.

[Unreleased]: https://github.com/bdhoine/muesli/compare/0b6ded6...HEAD
[0.0.2]: https://github.com/bdhoine/muesli/compare/7685530...0b6ded6
[0.0.1]: https://github.com/bdhoine/muesli/commit/7685530
