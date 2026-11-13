import AppKit
import AVFoundation
import CoreGraphics

enum Permission: Equatable {
    case microphone, screenRecording

    var message: String {
        switch self {
        case .microphone:
            "Microphone access is off for Muesli."
        case .screenRecording:
            "Screen Recording access is off for Muesli (needed for the other side's audio). Turn it on, then quit and reopen Muesli."
        }
    }

    var settingsURL: URL {
        switch self {
        case .microphone:
            URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")!
        case .screenRecording:
            URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture")!
        }
    }

    func openSettings() { NSWorkspace.shared.open(settingsURL) }
}

enum Permissions {
    static func missingForRecording() async -> Permission? {
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized:
            break
        case .notDetermined:
            guard await AVCaptureDevice.requestAccess(for: .audio) else { return .microphone }
        default:
            return .microphone
        }
        guard CGPreflightScreenCaptureAccess() else {
            CGRequestScreenCaptureAccess()
            return .screenRecording
        }
        return nil
    }
}
