import SwiftUI
import ServiceManagement
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
    case general, transcription, assistants, permissions

    var id: Self { self }

    var title: String {
        switch self {
        case .general: "General"
        case .transcription: "Transcription"
        case .assistants: "AI Assistants"
        case .permissions: "Permissions"
        }
    }

    var symbol: String {
        switch self {
        case .general: "gearshape.fill"
        case .transcription: "waveform"
        case .assistants: "sparkles"
        case .permissions: "lock.shield.fill"
        }
    }

    var tile: Color {
        switch self {
        case .general: .muesliMuted
        case .transcription: .muesliPumpkinSeed
        case .assistants: .muesliRoast
        case .permissions: .muesliMuted
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
                case .transcription: TranscriptionSettings(state: state, draft: $draft)
                case .assistants: AssistantSettings(state: state, draft: $draft)
                case .permissions: PermissionSettings()
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
        .onChange(of: draft.savedLanguages) { state.update(draft) }
        .onChange(of: state.settings) { _, new in if new != draft { draft = new } }
        .onChange(of: draft.customModelPath) { state.update(draft) }
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

            Section {
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
        .safeAreaInset(edge: .bottom) { about }
    }

    /// Bottom right of the page: which Muesli this is, and where it comes from.
    private var about: some View {
        HStack(spacing: 6) {
            Spacer()
            Text("Muesli \(MuesliInfo.version)")
            Text("·")
            Link("GitHub", destination: MuesliInfo.repository)
        }
        .font(.callout)
        .foregroundStyle(.secondary)
        .padding(.horizontal, 22)
        .padding(.bottom, 14)
    }

    private var endDelayOptions: [Int] {
        let options = Preferences.endDelayOptions.map(Int.init)
        return options.contains(Int(draft.endDelaySeconds)) ? options : (options + [Int(draft.endDelaySeconds)]).sorted()
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
    let state: AppState
    @Binding var draft: Preferences

    var body: some View {
        Form {
            ModelSection(model: state.model, draft: $draft, state: state)
            LanguageSections(draft: $draft)
        }
    }
}

/// The language choice, shared by Settings and onboarding.
struct LanguageSections: View {
    @Binding var draft: Preferences

    private var usesSystem: Binding<Bool> {
        Binding(
            get: { draft.languages == nil },
            set: { on in draft.setUsesSystemLanguages(on) }
        )
    }

    var body: some View {
        let chosen = draft.effectiveLanguages
        Group {
            Section {
                Toggle(isOn: usesSystem) {
                    Text("Use my Mac's languages")
                    if draft.usesSystemLanguages {
                        Text(Languages.defaults().map { Languages.displayName($0) }.joined(separator: ", "))
                    }
                }
            } footer: {
                if draft.usesSystemLanguages { languagesFooter }
            }

            // The own list is only shown while it is in use.
            if !draft.usesSystemLanguages {
            Section {
                ForEach(chosen, id: \.self) { code in
                    HStack {
                        Text(Languages.displayName(code))
                        Spacer()
                        if chosen.count > 1 {
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
                Menu {
                        ForEach(available(excluding: chosen), id: \.self) { code in
                            Button(Languages.displayName(code)) { draft.languages = chosen + [code] }
                        }
                    } label: {
                        Label("Add Language", systemImage: "plus")
                    }
                    .menuStyle(.borderlessButton)
                    .fixedSize()
            } footer: {
                languagesFooter
            }
            }
        }
    }

    private var languagesFooter: some View {
        Text("Muesli recognises which of these is spoken. Fewer languages means fewer mistakes. Changes apply from the next recording.")
            .font(.callout)
            .foregroundStyle(.secondary)
    }

    private func available(excluding chosen: [String]) -> [String] {
        Languages.supported
            .filter { !chosen.contains($0) }
            .sorted { Languages.displayName($0) < Languages.displayName($1) }
    }
}

struct ModelSection: View {
    let model: ModelController
    @Binding var draft: Preferences
    let state: AppState
    @State private var ownModel = false

    var body: some View {
        Section {
            LabeledContent {
                HStack(spacing: 8) {
                    Text(ownModel ? "Own file" : "\(SpeechModel.info.title) · \(Self.size(Int64(SpeechModel.info.approximateBytes)))")
                        .foregroundStyle(.secondary)
                    InfoButton { info }
                }
            } label: {
                Text("Model")
                HStack(spacing: 5) {
                    Circle().fill(status.health.color).frame(width: 8, height: 8).accessibilityHidden(true)
                    Text(status.text)
                }
            }

            if case .downloading(let received, let total) = model.phase {
                ModelProgressView(title: "the speech model", received: received, total: total)
                HStack {
                    Spacer()
                    Button("Cancel Download") { model.cancel() }
                }
            } else if case .failed = model.phase, draft.customModelURL == nil || model.lastError?.contains("not found") == false {
                HStack {
                    Spacer()
                    Button("Try Again") { model.ensureReady() }
                }
            } else if case .missing = model.phase {
                HStack {
                    Spacer()
                    Button("Download") { model.ensureReady() }
                }
            }

            Toggle("Use my own model file", isOn: $ownModel)
            if ownModel {
                LabeledContent {
                    Button(draft.customModelURL == nil ? "Choose File…" : "Change…") {
                        if let new = model.chooseCustomModel(draft) { draft = new }
                    }
                } label: {
                    Text("Model file")
                    Text(draft.customModelPath.map { ($0 as NSString).abbreviatingWithTildeInPath } ?? "Any whisper.cpp .bin file. None chosen yet.")
                        .truncationMode(.middle)
                }
            }

            if model.legacy.files > 0 {
                LabeledContent {
                    Button("Remove") { model.removeLegacy() }
                } label: {
                    Text("Old model files")
                    Text("From earlier versions, \(Self.size(Int64(model.legacy.bytes))). Muesli no longer uses them.")
                }
            }
        }
        .onAppear { ownModel = draft.customModelURL != nil }
        .onChange(of: ownModel) { _, on in if !on { draft.customModelPath = nil } }
        .onReceive(NotificationCenter.default.publisher(for: NSWindow.didBecomeKeyNotification)) { _ in model.refresh() }
    }

    /// One line that says what is in use and what is happening.
    private var status: (health: StatusRow.Health, text: String) {
        switch model.phase {
        case .ready: return (.ok, "Ready")
        case .downloading: return (.warning, "Downloading. Transcribing is off until it is ready.")
        case .missing: return (.warning, "Not downloaded yet. Transcribing is off.")
        case .failed(let message): return (.error, message)
        }
    }

    @ViewBuilder private var info: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Transcription runs on this Mac. Muesli only goes online to download the speech model once, and a small model that finds the parts of the audio that contain speech. Use your own file to transcribe with another model.")
                .fixedSize(horizontal: false, vertical: true)
            Divider()
            Text("Models are stored in").foregroundStyle(.secondary)
            Text((Paths.support.path as NSString).abbreviatingWithTildeInPath)
                .font(.system(.callout, design: .monospaced))
                .textSelection(.enabled)
            Button("Show in Finder") { NSWorkspace.shared.open(Paths.support) }
        }
    }

    private static func size(_ bytes: Int64) -> String {
        ByteCountFormatter.string(fromByteCount: bytes, countStyle: .file)
    }
}

/// A small ⓘ button that reveals extra explanation and actions in a popover.
struct InfoButton<Content: View>: View {
    @ViewBuilder let content: () -> Content
    @State private var shown = false

    var body: some View {
        Button { shown.toggle() } label: {
            Image(systemName: "info.circle")
        }
        .buttonStyle(.borderless)
        .help("More information")
        .accessibilityLabel("More information")
        .popover(isPresented: $shown, arrowEdge: .bottom) {
            content()
                .font(.callout)
                .padding(14)
                .frame(width: 340, alignment: .leading)
        }
    }
}

/// The download bar with its caption, shared by Settings and onboarding. The caption counts whole
/// megabytes in fixed-width digits, so it changes once per MB and never changes width.
struct ModelProgressView: View {
    let title: String
    let received: Int64
    let total: Int64

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            ProgressView(value: total > 0 ? min(Double(received) / Double(total), 1) : nil)
            Text("Downloading \(title) · \(received / 1_000_000) of \(total / 1_000_000) MB")
                .font(.callout).foregroundStyle(.secondary)
                .monospacedDigit()
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
    }
}

// MARK: AI assistants

private struct AssistantSettings: View {
    let state: AppState
    @Binding var draft: Preferences
    @State private var client: AssistantClient = .claudeCode

    private var claudeInstalled: Bool { state.bridge.claudeExtension != .claudeMissing }

    var body: some View {
        Form {
            Section {
                LabeledContent {
                    Toggle("Let AI assistants use Muesli", isOn: $draft.mcpEnabled)
                        .labelsHidden()
                        .toggleStyle(.switch)
                } label: {
                    HStack(spacing: 6) {
                        Text("Let AI assistants use Muesli")
                        InfoButton {
                            Text("Assistants can read Muesli's status and transcripts, rename meetings, set attendees and speakers, and delete finished meetings (deleted files go to the Trash). They cannot start or stop a recording.\n\nAn assistant on this Mac starts Muesli's small MCP server itself; nothing is exposed to the network. What an assistant reads is sent to its provider, for example Anthropic for Claude.")
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                }
            }

            if draft.mcpEnabled {
                if claudeInstalled { claudeSection }
                otherAssistants
            }
        }
        .onReceive(NotificationCenter.default.publisher(for: NSWindow.didBecomeKeyNotification)) { _ in
            state.bridge.refreshExtension()
        }
        .onAppear { state.bridge.refreshExtension() }
    }

    /// Only shown when the Claude app is installed on this Mac.
    private var claudeSection: some View {
        Section("Claude Desktop") {
            LabeledContent {
                HStack(spacing: 8) {
                    Circle().fill(claudeHealth.color).frame(width: 8, height: 8).accessibilityHidden(true)
                    Text(claudeStatus)
                    if let action = claudeAction {
                        Button(action) { state.bridge.installExtension() }
                            .disabled(BridgeController.extensionURL == nil)
                    }
                }
            } label: {
                HStack(spacing: 6) {
                    Text("Muesli extension")
                    InfoButton { claudeInfo }
                }
            }
            if draft.claudeDesktopConfigEntry && !state.bridge.extensionInstalled {
                LabeledContent {
                    Button("Remove") { draft.claudeDesktopConfigEntry = false }
                } label: {
                    Text("Config file entry")
                    if !state.bridge.localStatus.isEmpty { Text(state.bridge.localStatus) }
                }
            }
        }
    }

    @ViewBuilder private var claudeInfo: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("The extension lists Muesli in Claude Desktop with its name and icon. Claude Desktop asks you to confirm when you install it.")
                .fixedSize(horizontal: false, vertical: true)
            if case .outdated(let installed, let bundled) = state.bridge.claudeExtension {
                Text("Version \(installed) is installed; \(bundled) is available.")
            } else if case .installed(let version) = state.bridge.claudeExtension {
                Text("Version \(version) is installed.")
            }
            if !state.bridge.extensionInstalled {
                Divider()
                Text("If Claude Desktop says the extension cannot be installed, your organisation blocks extensions. Muesli can add itself to Claude Desktop's config file instead: it is then listed as “Muesli” without an icon. Quit and reopen Claude Desktop afterwards. Installing the extension later removes the entry.")
                    .fixedSize(horizontal: false, vertical: true)
                Button(draft.claudeDesktopConfigEntry ? "Remove from Config File" : "Add to Config File") {
                    draft.claudeDesktopConfigEntry.toggle()
                }
            }
        }
    }

    private var otherAssistants: some View {
        Section(claudeInstalled ? "Other assistants" : "Set up an assistant") {
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

    private var claudeHealth: StatusRow.Health {
        switch state.bridge.claudeExtension {
        case .installed: .ok
        case .outdated, .notInstalled: .warning
        case .claudeMissing: .off
        }
    }

    private var claudeStatus: String {
        switch state.bridge.claudeExtension {
        case .installed: "Installed"
        case .outdated: "Update available"
        case .notInstalled: draft.claudeDesktopConfigEntry ? "Using the config file" : "Not installed"
        case .claudeMissing: ""
        }
    }

    private var claudeAction: String? {
        switch state.bridge.claudeExtension {
        case .notInstalled: "Install…"
        case .outdated: "Update…"
        case .installed, .claudeMissing: nil
        }
    }
}

// MARK: Permissions

/// How Muesli stands with macOS: one row per thing it asks the system for.
private struct PermissionSettings: View {
    @State private var access: [Permission: Permission.Access] = [:]
    @State private var loginItem = SMAppService.mainApp.status

    var body: some View {
        Form {
            Section {
                ForEach(Permission.allCases, id: \.self) { permission in
                    row(permission)
                }
            } footer: {
                Text("macOS asks once for each. After that you change them in System Settings; Muesli picks the change up when this window comes forward.")
                    .font(.callout)
                    .foregroundStyle(.secondary)
            }

            Section {
                LabeledContent {
                    VStack(alignment: .trailing, spacing: 6) {
                        indicator(loginItem == .enabled ? .ok : .warning, loginItem == .enabled ? "On" : "Off")
                        if loginItem != .enabled {
                            Button("Open Login Items…") { SMAppService.openSystemSettingsLoginItems() }
                        }
                    }
                } label: {
                    Text("Start at login")
                    Text("So Muesli is there when a meeting starts")
                }
            }
        }
        .task { await refresh() }
        .onReceive(NotificationCenter.default.publisher(for: NSWindow.didBecomeKeyNotification)) { _ in
            Task { await refresh() }
        }
    }

    private func row(_ permission: Permission) -> some View {
        let state = access[permission] ?? .unknown
        return LabeledContent {
            VStack(alignment: .trailing, spacing: 6) {
                switch state {
                case .granted:
                    indicator(.ok, "Allowed")
                case .denied:
                    indicator(.error, "Not allowed")
                    Button("Open System Settings…") { permission.openSettings() }
                case .unknown:
                    indicator(.off, "Not asked yet")
                    Button("Allow…") {
                        Task {
                            await permission.request()
                            await refresh()
                        }
                    }
                }
            }
        } label: {
            Text(permission.name)
            Text(permission.purpose)
        }
    }

    private func indicator(_ health: StatusRow.Health, _ text: String) -> some View {
        HStack(spacing: 6) {
            Circle().fill(health.color).frame(width: 8, height: 8).accessibilityHidden(true)
            Text(text)
        }
    }

    private func refresh() async {
        for permission in Permission.allCases { access[permission] = await permission.status() }
        loginItem = SMAppService.mainApp.status
    }
}

// MARK: Shared pieces

struct StatusRow: View {
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
