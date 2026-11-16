# macOS and Windows: how the two apps differ

Muesli has two apps that are both here to stay: the macOS app (Swift, `macos/`) and the Windows app (Rust and Tauri, `windows/`). They share the file formats and the MCP tools (`fixtures/` is the contract, checked by tests) and should offer the same features. This page records every difference and why it exists:

1. differences the platform itself imposes, which will stay;
2. differences that could be closed and are not yet;
3. what has not been verified on real hardware.

## 1. Imposed by the platform

These are not gaps to fix. Each follows from how macOS or Windows works.

| Area | macOS | Windows | Why |
|---|---|---|---|
| Calendar detection | Reads the system calendar (EventKit): every account added in System Settings, one permission. Gives meeting start, title and attendees. | None. Meetings are noticed from the microphone and Teams only, and are titled "Call" or "Teams call". | Windows has no local, system-wide calendar that a desktop app can read. Calendars live inside each mail client or in the cloud. Classic Outlook can be scripted (COM), but the new Outlook that replaces it cannot, and neither covers Google or other calendars. Microsoft Graph would work for Microsoft 365 only, and needs a registered cloud app, a sign-in and a network connection, which contradicts "no account, nothing leaves your computer". The Windows appointment store API is meant for packaged Store apps and is not filled by the new Outlook either. Consequence: on Windows a meeting without call audio (in person) is never suggested; start it by hand. |
| Permission for the other side's audio | "System Audio Recording Only" permission, asked once | No permission exists | Windows lets any desktop app record what a device plays (WASAPI loopback) without asking. macOS guards it. |
| Checking that permission | Read through a private macOS framework; if unavailable Muesli just tries | Not applicable | macOS has no public call that tells whether system audio recording is allowed. |
| Permissions in the first-launch assistant | Microphone, system audio, calendar, notifications: four screens | Microphone only | Follows from the rows above. Windows asks nothing for notifications, and for the microphone it has a system-wide switch rather than a prompt for desktop apps, so the screen opens the microphone once and links to the privacy settings. |
| Where the app lives | Menu bar | System tray (notification area) | Platform convention. |
| Deleted transcripts | Trash | Recycle Bin | Platform convention. Both can be restored. |
| Where models are stored | `~/Library/Application Support/Muesli` | `%LOCALAPPDATA%\Muesli` (everything else in `%APPDATA%\Muesli`) | Windows roams `%APPDATA%` between PCs in company domains; multi-gigabyte files must not roam. macOS has no such split. |
| Where the chosen transcripts folder is remembered | The app's own defaults | `transcriptsFolder` in `config.json` | Windows has no per-app defaults store like macOS; the registry would be the alternative. The macOS app ignores the key and never writes it. Assistants find the folder through `state.json` on both. |
| Speech model acceleration | GPU (Metal), always available on Apple Silicon | CPU | Every supported Mac has the same GPU API. On Windows it depends on the graphics card and its drivers (Vulkan or CUDA), so one build cannot rely on it. See section 2. |

## 2. Could be closed, not done yet

| Area | macOS | Windows | What it would take |
|---|---|---|---|
| Meeting title without a calendar | From the calendar | "Call" / "Teams call" | Read the title of the Teams or Zoom meeting window. Windows only, and fragile across app versions. |
| GPU transcription | yes | no | Build `whisper-rs` with Vulkan as an optional second installer; needs the Vulkan SDK in CI. |
| Windows on ARM build | not applicable | x64 only (runs emulated on ARM PCs) | Add an `aarch64-pc-windows-msvc` job to the Build app workflow. |
| Code signing | Apple Development certificate (local builds) | none; SmartScreen warns | A code-signing certificate and a signing step in CI. For macOS distribution: Developer ID and notarisation. |
| Showing the download percentage in the menu | yes | no (shown in the windows) | Rebuilding a tray menu closes it if open; needs an in-place item update. |

Closed since this page was first written: a broken audio stream is now handled on both (macOS restarts the stream on the new device and saves the transcript if that fails; Windows saves and reports); the transcripts folder can be changed in Settings on both; and the meeting prompts are system notifications with buttons on both.

## 3. Same by design, verification status

| Area | Implementation | Verified |
|---|---|---|
| Files: `config.json`, `state.json`, `meetings/`, transcripts | Shared formats | Golden tests against files written by the Swift app |
| MCP server (7 tools) | Same tools and wording (Trash / Recycle Bin differ) | 33 recorded requests match the Swift replies |
| Whisper transcription | SwiftWhisper / whisper-rs, same models | Real speech on macOS in both; recording tested on Windows by the maintainer (2026-11-16) |
| Model download, sizes, custom file | Same | Tests with a local server; real download on macOS |
| Microphone | AVAudioEngine / cpal (WASAPI), resampled with rubato | macOS; recording tested on Windows by the maintainer (2026-11-16) |
| System audio | Core Audio tap / WASAPI loopback | No automated test records a call on either. Recording tested by hand on Windows (2026-11-16); which streams were checked is not recorded here |
| Device change mid-call | Restart the stream / left to cpal, save on failure | Not exercised on either platform |
| Call detection: microphone in use | Core Audio process list / WASAPI sessions | macOS; not run on Windows |
| Call detection: Teams | Bundle id / process name | macOS; not run on Windows |
| Meeting prompts ("Start transcribing?", "Stop and save?") | Notification with a button on both. Windows: a toast with two buttons (`crates/system`), answered through the toast's in-process event, which works because Muesli is always running; a click on a toast left in the notification centre after Muesli quit does nothing. If the toast cannot be shown, a small window asks instead. | macOS; the Windows toast type-checks but has not been shown or clicked on Windows |
| Permissions page in Settings | macOS: microphone, system audio, calendar, notifications, start at login, each with Allow or Open System Settings. Windows: microphone (the privacy switches, read from the registry), notifications (the toast setting), start at login (a switch). System audio is not listed on Windows because it needs no permission there. | macOS builds, not seen on screen by the author; Windows reads type-check but have not run on Windows |
| Claude Desktop section in Settings | Shown only when the Claude app is installed. macOS: by bundle id. Windows: the installer's folder, or Claude's data folder as a fallback because the install location differs between builds. | macOS; the Windows check is unverified |
| Onboarding, Settings, auto-stop, Claude Desktop extension and config entry, start at login | Same behaviour | Logic tested; Windows UI only seen in a browser with a mock backend |

The maintainer has run the Windows app and tested a recording on Windows (2026-11-16); the details of that test are not recorded here. Everything marked "not run on Windows" or "unverified" above is still open.
