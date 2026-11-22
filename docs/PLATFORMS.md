# macOS and Windows: how the two platforms differ

Muesli is one app on both platforms: the same Tauri shell (`apps/desktop`), the same web UI (`ui/`, with a macOS and a Windows skin) and the same Rust crates. Everything the app decides lives in `crates/engine` and is tested once. What differs is behind the ports (`crates/core/src/ports.rs`): `platform-macos` and `platform-windows` implement them for their OS, and the platform's own words (Trash / Recycle Bin, Finder / Explorer, Mac / PC, menu bar / system tray, Start at login / Start when I sign in) come from `PlatformWords`.

Until milestone 6 of the 2026-11 refinement, macOS shipped a separate SwiftUI app (`macos/`). It is retired and kept only until the Tauri app has been verified on a real Mac.

This page records every difference and why it exists:

1. differences the platform itself imposes, which will stay;
2. differences that could be closed and are not yet;
3. what has been verified on real hardware.

## 1. Imposed by the platform

These are not gaps to fix. Each follows from how macOS or Windows works.

| Area | macOS | Windows | Why |
|---|---|---|---|
| Calendar detection | Reads the system calendar (EventKit): every account added in System Settings, one permission. Gives meeting start, title and attendees. | None. Meetings are noticed from the microphone and Teams only, and are titled "Call" or "Teams call". | Windows has no local, system-wide calendar that a desktop app can read. Calendars live inside each mail client or in the cloud. Classic Outlook can be scripted (COM), but the new Outlook that replaces it cannot, and neither covers Google or other calendars. Microsoft Graph would work for Microsoft 365 only, and needs a registered cloud app, a sign-in and a network connection, which contradicts "no account, nothing leaves your computer". The Windows appointment store API is meant for packaged Store apps and is not filled by the new Outlook either. Consequence: on Windows a meeting without call audio (in person) is never suggested; start it by hand. |
| Permission for the other side's audio | "System Audio Recording Only" permission, asked once | No permission exists | Windows lets any desktop app capture what a device plays (WASAPI loopback) without asking. macOS guards it. |
| Checking that permission | Read through the private TCC framework (`platform-macos/src/tcc.rs`), as the Swift app did; if it is unavailable the status is "Unknown" and "Allow…" opens the tap, which makes macOS ask | Not applicable | macOS has no public call that tells whether system audio recording is allowed. This is the one private API in Muesli: re-check it on every major macOS release. |
| Permissions in setup | Microphone and system audio (required), calendar and notifications (optional), on one screen | Microphone only, with a note that call audio needs no permission and a link to the privacy settings | Follows from the rows above. Windows has a system-wide microphone switch rather than a prompt for desktop apps, so "Allow…" opens the microphone once. |
| Where the app lives | Menu bar; a Dock icon only while a Muesli window is open | System tray (notification area) | Platform convention. |
| Main window at start | Opens when a person starts Muesli, not at the start at login (the launch Apple event says which); also when Muesli is opened again while running | Never at start: the tray only | macOS reports a login-item start; Windows' autostart looks like any other start. The macOS check is unverified: it is the pattern the common LaunchAtLogin libraries use, but a real login has not been tried here. |
| Clicking the icon | A click shows the menu | A left click opens Muesli's window, a right click shows the menu | Platform convention for tray apps. |
| Dropping audio files | On Muesli's window | On Muesli's window | A notification-area icon on Windows (`Shell_NotifyIcon`) cannot be a drop target, so files are dropped on Muesli's window on both platforms (the menu bar icon does not take drops either, by decision of 2026-11-20). |
| Deleted transcripts | Trash (NSFileManager, so Put Back works) | Recycle Bin | Platform convention. Both can be restored; Muesli's own Undo works for a few seconds on both. |
| Questions ("Start transcribing?", "Stop and save?") | Notification with two buttons (UNUserNotificationCenter) | Toast with two buttons | Both fall back to Muesli's own prompt window when notifications are off or cannot be shown. A Windows toast is answered through an in-process event, which works because Muesli is always running. |
| Start at login | SMAppService (System Settings → General → Login Items) | The `Run` key in the registry | Platform API. |
| Where models are stored | `~/Library/Application Support/Muesli` | `%LOCALAPPDATA%\Muesli` (everything else in `%APPDATA%\Muesli`) | Windows roams `%APPDATA%` between PCs in company domains; multi-gigabyte files must not roam. macOS has no such split. |
| Updates | Downloads `.app.tar.gz` and replaces the app bundle, then starts the new version | Downloads the NSIS installer and runs it per user in passive mode (a progress bar, no administrator prompt), which closes Muesli and starts the new version; the `.msi` is a manual download only | `tauri-plugin-updater` does both (`apps/desktop/src/updater.rs`, the same code on both platforms); the engine and its rules are shared. Each OS installs apps its own way. |
| Speech model acceleration | GPU (Metal), always available on Apple Silicon, and the encoder on the Neural Engine (Core ML) after a 1.2 GB download: about three times faster (`docs/AUDIO-SPIKE.md`); the first load compiles it for the Mac, so Muesli loads it once in the background | The graphics card through Vulkan when its driver supports Vulkan 1.2, otherwise the CPU | Every supported Mac has the same GPU API. On Windows it depends on the graphics card and its drivers, so the one Windows build uses Vulkan (every current Intel, AMD and NVIDIA driver has it) and falls back to the CPU by itself. It bundles the Vulkan loader (`vulkan-1.dll`), because without it Windows would not start an app linked against it. CUDA would be faster on NVIDIA but needs a large runtime and only helps one vendor. |

## 2. Could be closed, not done yet

| Area | macOS | Windows | What it would take |
|---|---|---|---|
| Meeting title without a calendar | From the calendar | "Call" / "Teams call" | Read the title of the Teams or Zoom meeting window. Windows only, and fragile across app versions. |
| Tray icon after a theme switch | not applicable | the milk or black icon is picked from the taskbar theme when the menu changes, not at the moment the theme changes | Listen for `WM_SETTINGCHANGE` ("ImmersiveColorSet") and redraw. |
| Windows on ARM build | not applicable | x64 only (runs emulated on ARM PCs) | Add an `aarch64-pc-windows-msvc` job to the Release workflow. |
| Code signing | ad hoc (CI has no certificate) | none; SmartScreen warns | Developer ID and notarisation for macOS, a code-signing certificate for Windows, and a signing step in CI. |
| Permissions after an update | macOS ties permissions to the app's signature, and every ad-hoc signed build has a different one, so microphone and system audio are probably asked again after each update (not verified). Muesli notices on the first start of the new version and opens the permissions screen. | Not affected: permissions are not tied to the build | A Developer ID signature (row above) would keep them. |
| Updates without a certificate | not applicable | The update installer is unsigned. SmartScreen is not expected to stop it (the updater runs it, it was not downloaded by a browser), but this is not verified. | A code-signing certificate. |
| Intel Macs | not built | not applicable | Add `x86_64-apple-darwin` to the Release workflow. |

## 3. Same by design, verification status

| Area | Implementation | Verified |
|---|---|---|
| Behaviour (prompts, timings, main window, Settings, setup, imports) | `crates/engine`, the same on both | Engine tests against the fakes (Linux); UI tests in Chromium against the simulator for both skins (Linux). **Not run in the real app on either platform yet.** |
| Files: `config.json`, `state.json`, `meetings/`, transcripts | Shared formats; old `config.json` keys are migrated on start | Golden tests against files written by the Swift app; migration tests |
| MCP server (7 tools) | The same Rust binary (`crates/mcp`) on both; Trash / Recycle Bin wording differs | 33 recorded requests match the replies |
| Whisper transcription | The same Rust code on both (`crates/whisper`), same models | Real speech on macOS (Swift app); a recording tested on Windows by the maintainer (2026-11-16) |
| Updates (check, download, signature, Restart, Quit) | Rules in `crates/engine` (`engine/updates.rs`); `tauri-plugin-updater` in `apps/desktop/src/updater.rs` on both | Engine tests and the `Updater` contract against the fake (Linux); UI tests for both skins. **No update between two signed releases has run on either platform yet**, and the release workflow has not run. There is no native contract test: the adapter lives in the Tauri app. |
| Echo cancellation | `EchoStage` (`crates/core`) and LocalVQE (`crates/aec`), a shared library opened at run time and bundled with its model on both | macOS (M1, 2026-11-20): built, bundled in the app and loaded next to whisper.cpp; with recorded speech through a simulated room, 48–53 dB of echo removed at 40–500 ms and the near speaker kept in double talk. Not yet tried in a real call. Windows: not built. On x86, LocalVQE's CMake builds ggml as separate DLLs (one per CPU generation) which are bundled beside `localvqe.dll`; unverified. |
| End-of-meeting pass, vocabulary | The same engine code on both (`final_pass.rs`, `vocabulary.rs`) | Engine and UI tests on the fakes. macOS (M1): the vocabulary prompt fixes a made-up name with the real model. The pass costs about 20 minutes per hour of speech on an M1 (`docs/AUDIO-SPIKE.md`); not measured on Windows, where it will be slower. |
| Core ML encoder (macOS) | Downloaded after the required models, unpacked next to the speech model, loaded once in the background (`SpeechModel::prepare`) | Measured on an M1 with the real model (`docs/AUDIO-SPIKE.md`); in the installed release app on an M1 (2026-11-20): the 1.2 GB download started by itself at launch while transcribing stayed available, was unpacked into place about 160 s later with the zip removed, and the background load compiled it (Core ML's cache in `~/Library/Caches/com.github.bdhoine.muesli`, tied to the app's identifier, so it should survive Muesli updates). Not yet: a real meeting with it, an M2/M3/M4, and after a macOS update. |
| GPU transcription on Windows | `whisper-rs` with Vulkan (`crates/whisper`), the loader bundled beside `muesli.exe` (`scripts/fetch-vulkan-loader.py`, `tauri.windows.conf.json`), CPU fallback in ggml | **Not built or run yet**: written on a Mac. To check, in order: the Windows CI build; the installer has `vulkan-1.dll` beside `muesli.exe`; on a PC with a graphics card the log names the Vulkan device and transcribing is faster; in a VM without a GPU driver Muesli starts and transcribes on the CPU. |
| Model download, the one fixed model, voice activity model | `ModelManager` | Tests with a local server; real download on macOS (Swift app) |
| Microphone, system audio | cpal on both (WASAPI / WASAPI loopback, Core Audio / Core Audio tap), resampled with rubato | Windows: a recording tested by the maintainer (2026-11-16). macOS through cpal: not run. |
| Call detection: microphone in use, Teams | Core Audio process list / WASAPI sessions; process names | Port contracts on the fakes only; not run natively since the move to ports |
| Calendar (macOS) | EventKit through objc2 | Not compiled or run yet |
| Tray icon colour with the red or green dot | A coloured icon cannot be tinted, so milk and roast glyphs; chosen from the bar's own appearance (macOS: the status item button, redrawn every 2 s while a dot shows; Windows: the registry theme) | Not run on either platform yet |
| Click on the notification itself | `Question::click`: "Looks like a meeting" starts, "The call seems to have ended" opens Muesli. macOS: delegate's default action; Windows: the toast's body click | Port contract and engine tests on the fakes (Linux); native adapters not compiled or run yet |
| Notifications, permissions, Trash, login item | `platform-macos` (objc2) / `platform-windows` | Not compiled for macOS here; the Windows adapters are moved code with small changes, not run |
| Claude plugin (`plugin/`) | A Node launcher finds `muesli-mcp` through `mcp-path`, which the app writes on start | `claude plugin validate`, launcher unit tests; server connects through the launcher in Claude Code on macOS (Swift app) |

Run `cargo test -p muesli-port-contracts --test native` (and `-- --ignored` with a microphone, something playing and a person to click) on each platform to fill this table.
