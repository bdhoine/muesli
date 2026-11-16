// A fake backend so the pages can be developed and screenshotted in a plain browser:
//   index.html?mock=1&window=onboarding   (or settings, prompt)
(() => {
  if (!new URLSearchParams(location.search).has("mock")) return;
  const listeners = [];
  const state = {
    version: "0.3.0", platform: "windows", status: { kind: "idle" },
    prefs: { mcpEnabled: false, claudeDesktopConfigEntry: false, autoStop: false, endDelaySeconds: 60, recentLimit: 5, model: "small", onboardingDone: false },
    effectiveLanguages: ["nl", "en"], systemLanguages: ["nl", "en"],
    supportedLanguages: ["en", "nl", "fr", "de", "es", "it", "pt", "ja", "zh"].map((code) => ({ code, name: code })),
    model: { kind: "missing", model: "small" },
    models: [
      { id: "small", title: "Small", summary: "Good balance of speed and accuracy", bytes: 487601967, installed: false },
      { id: "medium", title: "Medium", summary: "More accurate, slower", bytes: 1533763059, installed: false },
      { id: "large", title: "Large", summary: "Most accurate, slowest, needs plenty of memory", bytes: 3095033483, installed: false },
    ],
    recent: [], transcriptsFolderIsCustom: false, transcriptsFolderLocked: false, transcriptsFolder: "C:\\Users\\me\\Documents\\MeetingTranscripts",
    assistants: { helper: "C:\\Program Files\\Muesli\\muesli-mcp.exe", configStatus: "",
      extension: { state: "notInstalled", version: null, bundled: true },
      clients: [{ id: "ClaudeCode", name: "Claude Code", instructions: "Run this in PowerShell.", terminal: true, snippet: "claude mcp add --scope user muesli -- 'C:\\Program Files\\Muesli\\muesli-mcp.exe'" }] },
  };
  const emit = () => listeners.forEach((f) => f({}));
  let timer = null;
  function download(id) {
    const m = state.models.find((x) => x.id === id);
    state.model = { kind: "downloading", model: id, received: 0, total: m.bytes };
    clearInterval(timer);
    timer = setInterval(() => {
      state.model.received += 9_000_000 + Math.floor(Math.random() * 4_000_000);
      if (state.model.received >= m.bytes) { clearInterval(timer); m.installed = true; state.model = { kind: "ready", path: "C:\\Users\\me\\AppData\\Roaming\\Muesli\\ggml-small.bin" }; }
      emit();
    }, 120);
    emit();
  }
  const commands = {
    get_state: () => JSON.parse(JSON.stringify(state)),
    get_prompt: () => ({ kind: "start", title: "Looks like a meeting: Weekly sync", body: "Detected via calendar. Start transcribing?" }),
    update_prefs: ({ prefs }) => { state.prefs = prefs; state.effectiveLanguages = prefs.languages || state.systemLanguages; state.transcriptsFolderIsCustom = !!prefs.transcriptsFolder; state.transcriptsFolder = prefs.transcriptsFolder || "C:\\Users\\me\\Documents\\MeetingTranscripts"; setTimeout(emit); },
    download_model: ({ model }) => download(model),
    cancel_download: () => { clearInterval(timer); state.model = { kind: "missing", model: state.prefs.model }; emit(); },
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
