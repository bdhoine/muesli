# Changelog

All notable changes to Muesli that you can see or use are listed here, newest first. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses [Semantic Versioning](https://semver.org/) (while the version starts with 0, anything can still change).

## [Unreleased]

## [0.0.1]

The first release of Muesli, an on-device meeting transcriber for macOS (14.4 or later, Apple Silicon) and Windows.

### Added

**Noticing meetings**
- Muesli lives in the macOS menu bar or the Windows system tray. It notices a meeting from other apps using the microphone, from Teams and, on a Mac, from your calendar, and asks "Looks like a meeting… Start transcribing?". It never starts by itself, and asks only once per call.
- When the call seems to be over, Muesli asks "Stop and save the transcript?". Without an answer within 5 minutes it stops, saves and tells you so. Without notifications, a small Muesli window asks the same questions.

**Transcribing on your computer**
- Whisper Large v3 Turbo transcribes on your own computer; nothing is sent anywhere. Your microphone is labelled "You" and the call audio "Others".
- Live text appears a few seconds after each sentence. When the call is over, Muesli goes over the whole meeting once more, more carefully, and replaces the live text with the better version.
- Without headphones, your microphone's copy of the call is removed, so the others' words show up once, as Others.
- Silence and background noise are never sent to the speech model, so it does not invent text such as "Thank you.".
- The language is picked per stretch of speech from the languages you choose (by default your system's language and English); a short reply keeps the language of what was said before it.
- A **Vocabulary** of names and terms Muesli should spell right; the meeting's title and calendar attendees help too.
- On a Mac, transcription uses the graphics chip and, after an extra download (1.2 GB), Apple's Neural Engine, about three times faster. On Windows, transcription uses the graphics card (Vulkan) when its driver supports it.
- Audio files: **Transcribe Audio File…** in the menu, or drop files (mp3, m4a, wav, flac, ogg) on Muesli's window, with progress and Cancel.

**Transcripts**
- Every meeting becomes a Markdown file and a JSON file in a transcripts folder you choose. The transcript is saved while transcribing, so a crash keeps what was transcribed.
- Muesli's window lists your transcripts, newest first, with the text beside them. Rename a transcript (its files are renamed too) or move it to the Trash or Recycle Bin.
- The menu shows your three latest transcripts with their length, day and start time, and while transcribing, how long it has run (macOS). The menu bar or tray icon shows a red dot while Muesli transcribes.

**Setup and Settings**
- A short setup: welcome, languages, permissions (all on one screen) and done. The speech model downloads while you go through it, and transcribing waits until it is ready.
- Settings on one page: transcripts folder, start at login, languages, vocabulary, AI assistants and, folded away, the speech model and permissions. Permissions are asked only when you press **Allow…**; if that cannot get one, Muesli opens the right page of the system settings.
- Dark mode follows your system.

**AI assistants**
- An MCP server lets AI assistants read your transcripts on your computer: status, list and read transcripts, rename a meeting, set attendees, rename a speaker, delete to the Trash, and read and extend the vocabulary. It cannot start or stop transcribing, and it is off until you switch it on in Settings.
- Claude Desktop connects in one click with Muesli's extension, or through Claude Desktop's settings file for organisations that block extensions. Settings shows how it is connected, and Muesli tells you when Claude Desktop has an older extension.
- Copy-and-paste setup for Claude Code, Codex CLI, Gemini CLI, VS Code (Copilot) and Cursor, and a Claude plugin for Claude Code and Cowork with a skill and a meeting summary template.

**Updates**
- Muesli keeps itself up to date from its GitHub releases: once a day it checks, downloads in the background and offers **Restart to Update**; nothing interrupts a meeting you are transcribing. Only updates signed by Muesli are installed. Settings has **Check for Updates** and a switch to turn automatic checks off; the version and **What's new** sit at the bottom of Settings.

[Unreleased]: https://github.com/bdhoine/muesli/compare/v0.0.1...HEAD
[0.0.1]: https://github.com/bdhoine/muesli/releases/tag/v0.0.1
