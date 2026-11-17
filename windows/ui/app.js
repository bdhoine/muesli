// Muesli's windows: Settings, the first-launch assistant and the meeting prompt. Plain JS, no build step.
(() => {
  const tauri = window.__TAURI__;
  const invoke = (cmd, args) => tauri.core.invoke(cmd, args);

  let S = null; // the last state from the backend
  const ui = { page: "general", step: 0, chosen: { autoStop: false, assistants: false }, access: {}, client: 0, prompt: null, info: null, ownModel: null };

  // ---- tiny DOM helper -------------------------------------------------------------------
  function h(tag, attrs, ...kids) {
    const el = document.createElement(tag);
    for (const [k, v] of Object.entries(attrs || {})) {
      if (v === false || v == null) continue;
      if (k.startsWith("on")) el.addEventListener(k.slice(2), v);
      else if (k === "class") el.className = v;
      else if (k === "style") el.setAttribute("style", v);
      else if (v === true) el.setAttribute(k, "");
      else el.setAttribute(k, v);
    }
    for (const kid of kids.flat(Infinity)) {
      if (kid == null || kid === false) continue;
      el.append(kid.nodeType ? kid : document.createTextNode(kid));
    }
    return el;
  }

  const mb = (bytes) => `${Math.round(bytes / 1_000_000)} MB`;
  const size = (bytes) => (bytes >= 1e9 ? `${(bytes / 1e9).toFixed(1)} GB` : mb(bytes));
  const setPrefs = (patch) => invoke("update_prefs", { prefs: { ...S.prefs, ...patch } });
  const langName = (code) => {
    try { return new Intl.DisplayNames([navigator.language], { type: "language" }).of(code) || code; } catch { return code; }
  };
  const platformName = () => (S.platform === "windows" ? "Explorer" : "Finder");

  function toggle(on, onchange) {
    return h("label", { class: "switch" }, h("input", { type: "checkbox", checked: on, onchange: (e) => onchange(e.target.checked) }), h("span"));
  }
  function row(label, sub, control) {
    return h("div", { class: "row" },
      h("div", { class: "label" }, h("span", null, label), sub ? h("span", { class: "sub small" }, sub) : null),
      h("div", { class: "ctl" }, control));
  }
  const group = (...rows) => h("div", { class: "group" }, rows);
  const foot = (text) => h("p", { class: "footer" }, text);
  const dot = (kind) => h("span", { class: `status-dot ${kind}` });
  const status = (kind, text) => h("span", { class: "status" }, dot(kind), text);

  // A small ⓘ button that reveals extra explanation and actions. Which one is open is kept in
  // `ui.info`, so it survives the re-render that every state change causes.
  function info(id, ...content) {
    const open = ui.info === id;
    const side = id.startsWith("label:") ? " left" : ""; // after a row's title: open towards the right
    return h("span", { class: "info" + side + (open ? " open" : "") },
      h("button", { class: "info-btn", title: "More information", "aria-expanded": String(open),
        onclick: (e) => { e.stopPropagation(); ui.info = open ? null : id; render(); } }, "ⓘ"),
      open ? h("div", { class: "info-panel", onclick: (e) => e.stopPropagation() }, content) : null);
  }
  document.addEventListener("click", () => { if (ui.info) { ui.info = null; render(); } });

  // ---- shared pieces -----------------------------------------------------------------------

  // Settings: a switch for the computer's languages; the own list only shows while it is in use.
  function languageSettings() {
    const system = !S.prefs.languages;
    return [
      group(row("Use my computer's languages", system ? S.systemLanguages.map(langName).join(", ") : null,
        toggle(system, (v) => invoke("use_system_languages", { on: v })))),
      system ? null : languageChips(false)];
  }

  function languageChips(withLinks = true) {
    const chosen = S.effectiveLanguages;
    const custom = !!S.prefs.languages;
    const available = S.supportedLanguages.filter((l) => !chosen.includes(l.code)).sort((a, b) => langName(a.code).localeCompare(langName(b.code)));
    // Editing the list makes it the user's own; the system list stays one click away, and back.
    const own = (languages) => setPrefs({ languages });
    return h("div", { style: "display:flex;flex-direction:column;gap:14px" },
      h("div", { class: "chips" }, chosen.map((code) =>
        h("span", { class: "chip" }, langName(code),
          chosen.length > 1 ? h("button", { title: "Remove", onclick: () => own(chosen.filter((c) => c !== code)) }, "✕") : null))),
      h("div", { style: "display:flex;gap:12px;align-items:center" },
        h("select", { onchange: (e) => { if (e.target.value) own([...chosen, e.target.value]); } },
          h("option", { value: "" }, "+ Add a language"),
          available.map((l) => h("option", { value: l.code }, langName(l.code)))),
        !withLinks ? null : custom ? h("button", { class: "link", onclick: () => invoke("use_system_languages", { on: true }) }, "Use my computer's languages")
          : S.prefs.savedLanguages ? h("button", { class: "link", onclick: () => invoke("use_system_languages", { on: false }) }, "Use my own list again") : null),
      custom || !withLinks ? null : h("span", { class: "sub small" }, "Taken from your computer's settings."));
  }

  // ---- Settings window ---------------------------------------------------------------------
  const PAGES = [["general", "General", "⚙", "#6b5a4b"], ["transcription", "Transcription", "🎙", "#5b7a3a"], ["assistants", "AI Assistants", "✦", "#3b2a1e"], ["permissions", "Permissions", "🛡", "#6b5a4b"]];

  function settings() {
    const page = { general: generalPage, transcription: transcriptionPage, assistants: assistantsPage, permissions: permissionsPage }[ui.page]();
    return h("div", { class: "settings" },
      h("nav", { class: "sidebar" }, PAGES.map(([id, title, icon, color]) =>
        h("button", { class: "nav" + (ui.page === id ? " on" : ""), onclick: () => { ui.page = id; render(); } },
          h("span", { class: "tile", style: `background:${color}` }, icon), title))),
      h("main", { class: "content" }, h("h1", null, PAGES.find((p) => p[0] === ui.page)[1]), page));
  }

  function generalPage() {
    const p = S.prefs;
    return h("div", { style: "display:flex;flex-direction:column;gap:18px;flex:1" },
      group(
        row("Save transcripts to", S.transcriptsFolderLocked ? "Set by the TRANSCRIPT_DIR environment variable" : S.transcriptsFolder, [
          h("button", { disabled: S.transcriptsFolderLocked, onclick: async () => { const f = await invoke("choose_transcripts_folder"); if (f) setPrefs({ transcriptsFolder: f }); } }, "Choose…"),
          S.transcriptsFolderIsCustom && !S.transcriptsFolderLocked ? h("button", { class: "link", onclick: () => setPrefs({ transcriptsFolder: null }) }, "Use default") : null,
          h("button", { onclick: () => invoke("open_path", { path: S.transcriptsFolder }) }, `Show in ${platformName()}`)]),
        row("Recent transcripts in menu", "Shown in the tray menu, newest first",
          h("select", { onchange: (e) => setPrefs({ recentLimit: +e.target.value }) },
            [0, 3, 5, 10].map((n) => h("option", { value: n, selected: p.recentLimit === n }, n === 0 ? "Don't show" : String(n)))))),
      group(
        row("Stop automatically when the call ends", p.autoStop ? "The transcript is saved without asking" : "Muesli asks before it stops and saves", toggle(p.autoStop, (v) => setPrefs({ autoStop: v }))),
        p.autoStop ? row("Call has ended after", "How long other apps must leave the microphone free",
          h("select", { onchange: (e) => setPrefs({ endDelaySeconds: +e.target.value }) },
            [30, 60, 120, 300].map((s) => h("option", { value: s, selected: p.endDelaySeconds === s }, s < 120 ? `${s} seconds` : `${s / 60} minutes`)))) : null),
      h("p", { class: "footer about" }, `Muesli ${S.version} · `, h("button", { class: "link", onclick: () => invoke("open_repository") }, "GitHub")));
  }

  // What is in use and what is happening, in one line under the model name.
  function modelStatus() {
    const m = S.model;
    if (m.kind === "ready") return status("ok", "Ready");
    if (m.kind === "downloading") return status("warn", "Downloading. Transcribing is off until it is ready.");
    if (m.kind === "failed") return status("err", m.message);
    return status("warn", "Not downloaded yet. Transcribing is off.");
  }

  function transcriptionPage() {
    const p = S.prefs;
    if (ui.ownModel === null) ui.ownModel = !!p.customModelPath;
    const d = S.download;
    const frac = d && d.total > 0 ? Math.min(d.received / d.total, 1) : null;
    return h("div", { style: "display:flex;flex-direction:column;gap:18px" },
      group(
        h("div", { class: "row" },
          h("div", { class: "label" }, h("span", null, "Model"), h("span", { class: "sub small" }, modelStatus())),
          h("div", { class: "ctl" },
            h("span", { class: "sub" }, ui.ownModel ? "Own file" : `${S.modelInfo.title} · ${size(S.modelInfo.bytes)}`),
            info("model",
              h("p", null, "Transcription runs on this computer. Muesli only goes online to download the speech model once, and a small model that finds the parts of the audio that contain speech. Use your own file to transcribe with another model."),
              S.downloadError ? h("p", { class: "err-text" }, S.downloadError) : null,
              h("p", { class: "sub" }, "Models are stored in"),
              h("pre", { class: "snippet" }, S.modelsFolder),
              h("div", { class: "info-actions" },
                h("button", { onclick: () => invoke("open_models_folder") }, `Show in ${platformName()}`))))),
        d ? h("div", { class: "row", style: "flex-direction:column;align-items:stretch;gap:6px" },
          h("div", { class: "bar" + (frac == null ? " indeterminate" : "") }, h("i", { style: `width:${(frac || 0) * 100}%` })),
          h("div", { style: "display:flex;justify-content:space-between;align-items:center" },
            h("span", { class: "sub small mono-digits" }, `Downloading the speech model · ${mb(d.received)} of ${mb(d.total)}`),
            h("button", { onclick: () => invoke("cancel_download") }, "Cancel download"))) : null,
        !d && (S.model.kind === "missing" || S.model.kind === "failed") && !ui.ownModel
          ? row(S.model.kind === "failed" ? "Download failed" : "Speech model", null, h("button", { onclick: () => invoke("download_model") }, S.model.kind === "failed" ? "Try again" : "Download")) : null,
        row("Use my own model file", null, toggle(ui.ownModel, (v) => { ui.ownModel = v; if (!v && p.customModelPath) setPrefs({ customModelPath: null }); render(); })),
        ui.ownModel ? row("Model file", p.customModelPath || "Any whisper.cpp .bin file. None chosen yet.",
          h("button", { onclick: async () => { const f = await invoke("choose_model_file"); if (f) setPrefs({ customModelPath: f }); } }, p.customModelPath ? "Change…" : "Choose File…")) : null,
        S.legacyModels > 0 ? row("Old model files", `From earlier versions, ${size(S.legacyBytes)}. Muesli no longer uses them.`,
          h("button", { onclick: () => invoke("remove_legacy_models") }, "Remove")) : null),
      h("h2", { style: "font-size:15px" }, "Languages"),
      languageSettings(),
      foot("Muesli recognises which of these is spoken. Fewer languages means fewer mistakes."));
  }

  function assistantsPage() {
    const p = S.prefs;
    const a = S.assistants;
    const ext = a.extension;
    const claudeInstalled = ext.state !== "claudeMissing";
    const extInstalled = ext.state === "installed" || ext.state === "outdated";
    const client = a.clients[ui.client] || a.clients[0];
    const trash = S.platform === "windows" ? "Recycle Bin" : "Trash";
    const extStatus = ext.state === "installed" ? status("ok", "Installed") : ext.state === "outdated" ? status("warn", "Update available")
      : status("warn", p.claudeDesktopConfigEntry ? "Using the config file" : "Not installed");
    return h("div", { style: "display:flex;flex-direction:column;gap:18px" },
      group(h("div", { class: "row" },
        h("div", { class: "label title" }, "Let AI assistants use Muesli",
          info("label:mcp",
            h("p", null, `Assistants can read Muesli's status and transcripts, rename meetings, set attendees and speakers, and delete finished meetings (deleted files go to the ${trash}). They cannot start or stop a recording.`),
            h("p", null, "An assistant on this computer starts Muesli's small MCP server itself; nothing is exposed to the network. What an assistant reads is sent to its provider, for example Anthropic for Claude."))),
        h("div", { class: "ctl" }, toggle(p.mcpEnabled, (v) => setPrefs({ mcpEnabled: v }))))),
      p.mcpEnabled ? [
        claudeInstalled ? [
          h("h2", { style: "font-size:15px" }, "Claude Desktop"),
          group(
            h("div", { class: "row" },
              h("div", { class: "label title" }, "Muesli extension",
                info("label:claude",
                  h("p", null, "The extension lists Muesli in Claude Desktop with its name and icon. Claude Desktop asks you to confirm when you install it."),
                  ext.version ? h("p", { class: "sub" }, `Version ${ext.version} is installed.`) : null,
                  extInstalled ? null : [
                    h("p", null, "If Claude Desktop says the extension cannot be installed, your organisation blocks extensions. Muesli can add itself to Claude Desktop's config file instead: it is then listed as “Muesli” without an icon. Quit and reopen Claude Desktop afterwards. Installing the extension later removes the entry."),
                    h("div", { class: "info-actions" }, h("button", { onclick: () => setPrefs({ claudeDesktopConfigEntry: !p.claudeDesktopConfigEntry }) },
                      p.claudeDesktopConfigEntry ? "Remove from config file" : "Add to config file"))])),
              h("div", { class: "ctl" }, extStatus,
                ext.bundled && !extInstalled ? h("button", { class: "primary", onclick: () => invoke("install_extension") }, "Install…")
                  : ext.bundled && ext.state === "outdated" ? h("button", { onclick: () => invoke("install_extension") }, "Update…") : null)),
            p.claudeDesktopConfigEntry && !extInstalled ? row("Config file entry", a.configStatus || null,
              h("button", { onclick: () => setPrefs({ claudeDesktopConfigEntry: false }) }, "Remove")) : null)] : null,
        h("h2", { style: "font-size:15px" }, claudeInstalled ? "Other assistants" : "Set up an assistant"),
        client && client.snippet ? group(
          row("Assistant", null, h("select", { onchange: (e) => { ui.client = +e.target.value; render(); } }, a.clients.map((c, i) => h("option", { value: i, selected: i === ui.client }, c.name)))),
          h("div", { class: "row", style: "flex-direction:column;align-items:stretch;gap:8px" },
            h("span", { class: "sub small" }, client.instructions),
            h("pre", { class: "snippet" }, client.snippet),
            h("div", { style: "text-align:right" }, h("button", { onclick: (e) => { navigator.clipboard?.writeText(client.snippet); e.target.textContent = "Copied"; } }, "Copy"))))
          : group(row("muesli-mcp", "Missing next to the app. Reinstall Muesli.", [dot("err")])),
      ] : null);
  }

  // How Muesli stands with the operating system: one row per thing it needs from it.
  function permissionsPage() {
    const y = S.system;
    // The status is always at the right edge; a button to fix it sits beneath.
    const access = (value, settingsKind) => h("div", { class: "stack" },
      value === "allowed" ? status("ok", "Allowed") : value === "denied" ? status("err", "Not allowed") : status("off", "Unknown"),
      value === "allowed" ? null : h("button", { onclick: () => invoke("open_privacy_settings", { kind: settingsKind }) }, "Open settings…"));
    return h("div", { style: "display:flex;flex-direction:column;gap:18px" },
      group(
        row("Microphone", "To transcribe your side of the conversation", access(y.microphone, "microphone")),
        // Where the system asks nothing for this (Windows), there is nothing to show.
        y.systemAudioNeedsPermission ? row("System audio", "To transcribe the other side of a call. Muesli never records your screen.",
          h("div", { class: "stack" }, status("off", "Asked when you record"),
            h("button", { onclick: () => invoke("open_privacy_settings", { kind: "systemAudio" }) }, "Open settings…"))) : null,
        row("Notifications", "To ask whether to transcribe when a meeting starts", access(y.notifications, "notifications"))),
      group(row("Start at login", "So Muesli is there when a meeting starts", toggle(y.startAtLogin, (v) => invoke("set_start_at_login", { on: v })))),
      foot("Changes you make in your system settings show here when you come back to this window."));
  }

  // ---- onboarding assistant ----------------------------------------------------------------
  function steps() {
    const all = ["languages", "autoStop", "assistants", "microphone"];
    if (S.platform === "macos") all.push("systemAudio"); // Windows loopback needs no permission
    return all;
  }
  const META = {
    languages: ["🌐", "#6b5a4b", "Which languages do you speak?", "Muesli works out which one is spoken. Fewer languages means fewer mistakes."],
    autoStop: ["⏹", "#6b5a4b", "Stop when the call ends?", "Muesli can notice that the other app let go of the microphone and save the transcript for you, or ask you first."],
    assistants: ["✦", "#3b2a1e", "Connect an AI assistant?", "Claude and other assistants can read your transcripts. They can never start or stop a recording."],
    microphone: ["🎙", "#5b7a3a", "Hear your side", "Muesli needs the microphone to transcribe what you say."],
    systemAudio: ["🔊", "#6b5a4b", "Hear the other side", "To transcribe the other people on a call, Muesli listens to your computer's audio. It never records your screen."],
  };

  function choice(title, detail, on, onclick) {
    return h("button", { class: "choice" + (on ? " on" : ""), onclick }, h("div", null, h("div", { class: "t" }, title), h("div", { class: "sub small" }, detail)), h("span", { class: "mark" }, on ? "●" : "○"));
  }

  function onboarding() {
    const list = steps();
    const id = list[ui.step];
    const [icon, color, title, intro] = META[id];
    const p = S.prefs;
    let body;
    if (id === "languages") body = languageChips();
    else if (id === "autoStop") body = [
      choice("Stop and save automatically", "Once the call has ended, the transcript is saved without asking", ui.chosen.autoStop && p.autoStop, () => { ui.chosen.autoStop = true; setPrefs({ autoStop: true }); }),
      choice("Ask me first", "Muesli shows a prompt and keeps recording until you say so", ui.chosen.autoStop && !p.autoStop, () => { ui.chosen.autoStop = true; setPrefs({ autoStop: false }); }),
      h("p", { class: "footer" }, "You can change this later in Settings.")];
    else if (id === "assistants") body = [
      choice("Yes, connect assistants", "Read-only access to your transcripts", ui.chosen.assistants && p.mcpEnabled, () => { ui.chosen.assistants = true; setPrefs({ mcpEnabled: true }); }),
      choice("Not now", "You can switch this on later in Settings", ui.chosen.assistants && !p.mcpEnabled, () => { ui.chosen.assistants = true; setPrefs({ mcpEnabled: false }); }),
      p.mcpEnabled && ui.chosen.assistants && S.assistants.extension.state === "notInstalled" && S.assistants.extension.bundled
        ? h("button", { onclick: () => invoke("install_extension") }, "Add Muesli to Claude Desktop…") : null];
    else {
      const a = ui.access[id];
      body = [a === "granted" ? h("p", { class: "ok-text" }, "✓ Requested. If macOS or Windows asked, your answer is saved.")
        : a === "failed" ? h("p", { class: "warn-text" }, "That did not work. You can allow it in your system settings, or skip it for now.")
        : h("p", { class: "sub" }, "Your system will ask you to confirm.")];
      if (a === "failed") body.push(h("button", { onclick: () => invoke("open_privacy_settings", { kind: id }) }, "Open system settings"));
    }

    const last = ui.step === list.length - 1;
    const isPerm = id === "microphone" || id === "systemAudio";
    const needsAsk = isPerm && !ui.access[id];
    return h("div", { class: "onb" },
      h("div", { class: "hero hero-bg" }, h("div", { class: "icon", style: `background:${color}` }, icon), h("h1", null, title), h("p", null, intro)),
      h("div", { class: "body" }, body),
      h("div", { class: "foot" },
        h("div", { class: "dots" }, list.map((_, i) => h("i", { class: i === ui.step ? "on" : "" }))),
        ui.step > 0 ? h("button", { class: "big", onclick: () => { ui.step--; render(); } }, "Back") : null,
        needsAsk ? h("button", { class: "big", onclick: () => advance(list) }, "Skip") : null,
        h("button", { class: "primary big",
          onclick: () => (needsAsk ? askPermission(id) : advance(list)) },
          needsAsk ? `Allow ${id === "microphone" ? "Microphone" : "System Audio"}  →` : last ? "Start using Muesli  ✓" : "Continue  →")));
  }

  async function askPermission(id) {
    try { await invoke("request_permission", { kind: id }); ui.access[id] = "granted"; } catch { ui.access[id] = "failed"; }
    render();
  }
  async function advance(list) {
    if (ui.step === list.length - 1) { await invoke("finish_onboarding"); return; }
    ui.step++;
    render();
  }

  // ---- prompt window -----------------------------------------------------------------------
  function prompt() {
    const p = ui.prompt;
    if (!p) return h("div");
    const start = p.kind === "start";
    return h("div", { class: "prompt" },
      h("div", null, h("h3", null, p.title), h("p", { class: "sub" }, p.body)),
      h("div", { class: "actions" },
        h("button", { onclick: () => invoke("prompt_response", { accept: false }) }, "Not now"),
        h("button", { class: "primary", onclick: () => invoke("prompt_response", { accept: true }) }, start ? "Start transcribing" : "Stop and save")));
  }

  // ---- boot --------------------------------------------------------------------------------
  const label = () => new URLSearchParams(location.search).get("window") || tauri.window.getCurrentWindow().label;

  function render() {
    if (!S) return;
    const root = document.getElementById("app");
    const scroll = root.querySelector(".content, .body")?.scrollTop || 0;
    const view = { settings, onboarding, prompt }[label()] || settings;
    root.replaceChildren(view());
    const el = root.querySelector(".content, .body");
    if (el) el.scrollTop = scroll;
    document.title = { settings: "Muesli Settings", onboarding: "Welcome to Muesli", prompt: "Muesli" }[label()] || "Muesli";
  }

  let pending = false;
  async function refresh() {
    S = await invoke("get_state");
    if (label() === "prompt") ui.prompt = await invoke("get_prompt");
    if (!pending) { pending = true; requestAnimationFrame(() => { pending = false; render(); }); }
  }

  refresh();
  tauri.event.listen("state-changed", refresh);
  window.addEventListener("focus", refresh); // system settings may have changed meanwhile
})();
