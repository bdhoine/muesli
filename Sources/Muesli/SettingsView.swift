import SwiftUI
import MuesliCore

/// The Settings window. A plain AppKit window, opened on demand, so nothing
/// appears at login and it works from a menu-bar-only app.
@MainActor
enum SettingsWindow {
    private static var window: NSWindow?
    private static let navigation = SettingsNavigation()
    private static let frameName = "MuesliSettingsWindow"

    static func show(_ state: AppState, page: SettingsPage? = nil) {
        if let page { navigation.page = page }
        if window == nil {
            let host = NSHostingController(rootView: SettingsView(state: state, initial: state.settings, navigation: navigation))
            host.sceneBridgingOptions = [.toolbars, .title]
            host.sizingOptions = [.minSize]
            let w = NSWindow(contentViewController: host)
            w.title = "Muesli Settings"
            w.styleMask = [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView]
            w.toolbarStyle = .unified
            w.isReleasedWhenClosed = false
            w.setContentSize(NSSize(width: 840, height: 560))
            if !w.setFrameUsingName(frameName) { w.center() }
            w.setFrameAutosaveName(frameName)
            window = w
        }
        NSApp.activate(ignoringOtherApps: true)
        window?.makeKeyAndOrderFront(nil)
    }
}

@MainActor @Observable
final class SettingsNavigation {
    var page: SettingsPage? = .general
}

enum SettingsPage: String, CaseIterable, Identifiable {
    case general, transcription, assistants

    var id: Self { self }

    var title: String {
        switch self {
        case .general: "General"
        case .transcription: "Transcription"
        case .assistants: "AI Assistants"
        }
    }

    var symbol: String {
        switch self {
        case .general: "gearshape.fill"
        case .transcription: "waveform"
        case .assistants: "sparkles"
        }
    }

    var tile: Color {
        switch self {
        case .general: .muesliMuted
        case .transcription: .muesliPumpkinSeed
        case .assistants: .muesliRoast
        }
    }
}

struct SettingsView: View {
    let state: AppState
    @State private var draft: Preferences
    @Bindable var navigation: SettingsNavigation

    init(state: AppState, initial: Preferences, navigation: SettingsNavigation) {
        self.state = state
        _navigation = Bindable(wrappedValue: navigation)
        _draft = State(initialValue: initial)
    }

    var body: some View {
        NavigationSplitView {
            List(SettingsPage.allCases, selection: $navigation.page) { page in
                Label {
                    Text(page.title)
                        .lineLimit(1)
                        .fixedSize()
                } icon: {
                    SidebarTile(symbol: page.symbol, color: page.tile)
                }
                .tag(page)
            }
            .navigationSplitViewColumnWidth(260)
            .toolbar(removing: .sidebarToggle)
        } detail: {
            let page = navigation.page ?? .general
            Group {
                switch page {
                case .general: GeneralSettings(state: state, draft: $draft)
                case .transcription: TranscriptionSettings(draft: $draft)
                case .assistants: AssistantSettings(state: state, draft: $draft)
                }
            }
            .formStyle(.grouped)
            .navigationTitle(page.title)
        }
        .tint(.muesliAccent)
        .frame(minWidth: 740, minHeight: 460)
        .onChange(of: draft.autoStop) { state.update(draft) }
        .onChange(of: draft.endDelaySeconds) { state.update(draft) }
        .onChange(of: draft.recentLimit) { state.update(draft) }
        .onChange(of: draft.languages) { state.update(draft) }
        .onChange(of: draft.mcpEnabled) { state.update(draft) }
        .onChange(of: draft.claudeDesktopConfigEntry) { state.update(draft) }
    }
}

private struct SidebarTile: View {
    let symbol: String
    let color: Color

    var body: some View {
        Image(systemName: symbol)
            .font(.system(size: 11, weight: .semibold))
            .foregroundStyle(.white)
            .frame(width: 22, height: 22)
            .background(color.gradient, in: RoundedRectangle(cornerRadius: 6, style: .continuous))
            .accessibilityHidden(true)
    }
}

// MARK: General

private struct GeneralSettings: View {
    let state: AppState
    @Binding var draft: Preferences
    @State private var folder = Paths.transcripts

    var body: some View {
        Form {
            Section {
                LabeledContent {
                    FolderMenu(folder: $folder, state: state)
                } label: {
                    Text("Save transcripts to")
                    Text(Paths.transcriptsOverriddenByEnvironment ? "Set by the TRANSCRIPT_DIR environment variable" : abbreviated(folder))
                        .truncationMode(.middle)
                }

                Picker(selection: $draft.recentLimit) {
                    Text("Don't show").tag(0)
                    ForEach(Preferences.recentLimitOptions.filter { $0 > 0 }, id: \.self) { n in
                        Text("\(n)").tag(n)
                    }
                } label: {
                    Text("Recent transcripts in menu")
                    Text("Shown in the menu bar, newest first")
                }
            }

            Section("Recording") {
                Toggle(isOn: $draft.autoStop) {
                    Text("Stop automatically when the call ends")
                    Text(draft.autoStop ? "The transcript is saved without asking" : "Muesli asks before it stops and saves")
                }
                if draft.autoStop {
                    Picker(selection: $draft.endDelaySeconds) {
                        ForEach(endDelayOptions, id: \.self) { seconds in
                            Text(Self.duration(seconds)).tag(seconds)
                        }
                    } label: {
                        Text("Call has ended after")
                        Text("How long other apps must leave the microphone free")
                    }
                }
            }
        }
    }

    private var endDelayOptions: [Int] {
        let options = Preferences.endDelayOptions
        return options.contains(draft.endDelaySeconds) ? options : (options + [draft.endDelaySeconds]).sorted()
    }

    static func duration(_ seconds: Int) -> String {
        Duration.seconds(seconds).formatted(.units(allowed: [.minutes, .seconds], width: .wide))
    }

    private func abbreviated(_ url: URL) -> String {
        (url.path as NSString).abbreviatingWithTildeInPath
    }
}

private struct FolderMenu: View {
    @Binding var folder: URL
    let state: AppState

    var body: some View {
        Menu {
            Button("Show in Finder") { NSWorkspace.shared.activateFileViewerSelecting([folder]) }
            Divider()
            Button("Choose Other Folder…") { state.chooseTranscriptsFolder(); folder = Paths.transcripts }
            Button("Use Default Folder") { state.resetTranscriptsFolder(); folder = Paths.transcripts }
                .disabled(!Paths.transcriptsIsCustom)
        } label: {
            Label {
                Text(FileManager.default.displayName(atPath: folder.path))
            } icon: {
                Image(nsImage: icon)
            }
        }
        .fixedSize()
        .disabled(Paths.transcriptsOverriddenByEnvironment)
    }

    private var icon: NSImage {
        let image = NSWorkspace.shared.icon(forFile: folder.path)
        image.size = NSSize(width: 16, height: 16)
        return image
    }
}

// MARK: Transcription

private struct TranscriptionSettings: View {
    @Binding var draft: Preferences

    private var usesSystem: Binding<Bool> {
        Binding(
            get: { draft.languages == nil },
            set: { on in draft.languages = on ? nil : Languages.defaults() }
        )
    }

    var body: some View {
        let chosen = draft.effectiveLanguages
        Form {
            Section {
                Toggle(isOn: usesSystem) {
                    Text("Use my Mac's languages")
                    Text(Languages.defaults().map { Languages.displayName($0) }.joined(separator: ", "))
                }
            }

            Section {
                ForEach(chosen, id: \.self) { code in
                    HStack {
                        Text(Languages.displayName(code))
                        Spacer()
                        if draft.languages != nil && chosen.count > 1 {
                            Button {
                                draft.languages = chosen.filter { $0 != code }
                            } label: {
                                Image(systemName: "minus.circle.fill")
                                    .foregroundStyle(.secondary)
                            }
                            .buttonStyle(.borderless)
                            .help("Remove \(Languages.displayName(code))")
                            .accessibilityLabel("Remove \(Languages.displayName(code))")
                        }
                    }
                }
                if draft.languages != nil {
                    Menu {
                        ForEach(available(excluding: chosen), id: \.self) { code in
                            Button(Languages.displayName(code)) { draft.languages = chosen + [code] }
                        }
                    } label: {
                        Label("Add Language", systemImage: "plus")
                    }
                    .menuStyle(.borderlessButton)
                    .fixedSize()
                }
            } header: {
                Text("Languages")
            } footer: {
                Text("Muesli recognises which of these is spoken. Fewer languages means fewer mistakes. Changes apply from the next recording.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }
        }
    }

    private func available(excluding chosen: [String]) -> [String] {
        Languages.supported
            .filter { !chosen.contains($0) }
            .sorted { Languages.displayName($0) < Languages.displayName($1) }
    }
}

// MARK: AI assistants

private struct AssistantSettings: View {
    let state: AppState
    @Binding var draft: Preferences
    @State private var client: AssistantClient = .claudeCode

    var body: some View {
        Form {
            Section {
                Toggle(isOn: $draft.mcpEnabled) {
                    Text("Let AI assistants use Muesli")
                    Text("Read status and transcripts, rename meetings, set attendees and speakers, and delete meetings, through the Model Context Protocol (MCP)")
                }
            } footer: {
                Label("Assistants on this Mac start Muesli's MCP server themselves; nothing is exposed to the network. They can rename and delete finished meetings (deleted files go to the Trash), but cannot start or stop a recording. What an assistant reads is sent to its provider, for example Anthropic for Claude.",
                      systemImage: "lock.shield")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }

            if draft.mcpEnabled {
                localSections
            }
        }
    }

    @ViewBuilder private var localSections: some View {
        Section {
            StatusRow(title: "Muesli extension", health: claudeDesktopHealth, detail: claudeDesktopDetail)
            if let action = claudeDesktopAction {
                HStack {
                    Spacer()
                    Button(action) { state.bridge.installExtension() }
                        .disabled(BridgeController.extensionURL == nil)
                }
            }
            if !state.bridge.extensionInstalled && state.bridge.claudeExtension != .claudeMissing {
                Toggle(isOn: $draft.claudeDesktopConfigEntry) {
                    Text("Add to Claude Desktop's config file instead")
                    Text("For accounts that don't allow extensions. Listed as “Muesli”, without an icon. Quit and reopen Claude Desktop after changing this.")
                }
                if draft.claudeDesktopConfigEntry && !state.bridge.localStatus.isEmpty {
                    Text(state.bridge.localStatus)
                        .font(.callout)
                        .foregroundStyle(.secondary)
                }
            }
        } header: {
            Text("Claude Desktop")
        } footer: {
            Text("The extension shows Muesli with its name and icon; Claude Desktop asks you to confirm. If it says the extension cannot be installed, your organisation blocks extensions: use the config file instead. Installing the extension later removes the config entry.")
                .font(.callout)
                .foregroundStyle(.secondary)
        }
        .onReceive(NotificationCenter.default.publisher(for: NSWindow.didBecomeKeyNotification)) { _ in
            state.bridge.refreshExtension()
        }
        .onAppear { state.bridge.refreshExtension() }

        Section("Set up another assistant") {
            if let helper = BridgeController.helperURL {
                Picker("Assistant", selection: $client) {
                    ForEach(AssistantClient.allCases) { Text($0.name).tag($0) }
                }
                VStack(alignment: .leading, spacing: 8) {
                    Text(client.instructions)
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                    CopyableSnippet(text: client.snippet(helper: helper.path))
                }
            } else {
                StatusRow(title: "muesli-mcp", health: .error, detail: "Missing from the app bundle. Rebuild Muesli with build.sh.")
            }
        }
    }

    private var claudeDesktopHealth: StatusRow.Health {
        switch state.bridge.claudeExtension {
        case .installed: .ok
        case .outdated, .notInstalled: .warning
        case .claudeMissing: .off
        }
    }

    private var claudeDesktopDetail: String {
        switch state.bridge.claudeExtension {
        case .installed(let version): "Version \(version) installed"
        case .outdated(let installed, let bundled): "Version \(installed) installed; \(bundled) is available"
        case .notInstalled: "Not installed yet"
        case .claudeMissing: "Claude Desktop is not installed on this Mac"
        }
    }

    private var claudeDesktopAction: String? {
        switch state.bridge.claudeExtension {
        case .notInstalled: "Install in Claude Desktop…"
        case .outdated: "Update Extension…"
        case .installed: "Reinstall…"
        case .claudeMissing: nil
        }
    }
}

// MARK: Shared pieces

private struct StatusRow: View {
    enum Health {
        case ok, warning, error, off

        var color: Color {
            switch self {
            case .ok: .green
            case .warning: .orange
            case .error: .red
            case .off: .secondary
            }
        }

        var label: String {
            switch self {
            case .ok: "OK"
            case .warning: "Needs attention"
            case .error: "Error"
            case .off: "Off"
            }
        }
    }

    let title: String
    let health: Health
    let detail: String

    var body: some View {
        LabeledContent {
            HStack(spacing: 6) {
                Circle().fill(health.color).frame(width: 8, height: 8).accessibilityHidden(true)
                Text(health.label)
            }
        } label: {
            Text(title)
            if !detail.isEmpty { Text(detail) }
        }
        .accessibilityElement(children: .combine)
    }
}

private struct CopyableSnippet: View {
    let text: String
    @State private var copied = false

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            ScrollView(.horizontal, showsIndicators: false) {
                Text(text)
                    .font(.system(.callout, design: .monospaced))
                    .textSelection(.enabled)
                    .fixedSize()
                    .padding(.horizontal, 10)
                    .padding(.vertical, 7)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(.quaternary.opacity(0.6), in: RoundedRectangle(cornerRadius: 6, style: .continuous))

            Button {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(text, forType: .string)
                copied = true
                Task {
                    try? await Task.sleep(for: .seconds(1.5))
                    copied = false
                }
            } label: {
                Label(copied ? "Copied" : "Copy", systemImage: copied ? "checkmark" : "doc.on.doc")
                    .frame(minWidth: 64)
            }
            .help("Copy to the clipboard")
        }
    }
}

extension Color {
    static let muesliRoast = Color(red: 0x3B / 255, green: 0x2A / 255, blue: 0x1E / 255)
    static let muesliPumpkinSeed = Color(red: 0x5B / 255, green: 0x7A / 255, blue: 0x3A / 255)
    static let muesliMuted = Color(red: 0x6B / 255, green: 0x5A / 255, blue: 0x4B / 255)

    static let muesliAccent = Color(nsColor: NSColor(name: nil) { appearance in
        appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
            ? NSColor(srgbRed: 0x5B / 255, green: 0x7A / 255, blue: 0x3A / 255, alpha: 1)
            : NSColor(srgbRed: 0x4E / 255, green: 0x6B / 255, blue: 0x31 / 255, alpha: 1)
    })
}
