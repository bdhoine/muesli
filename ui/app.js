// Muesli's windows: main, settings, setup and prompt. Each renders the view state the engine
// gives it and sends intents back (see bridge.js). Nothing here decides what Muesli does; the
// only local state is what the user is typing (a new title) and which <details> are open.
(function () {
  const { label, view, send, onChange } = window.muesli;
  const root = document.getElementById("app");
  const local = { renaming: null, draft: "" };
  let current = null;

  // ------------------------------------------------------------------------------------------
  // Helpers

  const esc = (s) =>
    String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);

  /** A button that sends `intent` when clicked. */
  function button(text, intent, { kind = "", disabled = false, testid = "" } = {}) {
    return `<button class="btn ${kind}" ${disabled ? "disabled" : ""} ${testid ? `data-testid="${testid}"` : ""}
      data-intent='${esc(JSON.stringify(intent))}'>${esc(text)}</button>`;
  }

  function date(iso) {
    if (!iso) return "";
    return new Date(iso).toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
  }

  function duration(minutes) {
    if (minutes == null) return "";
    if (minutes < 60) return `${minutes} min`;
    return `${Math.floor(minutes / 60)} h ${minutes % 60} min`;
  }

  const meta = (...parts) => parts.filter(Boolean).map(esc).join(" · ");

  function progress(fraction) {
    if (fraction == null) return `<div class="progress indeterminate"><span></span></div>`;
    return `<div class="progress"><span style="width:${Math.round(fraction * 100)}%"></span></div>`;
  }

  function setSkin(words) {
    if (words && document.documentElement.dataset.skin !== words.skin) document.documentElement.dataset.skin = words.skin;
  }

  /** Keeps scroll positions across a re-render. */
  function keepScroll(render) {
    const saved = [...root.querySelectorAll("[data-scroll]")].map((el) => [
      el.dataset.scroll,
      el.scrollTop,
      el.scrollTop + el.clientHeight >= el.scrollHeight - 4,
    ]);
    render();
    for (const [key, top, atBottom] of saved) {
      const el = root.querySelector(`[data-scroll="${key}"]`);
      if (!el) continue;
      el.scrollTop = key === "live" && atBottom ? el.scrollHeight : top;
    }
  }

  // ------------------------------------------------------------------------------------------
  // Main window

  function main(v) {
    const reader = v.reader;
    const rows = [];
    if (v.live) {
      rows.push(`<li class="row live ${v.live.selected ? "selected" : ""}" data-testid="live-row"
          data-intent='${esc(JSON.stringify({ type: "select", id: v.live.id }))}'>
        <span class="dot red" aria-hidden="true"></span>
        <div><div class="title">${esc(v.live.title)}</div><div class="sub">${esc(v.live.status)}</div></div></li>`);
    }
    for (const i of v.imports) {
      rows.push(`<li class="row import ${i.failed ? "failed" : ""}" data-testid="import-row">
        <div class="grow"><div class="title">${esc(i.name)}</div><div class="sub">${esc(i.status)}</div>
        ${i.running ? progress(i.progress) : ""}</div>
        ${i.failed ? button("Dismiss", { type: "dismissImport", id: i.id }, { kind: "small" }) : button("Cancel", { type: "cancelImport", id: i.id }, { kind: "small" })}
      </li>`);
    }
    for (const m of v.meetings) {
      rows.push(`<li class="row ${m.selected ? "selected" : ""}" data-testid="meeting-row"
          data-intent='${esc(JSON.stringify({ type: "select", id: m.id }))}'>
        <div><div class="title">${esc(m.title)}${m.partial ? ' <span class="badge">Partial</span>' : ""}${m.failed ? ' <span class="badge">Failed</span>' : ""}</div>
        <div class="sub">${meta(date(m.startedAt), duration(m.durationMinutes), m.source)}</div></div></li>`);
    }

    let right = `<div class="empty">${esc(v.emptyText || "Choose a transcript on the left.")}</div>`;
    if (reader) {
      const renaming = local.renaming === reader.id;
      const heading = renaming
        ? `<form class="rename" data-testid="rename-form"><input name="title" value="${esc(local.draft)}" aria-label="Title" autofocus>
             <button class="btn primary" type="submit">Rename</button><button class="btn" type="button" data-local="cancel-rename">Cancel</button></form>`
        : `<h1 data-testid="reader-title">${esc(reader.title)}</h1>`;
      const actions = reader.live
        ? button("Stop and Save", { type: "stopAndSave" }, { kind: "primary", testid: "stop" })
        : reader.editable && !renaming
          ? `<button class="btn" data-local="rename" data-testid="rename">Rename</button>` + button("Delete", { type: "delete", id: reader.id }, { testid: "delete" })
          : "";
      const lines = reader.lines.length
        ? reader.lines
            .map((l) => `<p class="line"><span class="time">[${esc(l.time)}]</span> <strong>${esc(l.speaker)}</strong>: ${esc(l.text)}</p>`)
            .join("")
        : `<p class="empty-text">${esc(reader.emptyText || "")}</p>`;
      right = `<header class="reader-head">
          <div class="grow">${heading}
            ${reader.badge ? `<span class="badge live-badge" data-testid="live-badge"><span class="dot red"></span>${esc(reader.badge)}</span>` : ""}
            <div class="sub">${meta(date(reader.startedAt), duration(reader.durationMinutes), reader.source)}</div>
            ${reader.attendees.length ? `<div class="sub">With ${esc(reader.attendees.join(", "))}</div>` : ""}
          </div>
          <div class="actions">${actions}</div>
        </header>
        <article class="transcript" data-scroll="${reader.live ? "live" : "reader"}" data-testid="transcript">${lines}</article>`;
    }

    keepScroll(() => {
      root.innerHTML = `<div class="main-window">
        <div class="toolbar">
          ${v.transcribing ? "" : button("Start Transcribing", { type: "startTranscribing" }, { kind: "primary", disabled: !v.canStart, testid: "start" })}
          ${button("Transcribe Audio File…", { type: "chooseAudioFiles" }, { disabled: !v.canTranscribeFile, testid: "transcribe-file" })}
          ${button(v.showInFileManager, { type: "showInFileManager", id: reader && !reader.live ? reader.id : null }, { testid: "reveal" })}
          <span class="grow"></span>
          ${button("Settings…", { type: "openSettings" }, { kind: "quiet" })}
        </div>
        ${v.error ? `<div class="banner" role="alert">${esc(v.error)} ${button("Dismiss", { type: "dismissError" }, { kind: "small" })}</div>` : ""}
        <div class="split">
          <ul class="list" data-scroll="list" data-testid="list">${rows.join("")}</ul>
          <section class="reader">${right}</section>
        </div>
        ${v.toast ? `<div class="toast" role="status" data-testid="toast">${esc(v.toast.text)}
            ${v.toast.undo ? button("Undo", { type: "undo" }, { kind: "small", testid: "undo" }) : ""}
            ${button("×", { type: "dismissToast" }, { kind: "small quiet" })}</div>` : ""}
      </div>`;
    });
    const input = root.querySelector(".rename input");
    if (input && document.activeElement !== input) {
      input.focus();
      input.setSelectionRange(input.value.length, input.value.length);
    }
  }

  // ------------------------------------------------------------------------------------------
  // Settings

  function permissionRows(rows) {
    return rows
      .map(
        (p) => `<div class="perm" data-testid="permission-${p.permission}">
          <div class="grow"><div class="title">${esc(p.title)} ${p.required ? '<span class="badge">Required</span>' : '<span class="badge quiet">Optional</span>'}</div>
          <div class="sub">${esc(p.detail)}</div></div>
          <span class="status ${p.granted ? "ok" : ""}"><span class="dot ${p.granted ? "green" : ""}"></span>${esc(p.badge)}</span>
          ${
            p.button
              ? button(p.button, p.access === "notAsked" ? { type: "requestPermission", permission: p.permission } : { type: "openPermissionSettings", permission: p.permission }, { kind: "small" })
              : ""
          }
        </div>`,
      )
      .join("");
  }

  function languages(l) {
    const chips = l.chosen
      .map(
        (c) => `<span class="chip" data-testid="chip-${c.code}">${esc(c.name)}
          ${l.canRemove ? `<button class="chip-x" aria-label="Remove ${esc(c.name)}" data-intent='${esc(JSON.stringify({ type: "removeLanguage", code: c.code }))}'>×</button>` : ""}</span>`,
      )
      .join("");
    const options = l.available.map((c) => `<option value="${esc(c.code)}">${esc(c.name)}</option>`).join("");
    return `<div class="chips">${chips}
      <select class="add-language" data-testid="add-language" aria-label="Add language"><option value="">+ Add language</option>${options}</select></div>`;
  }

  function model(m) {
    return `<div class="model" data-testid="model">
      <div class="grow"><div class="title">${esc(m.title)}</div>
      <div class="sub">${esc(m.text)}</div>
      ${m.progress != null ? progress(m.progress) : ""}
      ${m.error ? `<div class="error-text">${esc(m.error)}${m.url ? `<br><code>${esc(m.url)}</code>` : ""}</div>` : ""}</div>
      ${m.canRetry ? button("Download again", { type: "downloadModelAgain" }, { kind: "small" }) : ""}
    </div>`;
  }

  function settings(v) {
    const a = v.assistants;
    const open = [...root.querySelectorAll("details[open]")].map((d) => d.dataset.key);
    const assistants = a.enabled
      ? `${
          a.claudeDesktop
            ? `<div class="row-flat"><div class="grow"><div class="title">Claude Desktop</div><div class="sub">${esc(a.claudeDesktop.status)}</div></div>
               ${a.claudeDesktop.button ? button(a.claudeDesktop.button, { type: "connectClaudeDesktop" }, { kind: "primary small" }) : ""}</div>`
            : ""
        }
        <details data-key="other" ${open.includes("other") ? "open" : ""}><summary>Other apps…</summary>
          <div class="segmented" role="tablist">${a.assistants
            .map((c) => `<button role="tab" aria-selected="${c.selected}" class="seg ${c.selected ? "on" : ""}" data-intent='${esc(JSON.stringify({ type: "pickAssistant", name: c.name }))}'>${esc(c.name)}</button>`)
            .join("")}</div>
          <p class="sub">${esc(a.instructions)}</p>
          ${a.command ? `<pre class="command" data-testid="command">${esc(a.command)}</pre><button class="btn small" data-local="copy">Copy</button>` : `<p class="sub">muesli-mcp is not next to the app in this build.</p>`}
          ${a.canAddToClaudeConfig ? button("Add to Claude's config file", { type: "addToClaudeConfig" }, { kind: "small" }) : ""}
          ${a.claudeConfigStatus ? `<p class="sub">${esc(a.claudeConfigStatus)}</p>` : ""}
        </details>`
      : "";
    root.innerHTML = `<div class="settings" data-scroll="settings">
      <section><h2>General</h2>
        <div class="row-flat"><div class="grow"><div class="title">Transcripts folder</div><div class="sub path" data-testid="folder">${esc(v.transcriptsFolder)}</div></div>
          ${button(`Show in ${v.words.fileManager}`, { type: "showInFileManager", id: null }, { kind: "small" })}
          ${button("Change…", { type: "chooseTranscriptsFolder" }, { kind: "small", disabled: v.transcriptsFolderLocked })}</div>
        <label class="row-flat switch"><div class="grow title">${esc(v.words.startAtLogin)}</div>
          <input type="checkbox" role="switch" data-testid="login" ${v.startAtLogin ? "checked" : ""}
            data-toggle='${esc(JSON.stringify({ type: "setStartAtLogin" }))}'></label>
      </section>
      <section><h2>Languages</h2>
        <p class="sub">Muesli picks the most likely of these every 30 seconds.</p>
        ${languages(v.languages)}
        ${button("Reset", { type: "resetLanguages" }, { kind: "small quiet", disabled: !v.languages.canReset })}
      </section>
      <section><h2>AI assistants</h2>
        <label class="row-flat switch"><div class="grow"><div class="title">Let AI assistants use Muesli</div><div class="sub">${esc(a.privacy)}</div></div>
          <input type="checkbox" role="switch" data-testid="assistants" ${a.enabled ? "checked" : ""}
            data-toggle='${esc(JSON.stringify({ type: "setAssistantsEnabled" }))}'></label>
        ${assistants}
      </section>
      <details class="advanced" data-key="advanced" ${open.includes("advanced") ? "open" : ""}><summary><h2>Advanced</h2></summary>
        ${model(v.model)}
        <div class="perms">${permissionRows(v.permissions)}</div>
        <p class="sub">Muesli ${esc(v.version)}</p>
      </details>
    </div>`;
  }

  // ------------------------------------------------------------------------------------------
  // Setup

  function setup(v) {
    const dots = Array.from({ length: v.steps }, (_, i) => `<span class="dot ${i === v.step ? "on" : ""}" aria-hidden="true"></span>`).join("");
    let body = "";
    if (v.step === 0) {
      body = `<img class="mark" alt="" src="brand/mark.svg"><ul class="promises">${v.promises.map((p) => `<li>${esc(p)}</li>`).join("")}</ul>`;
    } else if (v.step === 1) {
      body = languages(v.languages);
    } else if (v.step === 2) {
      body = `<div class="perms">${permissionRows(v.permissions)}</div>${v.permissionsNote ? `<p class="sub note">${esc(v.permissionsNote)} ${button("Open privacy settings", { type: "openPermissionSettings", permission: "microphone" }, { kind: "link" })}</p>` : ""}`;
    } else {
      body = v.notice ? `<p class="notice">${esc(v.notice)}</p>` : "";
    }
    const footerModel = v.model ? `<div class="footer-model" data-testid="setup-model"><span>${esc(v.model.title)}: ${esc(v.model.text)}</span>${v.model.progress != null ? progress(v.model.progress) : ""}</div>` : "";
    const primary = v.step === 3 ? { type: "finishSetup" } : { type: "setupNext" };
    root.innerHTML = `<div class="setup step-${v.step}">
      <div class="setup-body">
        <h1 data-testid="setup-title">${esc(v.title)}</h1>
        ${v.subtitle ? `<p class="subtitle">${esc(v.subtitle)}</p>` : ""}
        ${body}
      </div>
      <footer>
        <div class="dots" data-testid="dots">${dots}</div>
        ${footerModel}
        <div class="buttons">
          ${v.canGoBack ? button("Back", { type: "setupBack" }, { kind: "quiet" }) : ""}
          ${button(v.primary, primary, { kind: "primary", disabled: !v.primaryEnabled, testid: "setup-primary" })}
        </div>
      </footer>
    </div>`;
  }

  // ------------------------------------------------------------------------------------------
  // Prompt

  function prompt(v) {
    if (!v) {
      root.innerHTML = "";
      return;
    }
    root.innerHTML = `<div class="prompt" data-testid="prompt">
      <img class="mark small" alt="" src="brand/mark.svg">
      <div class="grow"><div class="title">${esc(v.title)}</div><div class="sub">${esc(v.body)}</div>
      <div class="buttons">${button(v.decline, { type: "answerPrompt", accept: false })}${button(v.accept, { type: "answerPrompt", accept: true }, { kind: "primary" })}</div></div>
    </div>`;
  }

  // ------------------------------------------------------------------------------------------

  const renderers = { main, settings, setup, prompt };

  async function refresh() {
    const v = await view();
    current = v;
    setSkin(v?.words);
    document.body.className = `window-${label}`;
    renderers[label]?.(v);
  }

  root.addEventListener("click", (event) => {
    const localAction = event.target.closest("[data-local]");
    if (localAction) {
      const action = localAction.dataset.local;
      if (action === "rename") {
        local.renaming = current.reader.id;
        local.draft = current.reader.title;
      } else if (action === "cancel-rename") {
        local.renaming = null;
      } else if (action === "copy") {
        navigator.clipboard?.writeText(current.assistants.command);
        localAction.textContent = "Copied";
        return;
      }
      renderers[label]?.(current);
      return;
    }
    const target = event.target.closest("[data-intent]");
    if (!target || target.disabled) return;
    event.preventDefault();
    send(JSON.parse(target.dataset.intent));
  });

  root.addEventListener("input", (event) => {
    if (event.target.matches(".rename input")) local.draft = event.target.value;
  });

  root.addEventListener("submit", (event) => {
    if (!event.target.matches(".rename")) return;
    event.preventDefault();
    const title = local.draft.trim();
    const id = local.renaming;
    local.renaming = null;
    if (title) send({ type: "rename", id, title });
    renderers[label]?.(current);
  });

  root.addEventListener("keydown", (event) => {
    if (event.key === "Escape" && local.renaming) {
      local.renaming = null;
      renderers[label]?.(current);
    }
  });

  root.addEventListener("change", (event) => {
    const t = event.target;
    if (t.dataset.toggle) send({ ...JSON.parse(t.dataset.toggle), on: t.checked });
    if (t.matches(".add-language") && t.value) send({ type: "addLanguage", code: t.value });
  });

  // Permissions switched on in the system settings show up when the window comes back.
  window.addEventListener("focus", refresh);
  onChange(refresh);
  refresh();
})();
