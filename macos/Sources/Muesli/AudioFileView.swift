import SwiftUI
import MuesliCore

/// The small window for files picked from the menu: one row per file with its progress.
@MainActor
enum AudioFileWindow {
    private static var window: NSWindow?
    private static var closing: NSObjectProtocol?

    static func show(_ state: AppState) {
        if window == nil {
            let host = NSHostingController(rootView: AudioFileView(files: state.files) { close(state) })
            let w = NSWindow(contentViewController: host)
            w.title = "Transcribing Audio"
            w.styleMask = [.titled, .closable]
            w.isReleasedWhenClosed = false
            w.center()
            closing = NotificationCenter.default.addObserver(forName: NSWindow.willCloseNotification, object: w, queue: .main) { _ in
                Task { @MainActor in state.files.clearFinished(); window = nil }
            }
            window = w
        }
        NSApp.activate(ignoringOtherApps: true)
        window?.makeKeyAndOrderFront(nil)
    }

    static func close(_ state: AppState) {
        window?.close()
    }
}

private struct AudioFileView: View {
    let files: FileImports
    let done: () -> Void

    var body: some View {
        let items = files.manualItems
        let working = items.contains { !$0.isFinished }
        VStack(alignment: .leading, spacing: 14) {
            if items.isEmpty {
                Text("No audio files.").foregroundStyle(.secondary)
            }
            ForEach(items) { item in
                VStack(alignment: .leading, spacing: 5) {
                    HStack {
                        Text(item.name).lineLimit(1).truncationMode(.middle)
                        Spacer()
                        statusText(item)
                    }
                    if case .running(let fraction) = item.state {
                        if let fraction { ProgressView(value: fraction) } else { ProgressView().progressViewStyle(.linear) }
                    }
                    if case .failed(let message) = item.state {
                        Text(message).font(.callout).foregroundStyle(.red).fixedSize(horizontal: false, vertical: true)
                    }
                }
            }
            HStack {
                Text("Transcripts appear in your transcripts folder and the menu.").font(.callout).foregroundStyle(.secondary)
                Spacer()
                if working {
                    Button("Cancel") { files.cancelAllManual() }
                } else {
                    Button("Done", action: done).keyboardShortcut(.defaultAction)
                }
            }
        }
        .padding(20)
        .frame(width: 440)
    }

    @ViewBuilder
    private func statusText(_ item: FileImports.Item) -> some View {
        switch item.state {
        case .waiting: Text("Waiting").foregroundStyle(.secondary)
        case .running(let fraction): Text(fraction.map { "\(Int($0 * 100))%" } ?? "Starting…").foregroundStyle(.secondary).monospacedDigit()
        case .done: Label("Done", systemImage: "checkmark.circle.fill").foregroundStyle(.green).labelStyle(.titleAndIcon)
        case .failed: Label("Failed", systemImage: "exclamationmark.triangle.fill").foregroundStyle(.red)
        case .cancelled: Text("Cancelled").foregroundStyle(.secondary)
        }
    }
}
