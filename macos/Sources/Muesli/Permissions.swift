import AppKit
import AVFoundation
import EventKit
import UserNotifications

/// Everything Muesli asks macOS for. One place for the wording, the status, the request and the
/// System Settings pane, used by recording, the menu and onboarding.
enum Permission: Hashable, CaseIterable {
    case microphone, systemAudio, calendar, notifications

    enum Access { case unknown, granted, denied }

    var name: String {
        switch self {
        case .microphone: "Microphone"
        case .systemAudio: "System Audio"
        case .calendar: "Calendar"
        case .notifications: "Notifications"
        }
    }

    /// Why Muesli wants it, for the Permissions page.
    var purpose: String {
        switch self {
        case .microphone: "To transcribe your side of the conversation"
        case .systemAudio: "To transcribe the other side of a call. Muesli never records your screen."
        case .calendar: "To notice meetings as they start and to name the transcript"
        case .notifications: "To ask whether to transcribe when a meeting starts"
        }
    }

    var message: String {
        switch self {
        case .microphone:
            "Microphone access is off for Muesli."
        case .systemAudio:
            "System Audio Recording access is off for Muesli (needed for the other side's audio). Turn it on under “System Audio Recording Only”, then try again."
        case .calendar:
            "Calendar access is off for Muesli."
        case .notifications:
            "Notifications are off for Muesli."
        }
    }

    var settingsURL: URL {
        let pane = switch self {
        case .microphone: "com.apple.preference.security?Privacy_Microphone"
        case .systemAudio: "com.apple.preference.security?Privacy_AudioCapture"
        case .calendar: "com.apple.preference.security?Privacy_Calendars"
        case .notifications: "com.apple.Notifications-Settings.extension"
        }
        return URL(string: "x-apple.systempreferences:\(pane)")!
    }

    func openSettings() { NSWorkspace.shared.open(settingsURL) }

    /// What macOS says right now. `.unknown` means the user has not been asked yet (or, for
    /// system audio, that macOS will not say).
    func status() async -> Access {
        switch self {
        case .microphone:
            switch AVCaptureDevice.authorizationStatus(for: .audio) {
            case .authorized: .granted
            case .notDetermined: .unknown
            default: .denied
            }
        case .systemAudio:
            switch SystemAudioAccess.status() {
            case .authorized: .granted
            case .denied: .denied
            case .undetermined, .unknown: .unknown
            }
        case .calendar:
            switch EKEventStore.authorizationStatus(for: .event) {
            case .fullAccess: .granted
            case .notDetermined: .unknown
            default: .denied
            }
        case .notifications:
            switch await UNUserNotificationCenter.current().notificationSettings().authorizationStatus {
            case .authorized, .provisional, .ephemeral: .granted
            case .notDetermined: .unknown
            default: .denied
            }
        }
    }

    /// Shows the macOS prompt (if it has not been answered yet) and returns the new status.
    @MainActor @discardableResult
    func request() async -> Access {
        // The macOS dialog takes the focus and gives it back to whoever had it before only after
        // our answer arrives, so a single activate here is too early: repeat it a little later.
        let window = NSApp.isActive ? (NSApp.keyWindow ?? NSApp.windows.first { $0.isVisible }) : nil
        defer { if let window { Permissions.refocus(window) } }
        switch self {
        case .microphone: _ = await AVCaptureDevice.requestAccess(for: .audio)
        case .systemAudio: _ = await SystemAudioAccess.request()
        case .calendar: _ = try? await Permissions.eventStore.requestFullAccessToEvents()
        case .notifications: _ = try? await UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound])
        }
        return await status()
    }
}

enum Permissions {
    /// Brings `window` back to the front now and again shortly after, once macOS has finished
    /// closing its permission dialog.
    @MainActor
    static func refocus(_ window: NSWindow) {
        for delay in [0.0, 0.3, 0.8] {
            Task { @MainActor in
                try? await Task.sleep(for: .seconds(delay))
                guard window.isVisible else { return }
                NSApp.activate(ignoringOtherApps: true)
                window.makeKeyAndOrderFront(nil)
            }
        }
    }

    /// The one calendar store: the instance that asks for access is the one that reads events.
    static let eventStore = EKEventStore()

    /// The first permission a recording cannot do without, asking for it if it was never asked.
    static func missingForRecording() async -> Permission? {
        for permission in [Permission.microphone, .systemAudio] {
            var access = await permission.status()
            if access == .unknown { access = await permission.request() }
            if access == .denied { return permission }
        }
        return nil
    }
}

/// macOS has no public call to ask whether system audio recording is allowed, so this reads the
/// same TCC service the "System Audio Recording Only" pane controls. If the private symbols ever
/// disappear the status is `.unknown` and recording simply goes ahead.
enum SystemAudioAccess {
    enum Status { case authorized, denied, undetermined, unknown }

    private static let service = "kTCCServiceAudioCapture" as CFString
    private typealias Preflight = @convention(c) (CFString, CFDictionary?) -> Int
    private typealias Request = @convention(c) (CFString, CFDictionary?, @escaping (Bool) -> Void) -> Void

    private static let handle = dlopen("/System/Library/PrivateFrameworks/TCC.framework/Versions/A/TCC", RTLD_NOW)

    static func status() -> Status {
        guard let handle, let symbol = dlsym(handle, "TCCAccessPreflight") else { return .unknown }
        let preflight = unsafeBitCast(symbol, to: Preflight.self)
        switch preflight(service, nil) {
        case 0: return .authorized
        case 1: return .denied
        default: return .undetermined
        }
    }

    static func request() async -> Bool {
        guard let handle, let symbol = dlsym(handle, "TCCAccessRequest") else { return true }
        let request = unsafeBitCast(symbol, to: Request.self)
        return await withCheckedContinuation { cont in
            request(service, nil) { cont.resume(returning: $0) }
        }
    }
}
