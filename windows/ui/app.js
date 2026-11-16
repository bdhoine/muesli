// Muesli's windows: Settings, the first-launch assistant and the meeting prompt. Plain JS, no build step.
(() => {
  const tauri = window.__TAURI__;
  const invoke = (cmd, args) => tauri.core.invoke(cmd, args);

  let S = null; // the last state from the backend
  const ui = { page: "general", step: 0, chosen: { model: false, autoStop: false, assistants: false }, access: {}, client: 0, prompt: null };

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

  // ---- shared pieces -----------------------------------------------------------------------
  function modelProgress() {
    const m = S.model;
    if (m.kind === "downloading") {
      const frac = m.total > 0 ? Math.min(m.received / m.total, 1) : null;
      return h("div", { style: "display:flex;flex-direction:column;gap:6px" },
        h("div", { class: "bar" + (frac == null ? " indeterminate" : "") }, h("i", { style: `width:${(frac || 0) * 100}%` })),
        h("span", { class: "sub small mono-digits" }, `Downloading ${m.model} · ${mb(m.received)} of ${mb(m.total)}`));
    }
    if (m.kind === "ready") return h("span", { class: "ok-text" }, "✓ Ready. You can carry on.");
    if (m.kind === "failed") return h("span", { class: "err-text" }, m.message);
    return null;
  }

  function languageChips() {
    const chosen = S.effectiveLanguages;
    const custom = !!S.prefs.languages;
    const available = S.supportedLanguages.filter((l) => !chosen.includes(l.code)).sort((a, b) => langName(a.code).localeCompare(langName(b.code)));
    return h("div", { style: "display:flex;flex-direction:column;gap:14px" },
      h("div", { class: "chips" }, chosen.map((code) =>
        h("span", { class: "chip" }, langName(code),
          chosen.length > 1 ? h("button", { title: "Remove", onclick: () => setPrefs({ languages: chosen.filter((c) => c !== code) }) }, "✕") : null))),
      h("div", { style: "display:flex;gap:12px;align-items:center" },
        h("select", { onchange: (e) => { if (e.target.value) setPrefs({ languages: [...chosen, e.target.value] }); } },
          h("option", { value: "" }, "+ Add a language"),
          available.map((l) => h("option", { value: l.code }, langName(l.code)))),
        custom ? h("button", { class: "link", onclick: () => setPrefs({ languages: null }) }, "Back to my computer's languages") : null),
      custom ? null : h("span", { class: "sub small" }, "Taken from your computer's settings."));
  }

  // ---- Settings window ---------------------------------------------------------------------
  const PAGES = [["general", "General", "⚙", "#6b5a4b"], ["transcription", "Transcription", "🎙", "#5b7a3a"], ["assistants", "AI Assistants", "✦", "#3b2a1e"]];

  function settings() {
    const page = { general: generalPage, transcription: transcriptionPage, assistants: assistantsPage }[ui.page]();
    return h("div", { class: "settings" },
      h("nav", { class: "sidebar" }, PAGES.map(([id, title, icon, color]) =>
        h("button", { class: "nav" + (ui.page === id ? " on" : ""), onclick: () => { ui.page = id; render(); } },
          h("span", { class: "tile", style: `background:${color}` }, icon), title))),
      h("main", { class: "content" }, h("h1", null, PAGES.find((p) => p[0] === ui.page)[1]), page));
  }

  function generalPage() {
    const p = S.prefs;
    return h("div", { style: "display:flex;flex-direction:column;gap:18px" },
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
      foot(`Muesli ${S.version}`));
  }

  function transcriptionPage() {
    const p = S.prefs;
    const custom = !!p.customModelPath;
    const m = S.model;
    const installed = S.models.find((x) => x.id === p.model)?.installed;
    const statusRow = m.kind === "ready" ? row("Transcriber", m.path, [dot("ok"), "Ready"])
      : m.kind === "failed" ? row("Transcriber", m.message, [dot("err"), "Problem"])
      : m.kind === "downloading" ? null
      : row("Transcriber", "Not downloaded yet. Transcribing is off until it is.", [dot("warn"), "Missing"]);
    return h("div", { style: "display:flex;flex-direction:column;gap:18px" },
      group(
        custom ? null : row("Model", S.models.find((x) => x.id === p.model).summary,
          h("select", { onchange: (e) => setPrefs({ model: e.target.value }) },
            S.models.map((x) => h("option", { value: x.id, selected: p.model === x.id }, `${x.title} · ${size(x.bytes)}${x.installed ? " · installed" : ""}`)))),
        statusRow,
        m.kind === "downloading" ? h("div", { class: "row" }, modelProgress()) : null,
        h("div", { class: "row", style: "justify-content:flex-end" },
          m.kind === "downloading" ? h("button", { onclick: () => invoke("cancel_download") }, "Cancel download")
            : custom ? null
            : installed ? h("button", { class: "danger", onclick: () => invoke("delete_model", { model: p.model }) }, "Delete")
            : h("button", { class: "primary", onclick: () => invoke("download_model", { model: p.model }) }, `Download ${p.model}`)),
        row("Your own model", custom ? p.customModelPath : "Optional: use any whisper.cpp .bin file",
          [h("button", { onclick: async () => { const f = await invoke("choose_model_file"); if (f) setPrefs({ customModelPath: f }); } }, custom ? "Change…" : "Choose File…"),
           custom ? h("button", { class: "link", onclick: () => setPrefs({ customModelPath: null }) }, "Use downloaded model") : null])),
      foot("Runs on this computer. Muesli only goes online to download a model you pick here. Changes apply from the next recording."),
      h("h2", { style: "font-size:15px" }, "Languages"),
      languageChips(),
      foot("Muesli recognises which of these is spoken. Fewer languages means fewer mistakes."));
  }

  function assistantsPage() {
    const p = S.prefs;
    const a = S.assistants;
    const ext = a.extension;
    const extInstalled = ext.state === "installed" || ext.state === "outdated";
    const client = a.clients[ui.client] || a.clients[0];
    return h("div", { style: "display:flex;flex-direction:column;gap:18px" },
      group(row("Let AI assistants use Muesli", "They can read and manage finished transcripts, never start or stop a recording.", toggle(p.mcpEnabled, (v) => setPrefs({ mcpEnabled: v })))),
      p.mcpEnabled ? [
        group(
          row("Muesli extension",
            ext.state === "claudeMissing" ? "Claude Desktop is not installed on this computer" : ext.state === "notInstalled" ? "Not installed yet" : `Version ${ext.version} installed`,
            ext.state !== "claudeMissing" && ext.bundled ? h("button", { onclick: () => invoke("install_extension") }, ext.state === "notInstalled" ? "Install in Claude Desktop…" : "Reinstall…") : null),
          !extInstalled && ext.state !== "claudeMissing" ? row("Add to Claude Desktop's config file instead", "For accounts that don't allow extensions. Quit and reopen Claude Desktop after changing this.", toggle(p.claudeDesktopConfigEntry, (v) => setPrefs({ claudeDesktopConfigEntry: v }))) : null),
        p.claudeDesktopConfigEntry && a.configStatus ? foot(a.configStatus) : null,
        h("h2", { style: "font-size:15px" }, "Set up another assistant"),
        client && client.snippet ? group(
          row("Assistant", null, h("select", { onchange: (e) => { ui.client = +e.target.value; render(); } }, a.clients.map((c, i) => h("option", { value: i, selected: i === ui.client }, c.name)))),
          h("div", { class: "row", style: "flex-direction:column;align-items:stretch;gap:8px" },
            h("span", { class: "sub small" }, client.instructions),
            h("pre", { class: "snippet" }, client.snippet),
            h("div", { style: "text-align:right" }, h("button", { onclick: (e) => { navigator.clipboard?.writeText(client.snippet); e.target.textContent = "Copied"; } }, "Copy"))))
          : group(row("muesli-mcp", "Missing next to the app. Reinstall Muesli.", [dot("err")])),
      ] : null);
  }

  // ---- onboarding assistant ----------------------------------------------------------------
  function steps() {
    const all = ["model", "languages", "autoStop", "assistants", "microphone"];
    if (S.platform === "macos") all.push("systemAudio"); // Windows loopback needs no permission
    return all;
  }
  const META = {
    model: ["⬇", "#5b7a3a", "Pick a speech model", "Muesli transcribes on your computer. It downloads the model once, then works offline."],
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
    if (id === "model") {
      body = [S.models.map((m) => choice(`${m.title} · ${size(m.bytes)}`, m.summary + (m.id === "small" ? " (recommended)" : ""), ui.chosen.model && p.model === m.id, () => chooseModel(m.id))),
        ui.chosen.model ? h("div", { style: "margin-top:8px" }, modelProgress()) : null];
    } else if (id === "languages") body = languageChips();
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
        h("button", { class: "primary big", disabled: id === "model" && !ui.chosen.model,
          onclick: () => (needsAsk ? askPermission(id) : advance(list)) },
          needsAsk ? `Allow ${id === "microphone" ? "Microphone" : "System Audio"}  →` : last ? "Start using Muesli  ✓" : "Continue  →")));
  }

  async function chooseModel(id) {
    ui.chosen.model = true;
    render();
    await setPrefs({ model: id });
    // Starting a download replaces one that is still running for another model.
    if (!S.models.find((x) => x.id === id).installed) await invoke("download_model", { model: id });
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
})();
