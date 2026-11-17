import SwiftUI
import MuesliCore

/// The first-launch assistant: one question per screen. Nothing here asks macOS for access
/// until its own screen is showing.
@MainActor
enum OnboardingWindow {
    private static var window: NSWindow?

    static func show(_ state: AppState) {
        if window == nil {
            let host = NSHostingController(rootView: OnboardingView(state: state, initial: state.settings) { close() })
            let w = NSWindow(contentViewController: host)
            w.title = "Welcome to Muesli"
            w.styleMask = [.titled, .closable, .fullSizeContentView]
            w.titlebarAppearsTransparent = true
            w.titleVisibility = .hidden
            w.isReleasedWhenClosed = false
            w.center()
            window = w
        }
        NSApp.activate(ignoringOtherApps: true)
        window?.makeKeyAndOrderFront(nil)
    }

    /// Brings the window back to the front, e.g. after a macOS permission dialog took the focus.
    static func refocus() {
        guard let window else { return }
        NSApp.activate(ignoringOtherApps: true)
        window.makeKeyAndOrderFront(nil)
    }

    static func close() {
        window?.close()
        window = nil
    }
}

private enum Step: Int, CaseIterable {
    case languages, autoStop, assistants, microphone, systemAudio, calendar, notifications

    /// The permission this screen asks for, if it is a permission screen.
    var permission: Permission? {
        switch self {
        case .microphone: .microphone
        case .systemAudio: .systemAudio
        case .calendar: .calendar
        case .notifications: .notifications
        default: nil
        }
    }

    var symbol: String {
        switch self {
        case .languages: "globe"
        case .autoStop: "stop.circle.fill"
        case .assistants: "sparkles"
        case .microphone: "mic.fill"
        case .systemAudio: "speaker.wave.2.fill"
        case .calendar: "calendar"
        case .notifications: "bell.badge.fill"
        }
    }

    var color: Color {
        switch self {
        case .microphone, .calendar: .muesliPumpkinSeed
        case .languages, .systemAudio, .autoStop: .muesliMuted
        case .assistants, .notifications: .muesliRoast
        }
    }

    var title: String {
        switch self {
        case .languages: "Which languages do you speak?"
        case .autoStop: "Stop when the call ends?"
        case .assistants: "Connect an AI assistant?"
        case .microphone: "Hear your side"
        case .systemAudio: "Hear the other side"
        case .calendar: "Notice your meetings"
        case .notifications: "Ask before recording"
        }
    }

    var intro: String {
        switch self {
        case .languages: "Muesli works out which one is spoken. Fewer languages means fewer mistakes."
        case .autoStop: "Muesli can notice that the other app let go of the microphone and save the transcript for you, or ask you first."
        case .assistants: "Claude and other assistants can read your transcripts. They can never start or stop a recording."
        case .microphone: "Muesli needs the microphone to transcribe what you say."
        case .systemAudio: "To transcribe the other people on a call, Muesli listens to your Mac's audio. It never records your screen."
        case .calendar: "Muesli reads your calendar to spot a meeting as it starts and to name the transcript."
        case .notifications: "Muesli shows a notification when a meeting starts, so you decide whether to transcribe."
        }
    }
}

private typealias Access = Permission.Access

private struct OnboardingView: View {
    let state: AppState
    let finish: () -> Void
    @State private var draft: Preferences
    @State private var step: Step = .languages
    @State private var access: Access = .unknown
    @State private var assistantsChosen = false
    @State private var autoStopChosen = false

    init(state: AppState, initial: Preferences, finish: @escaping () -> Void) {
        self.state = state
        self.finish = finish
        _draft = State(initialValue: initial)
    }

    var body: some View {
        VStack(spacing: 0) {
            VStack(spacing: 10) {
                Image(systemName: step.symbol)
                    .font(.system(size: 34, weight: .semibold))
                    .foregroundStyle(.white)
                    .frame(width: 72, height: 72)
                    .background(step.color.gradient, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                    .shadow(color: step.color.opacity(0.35), radius: 10, y: 5)
                    .accessibilityHidden(true)
                    .padding(.top, 8)
                Text(step.title).font(.title.weight(.semibold))
                Text(step.intro)
                    .font(.body)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
            }
            .padding(.horizontal, 40)
            .padding(.top, 36)
            .padding(.bottom, 20)
            .frame(maxWidth: .infinity)
            .background(LinearGradient(colors: [step.color.opacity(0.14), .clear], startPoint: .top, endPoint: .bottom))

            Group {
                switch step {
                case .languages: LanguageStep(draft: $draft)
                case .autoStop: AutoStopStep(draft: $draft, chosen: $autoStopChosen)
                case .assistants: AssistantStep(state: state, draft: $draft, chosen: $assistantsChosen)
                default: PermissionStep(step: step, access: access)
                }
            }
            .padding(.horizontal, 40)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)

            footer
        }
        .tint(.muesliAccent)
        .frame(width: 520, height: 600)
        .onChange(of: draft) { state.update(draft) }
        .task(id: step) { access = await step.permission?.status() ?? .unknown }
        // The answer may change outside the app (System Settings), so look again on return.
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
            Task { await recheck() }
        }
        .animation(.easeInOut(duration: 0.2), value: step)
    }

    // MARK: Footer

    private var footer: some View {
        HStack(spacing: 6) {
            ForEach(Step.allCases, id: \.self) { s in
                Circle()
                    .fill(s == step ? Color.muesliAccent : Color.secondary.opacity(0.3))
                    .frame(width: 7, height: 7)
            }
            Spacer()
            if step != .languages {
                Button("Back") { move(-1) }.controlSize(.large)
            }
            if step.permission != nil, access == .unknown {
                Button("Skip") { advance() }.controlSize(.large)
            } else if let permission = step.permission, access == .denied {
                Button("Open System Settings") { permission.openSettings() }.controlSize(.large)
            }
            primary
        }
        .padding(20)
    }

    private var primary: some View {
        Button {
            if let permission = step.permission, access == .unknown {
                Task {
                    let answer = await permission.request()
                    access = answer == .unknown ? .denied : answer       // answered, but not with a yes
                    OnboardingWindow.refocus()                           // the macOS dialog took the focus
                }
            } else {
                advance()
            }
        } label: {
            HStack(spacing: 6) {
                Text(primaryTitle)
                Image(systemName: step == .notifications && access != .unknown ? "checkmark" : "arrow.right")
            }
            .font(.headline)
            .padding(.horizontal, 10)
            .padding(.vertical, 2)
        }
        .buttonStyle(.borderedProminent)
        .controlSize(.large)
        .keyboardShortcut(.defaultAction)
    }

    private var primaryTitle: String {
        if let permission = step.permission, access == .unknown { return "Allow \(permission.name)" }
        return step == .notifications ? "Start using Muesli" : "Continue"
    }

    /// Picks up a change made in System Settings. `.unknown` is ignored: it also means "macOS will
    /// not say" (system audio), which must not undo an answer we already have.
    private func recheck() async {
        guard let permission = step.permission else { return }
        let now = await permission.status()
        if now != .unknown { access = now }
    }

    private func move(_ delta: Int) {
        step = Step(rawValue: step.rawValue + delta) ?? step
    }

    private func advance() {
        if step == .notifications {
            draft.onboardingDone = true
            state.update(draft)
            finish()
        } else {
            move(1)
        }
    }
}

// MARK: - Screens

private struct ChoiceCard: View {
    let title: String
    let detail: String
    let selected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 12) {
                VStack(alignment: .leading, spacing: 2) {
                    Text(title).font(.headline)
                    Text(detail).font(.callout).foregroundStyle(.secondary)
                }
                Spacer()
                Image(systemName: selected ? "checkmark.circle.fill" : "circle")
                    .font(.title3)
                    .foregroundStyle(selected ? Color.muesliAccent : Color.secondary.opacity(0.5))
            }
            .padding(14)
            .frame(maxWidth: .infinity)
            .background(RoundedRectangle(cornerRadius: 12, style: .continuous)
                .fill(selected ? Color.muesliAccent.opacity(0.10) : Color.secondary.opacity(0.07)))
            .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous)
                .strokeBorder(selected ? Color.muesliAccent : .clear, lineWidth: 1.5))
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }
}

private struct LanguageStep: View {
    @Binding var draft: Preferences

    var body: some View {
        let chosen = draft.effectiveLanguages
        VStack(alignment: .leading, spacing: 14) {
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 120), spacing: 8)], alignment: .leading, spacing: 8) {
                ForEach(chosen, id: \.self) { code in
                    HStack(spacing: 6) {
                        Text(Languages.displayName(code)).lineLimit(1)
                        if chosen.count > 1 {
                            Button { draft.languages = chosen.filter { $0 != code } } label: {
                                Image(systemName: "xmark.circle.fill").foregroundStyle(.secondary)
                            }
                            .buttonStyle(.plain)
                            .accessibilityLabel("Remove \(Languages.displayName(code))")
                        }
                    }
                    .padding(.horizontal, 12).padding(.vertical, 7)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(Capsule().fill(Color.muesliAccent.opacity(0.14)))
                }
            }
            HStack {
                Menu {
                    ForEach(available(excluding: chosen), id: \.self) { code in
                        Button(Languages.displayName(code)) { draft.languages = chosen + [code] }
                    }
                } label: {
                    Label("Add a language", systemImage: "plus")
                }
                .menuStyle(.borderlessButton)
                .fixedSize()
                Spacer()
                if draft.languages != nil {
                    Button("Back to my Mac's languages") { draft.languages = nil }
                        .buttonStyle(.link)
                }
            }
            if draft.languages == nil {
                Text("Taken from your Mac's settings.")
                    .font(.callout).foregroundStyle(.secondary)
            }
        }
    }

    private func available(excluding chosen: [String]) -> [String] {
        Languages.supported
            .filter { !chosen.contains($0) }
            .sorted { Languages.displayName($0) < Languages.displayName($1) }
    }
}

private struct AutoStopStep: View {
    @Binding var draft: Preferences
    @Binding var chosen: Bool

    var body: some View {
        VStack(spacing: 10) {
            ChoiceCard(title: "Stop and save automatically", detail: "Once the call has ended, the transcript is saved without asking",
                       selected: chosen && draft.autoStop) { chosen = true; draft.autoStop = true }
            ChoiceCard(title: "Ask me first", detail: "Muesli shows a notification and keeps recording until you say so",
                       selected: chosen && !draft.autoStop) { chosen = true; draft.autoStop = false }
            Text("You can change this later in Settings.")
                .font(.callout).foregroundStyle(.secondary).padding(.top, 6)
        }
    }
}

private struct AssistantStep: View {
    let state: AppState
    @Binding var draft: Preferences
    @Binding var chosen: Bool

    var body: some View {
        VStack(spacing: 10) {
            ChoiceCard(title: "Yes, connect assistants", detail: "Read-only access to your transcripts",
                       selected: chosen && draft.mcpEnabled) { chosen = true; draft.mcpEnabled = true }
            ChoiceCard(title: "Not now", detail: "You can switch this on later in Settings",
                       selected: chosen && !draft.mcpEnabled) { chosen = true; draft.mcpEnabled = false }
            if draft.mcpEnabled, chosen {
                switch state.bridge.claudeExtension {
                case .installed:
                    Label("Claude Desktop is connected.", systemImage: "checkmark.circle.fill")
                        .foregroundStyle(.green).padding(.top, 8)
                case .notInstalled, .outdated:
                    Button("Add Muesli to Claude Desktop…") { state.bridge.installExtension() }
                        .disabled(BridgeController.extensionURL == nil)
                        .padding(.top, 8)
                case .claudeMissing:
                    Text("Claude Desktop isn't installed. Other assistants can be set up in Settings.")
                        .font(.callout).foregroundStyle(.secondary).padding(.top, 8)
                }
            }
        }
        .onAppear { state.bridge.refreshExtension() }
    }
}

private struct PermissionStep: View {
    let step: Step
    let access: Access

    var body: some View {
        Group {
            switch access {
            case .unknown:
                Text("macOS will ask you to confirm.").foregroundStyle(.secondary)
            case .granted:
                Label("Allowed", systemImage: "checkmark.circle.fill").foregroundStyle(.green)
            case .denied:
                Text("Not allowed. You can change this in System Settings, or skip it for now.")
                    .foregroundStyle(.orange).multilineTextAlignment(.center)
            }
        }
        .font(.callout)
        .padding(.top, 12)
    }
}
