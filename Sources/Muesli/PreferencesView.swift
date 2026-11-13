import SwiftUI
import MuesliCore

/// The Preferences window. A plain AppKit window, opened on demand, so nothing
/// appears at login and it works from a menu-bar-only app.
@MainActor
enum PreferencesWindow {
    private static var window: NSWindow?

    static func show(_ state: AppState) {
        if window == nil {
            let w = NSWindow(contentViewController: NSHostingController(rootView: PreferencesView(state: state, initial: state.settings)))
            w.title = "Muesli Preferences"
            w.styleMask = [.titled, .closable]
            w.isReleasedWhenClosed = false
            w.center()
            window = w
        }
        NSApp.activate(ignoringOtherApps: true)
        window?.makeKeyAndOrderFront(nil)
    }
}

struct PreferencesView: View {
    let state: AppState
    @State private var draft: Preferences
    @State private var folder = Paths.transcripts.path

    init(state: AppState, initial: Preferences) {
        self.state = state
        _draft = State(initialValue: initial)
    }

    var body: some View {
        Form {
            Section("Transcripts") {
                LabeledContent("Folder") {
                    Text(folder).lineLimit(2).truncationMode(.middle).textSelection(.enabled)
                }
                if Paths.transcriptsOverriddenByEnvironment {
                    Text("Set by the TRANSCRIPT_DIR environment variable.").font(.callout).foregroundStyle(.secondary)
                } else {
                    HStack {
                        Button("Choose…") { state.chooseTranscriptsFolder(); folder = Paths.transcripts.path }
                        Button("Reset to default") { state.resetTranscriptsFolder(); folder = Paths.transcripts.path }
                            .disabled(!Paths.transcriptsIsCustom)
                    }
                }
            }

            Section("Recording") {
                Toggle("Stop and save automatically when the call ends", isOn: $draft.autoStop)
                Text("Otherwise Muesli asks. A call counts as ended when other apps have released the microphone for \(draft.endDelaySeconds) seconds.")
                    .font(.callout).foregroundStyle(.secondary)
            }

            Section("Transcription") {
                let chosen = draft.effectiveLanguages
                ForEach(chosen, id: \.self) { code in
                    HStack {
                        Text(Languages.displayName(code))
                        Spacer()
                        Button("Remove") { draft.languages = chosen.filter { $0 != code } }
                            .disabled(chosen.count == 1)
                    }
                }
                Picker("Add language", selection: Binding<String>(
                    get: { "" },
                    set: { code in if !code.isEmpty { draft.languages = chosen + [code] } }
                )) {
                    Text("Choose…").tag("")
                    ForEach(Languages.supported.filter { !chosen.contains($0) }
                        .sorted { Languages.displayName($0) < Languages.displayName($1) }, id: \.self) { code in
                        Text(Languages.displayName(code)).tag(code)
                    }
                }
                HStack {
                    Button("Use system default") { draft.languages = nil }
                        .disabled(draft.languages == nil)
                    Text("System default: \(Languages.defaults().map { Languages.displayName($0) }.joined(separator: ", "))")
                        .foregroundStyle(.secondary)
                }
                Text("Muesli detects which of these is spoken; fewer languages means fewer mistakes. Changes apply from the next recording.")
                    .font(.callout).foregroundStyle(.secondary)
            }

            Section("Claude") {
                Toggle("Let Claude read Muesli's status and transcripts (MCP)", isOn: $draft.mcpEnabled)
                Picker("Connection", selection: $draft.mcpMode) {
                    Text("Local").tag(Preferences.MCPMode.local)
                    Text("Remote").tag(Preferences.MCPMode.remote)
                }
                .pickerStyle(.segmented)
                .disabled(!draft.mcpEnabled)

                if draft.mcpEnabled {
                    switch draft.mcpMode {
                    case .local: localSection
                    case .remote: remoteSection
                    }
                    Text("Transcripts Claude reads are sent to Anthropic. Claude can only read; it cannot start or stop a recording.")
                        .font(.callout).foregroundStyle(.secondary)
                }
            }
        }
        .formStyle(.grouped)
        .frame(width: 540, height: 640)
        .onChange(of: draft.autoStop) { state.update(draft) }
        .onChange(of: draft.languages) { state.update(draft) }
        .onChange(of: draft.mcpEnabled) { state.update(draft) }
        .onChange(of: draft.mcpMode) { state.update(draft) }
    }

    @ViewBuilder private var localSection: some View {
        Text("Claude Desktop on this Mac starts Muesli's MCP server itself. Nothing is exposed to the network.")
            .font(.callout).foregroundStyle(.secondary)
        LabeledContent("Claude Desktop") { Text(state.bridge.localStatus) }
        if let helper = BridgeController.helperURL {
            LabeledContent("Claude Code") {
                Text("claude mcp add muesli -- \"\(helper.path)\"").font(.system(.callout, design: .monospaced)).textSelection(.enabled)
            }
        }
    }

    @ViewBuilder private var remoteSection: some View {
        Text("For claude.ai in the browser and on your phone. Muesli runs a local server that your Cloudflare Tunnel forwards to; Cloudflare Access must protect it. See docs/CLAUDE_SETUP.md.")
            .font(.callout).foregroundStyle(.secondary)
        TextField("Public hostname", text: $draft.publicHostname, prompt: Text("muesli.example.com"))
        TextField("Team domain", text: $draft.teamDomain, prompt: Text("myteam.cloudflareaccess.com"))
        TextField("Audience (AUD) tag", text: $draft.audience, prompt: Text("from the Access application"))
        TextField("Allowed email", text: $draft.allowedEmail, prompt: Text("you@example.com"))
        TextField("Local port", value: $draft.httpPort, format: .number.grouping(.never))
        HStack {
            Button("Apply") { state.update(draft) }
                .disabled(draft == state.settings)
            Text(state.bridge.remoteStatus).foregroundStyle(.secondary)
        }
        if !draft.normalizedHostname.isEmpty {
            LabeledContent("Connector URL") {
                Text("https://\(draft.normalizedHostname)/mcp").textSelection(.enabled)
            }
        }
    }
}
