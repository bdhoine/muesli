# Changelog

All notable changes to Muesli that you can see or use are listed here, newest first. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses [Semantic Versioning](https://semver.org/) (while the version starts with 0, anything can still change).

## [Unreleased]

### Added
- When the call is over, Muesli transcribes the meeting once more, in longer stretches and more carefully, and replaces the live text with the better version. You can follow it in the menu and in Muesli's window; starting the next meeting stops it and keeps the live text.
- Settings: a **Vocabulary** of names and terms Muesli should spell right. The meeting's title and calendar attendees help too.
- AI assistants can read and extend the vocabulary (`get_vocabulary`, `update_vocabulary`). With the Claude plugin, Claude adds a name or term when you correct how it was transcribed.
- Without headphones, your microphone's copy of the call is removed before transcribing, so the others' words show up once, as Others, also when you talk at the same time. If that ever fails, Muesli carries on with the microphone as it is.
- Clicking the "Looks like a meeting" notification now starts transcribing, like its **Start transcribing** button. Clicking "The call seems to have ended" only opens Muesli's window; stopping stays a button press.
- Settings: a small green light before "Connected" for Claude Desktop (amber when an older version is connected).
- Muesli keeps itself up to date. Once a day it checks GitHub for a new version and downloads it in the background. When it is ready, the menu offers **Restart to Update**, the menu bar or tray icon gets a small green dot, Muesli's window shows a banner and you get one notification; nothing interrupts a meeting you are transcribing. Quitting Muesli with an update ready installs it too. Settings shows your version with **What's new**, **Check for Updates**, and a switch to turn automatic checks off. After an update Muesli tells you once, and on a Mac it opens the permissions screen if macOS asks for them again.
- When Claude Desktop has an older Muesli extension, Muesli tells you: a notification with **Update** (once per Muesli version) and a banner at the top of its window with **Update** and **Not now**. Update opens the new extension and Claude asks to install it, so you no longer have to look in Settings.
- The menu shows the length and day of each recent transcript ("32 min · Today 14:30"), and while transcribing, how long the recording has run (macOS).
- macOS: Muesli's window has a round **Start transcribing** button in the corner. While a meeting is transcribed, the newest line shows faded until it settles, and "Listening · text follows each pause" with three moving dots says more is coming (the dots stand still if your system asks for less motion).
- The setup and Settings use the wording of the Muesli design: shorter promises, a line under each step, and "Speech model" with its size.

### Changed
- Live text now appears a few seconds after a sentence ends, instead of every 30 seconds. On a slower computer Muesli waits for longer stretches so it keeps up. Silence is never sent to the speech model.
- A short reply ("Yes.") keeps the language of what you said before it, unless Muesli is sure it is another language.
- Settings: the Muesli version sits on the right of its row.

### Fixed
- macOS: while transcribing, the menu bar icon with its red dot stays readable on a dark menu bar (white), and follows the menu bar when it changes.
- Cancel on an audio file now stops within moments, shows "Cancelling…" and removes the unfinished transcript, instead of finishing the current part and keeping a partial transcript.
- Settings no longer jumps back to the top when you open **Other apps…** or something else changes in the background.
- Settings no longer shows a stray "Off" line under **Other apps…**.
- Without headphones, what the others said in a call no longer shows twice in the transcript (once as "Others" and once as "You", picked up by your microphone from the speakers).
- Settings: the arrows that show **Other apps…** and **Advanced** can be opened were missing in the app.

## [0.1.0] - 2026-11-20

### Added
- Muesli's window: your transcripts on the left, newest first, and the text on the right. The meeting being transcribed sits at the top and its text grows every 30 seconds while you watch. Rename a finished transcript (its files are renamed too) or delete it: it goes to the Trash (Recycle Bin on Windows), and Undo brings it back for a few seconds. "Show in Finder" (Explorer on Windows) shows the file.
- Audio files you transcribe show their progress in that list, with Cancel.
- A new first-start setup in four short screens: welcome, languages, permissions (all on one screen) and done. The speech model downloads while you go through it.
- Settings is one page: transcripts folder and start at login, languages, AI assistants and, folded away, the speech model and permissions.
- Dark mode follows your system.
- macOS: the windows follow the Muesli design: the window colour runs into the title bar, the list has its own buttons for audio files and Finder, and the menu bar menu has an icon beside each item, a Recent Transcripts header and the ⌘, and ⌘Q shortcuts.
- Drop audio files on Muesli's window to transcribe them.
- Windows: a left click on the tray icon opens Muesli's window; a right click shows the menu.
- The menu bar or tray icon shows a red dot while Muesli transcribes.
- Windows: the tray icon is light on a dark taskbar, so you can see it.

### Changed
- On the Mac, Muesli is now the same app as on Windows, with the same window, setup, Settings and menu. It still lives in the menu bar, and shows a Dock icon only while one of its windows is open. Your settings and transcripts carry over.
- A meeting or call is suggested only once. If you dismiss the prompt, ignore it or never join, Muesli does not ask again for the same call, however long it lasts and however often you mute. A call counts as new only after the microphone has been free for 60 seconds. Stopping while the call goes on, and a call that runs past its calendar meeting, do not trigger a new prompt either.
- Muesli always asks before it starts transcribing ("Looks like a meeting… Start transcribing?") and never starts by itself.
- When the call seems to be over (the microphone has been free for 60 seconds), Muesli always asks "Stop and save the transcript?". If you do not answer within 5 minutes, it stops, saves and tells you so, with a button to open the transcript.
- When notifications are off, or cannot be shown, a small Muesli window asks the same questions.
- The menu shows your three latest transcripts; clicking one opens it in Muesli's window. While the speech model downloads, the menu shows "Getting Ready" with its progress, and starting or transcribing a file waits until it is done.
- Muesli says "transcribe" everywhere instead of "record".
- Model files left by earlier versions (Small, Medium, Large) are deleted automatically.
- Settings from earlier versions carry over; the ones that no longer exist are dropped quietly.

### Removed
- The setting to stop automatically at the end of a call, and the call-end delay setting.
- The setting for how many recent transcripts the menu shows.
- The "use my computer's languages" switch: the language list starts from your system language and English, and Reset brings that back.
- Using your own model file.
- The drop folder. Transcribe audio files from the menu or drop them on Muesli's window instead; Muesli never moves or deletes them.

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
[0.1.0]: https://github.com/bdhoine/muesli/compare/0b6ded6...HEAD
[0.0.2]: https://github.com/bdhoine/muesli/compare/7685530...0b6ded6
[0.0.1]: https://github.com/bdhoine/muesli/commit/7685530
