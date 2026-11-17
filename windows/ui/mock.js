// A fake backend so the pages can be developed and screenshotted in a plain browser:
//   index.html?mock=1&window=onboarding   (or settings, prompt)
(() => {
  if (!new URLSearchParams(location.search).has("mock")) return;
  const listeners = [];
  const state = {
    version: "0.0.1", platform: "windows", status: { kind: "idle" },
    prefs: { mcpEnabled: false, claudeDesktopConfigEntry: false, autoStop: false, endDelaySeconds: 60, recentLimit: 5, onboardingDone: false },
    effectiveLanguages: ["nl", "en"], systemLanguages: ["nl", "en"],
    supportedLanguages: ["en", "nl", "fr", "de", "es", "it", "pt", "ja", "zh"].map((code) => ({ code, name: code })),
    model: { kind: "ready", path: "C:\\Users\\me\\AppData\\Local\\Muesli\\ggml-large-v3-turbo-q5_0.bin" },
    modelInfo: { title: "Large v3 Turbo", summary: "Accurate and fast, runs on your computer", bytes: 574041195 },
    download: null, downloadError: null,
    modelsFolder: "C:\\Users\\me\\AppData\\Local\\Muesli", legacyModels: 2, legacyBytes: 3583365450,
    system: { microphone: "allowed", notifications: "denied", systemAudioNeedsPermission: false, startAtLogin: true },
    recent: [], transcriptsFolderIsCustom: false, transcriptsFolderLocked: false, transcriptsFolder: "C:\\Users\\me\\Documents\\MeetingTranscripts",
    assistants: { helper: "C:\\Program Files\\Muesli\\muesli-mcp.exe", configStatus: "",
      extension: { state: "notInstalled", version: null, bundled: true },
      clients: [{ id: "ClaudeCode", name: "Claude Code", instructions: "Run this in PowerShell.", terminal: true, snippet: "claude mcp add --scope user muesli -- 'C:\\Program Files\\Muesli\\muesli-mcp.exe'" }] },
  };
  const emit = () => listeners.forEach((f) => f({}));
  let timer = null;
  function download() {
    const total = state.modelInfo.bytes;
    state.model = { kind: "downloading", received: 0, total };
    state.download = { received: 0, total };
    clearInterval(timer);
    timer = setInterval(() => {
      state.model.received += 9_000_000 + Math.floor(Math.random() * 4_000_000);
      state.download.received = state.model.received;
      if (state.model.received >= total) { clearInterval(timer); state.download = null; state.model = { kind: "ready", path: "C:\\Users\\me\\AppData\\Local\\Muesli\\ggml-large-v3-turbo-q5_0.bin" }; }
      emit();
    }, 120);
    emit();
  }
  const commands = {
    get_state: () => JSON.parse(JSON.stringify(state)),
    get_prompt: () => ({ kind: "start", title: "Looks like a meeting: Weekly sync", body: "Detected via calendar. Start transcribing?" }),
    update_prefs: ({ prefs }) => { state.prefs = prefs; state.effectiveLanguages = prefs.languages || state.systemLanguages; state.transcriptsFolderIsCustom = !!prefs.transcriptsFolder; state.transcriptsFolder = prefs.transcriptsFolder || "C:\\Users\\me\\Documents\\MeetingTranscripts"; setTimeout(emit); },
    download_model: () => download(),
    use_system_languages: ({ on }) => {
      if (on) { state.prefs.savedLanguages = state.prefs.languages; state.prefs.languages = null; }
      else state.prefs.languages = state.prefs.savedLanguages || state.systemLanguages;
      state.effectiveLanguages = state.prefs.languages || state.systemLanguages; emit();
    },
    set_start_at_login: ({ on }) => { state.system.startAtLogin = on; emit(); },
    remove_legacy_models: () => { state.legacyModels = 0; state.legacyBytes = 0; emit(); },
    cancel_download: () => { clearInterval(timer); state.download = null; if (state.model.kind !== "ready") state.model = { kind: "missing" }; emit(); },
    request_permission: () => new Promise((ok) => setTimeout(ok, 400)),
    choose_model_file: () => "C:\\models\\mine.bin",
    choose_transcripts_folder: () => "D:\\Transcripts",
  };
  window.__TAURI__ = {
    core: { invoke: async (cmd, args) => (commands[cmd] ? commands[cmd](args || {}) : undefined) },
    event: { listen: (name, f) => { listeners.push(f); } },
    window: { getCurrentWindow: () => ({ label: "settings" }) },
  };
})();
