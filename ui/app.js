// Muesli's windows: main, settings, setup and prompt. Each renders the view state the engine
// gives it and sends intents back (see bridge.js). Nothing here decides what Muesli does; the
// only local state is what the user is typing (a new title) and which <details> are open.
(function () {
  const { label, view, send, onChange, inApp } = window.muesli;
  const root = document.getElementById("app");
  const local = { renaming: null, draft: "", term: "" };
  let current = null;

  // ------------------------------------------------------------------------------------------
  // Helpers

  const esc = (s) =>
    String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);

  const svg = (body, extra = "") =>
    `<svg aria-hidden="true" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" ${extra}>${body}</svg>`;
  const icons = {
    computer: svg('<rect x="3" y="4" width="18" height="12" rx="2"/><path d="M8 20h8M12 16v4"/>'),
    file: svg('<path d="M6 3h9l4 4v14H6z"/><path d="M9 12h7M9 16h5"/>'),
    noBot: svg('<circle cx="9" cy="8" r="3.5"/><path d="M3 20c0-3.3 2.7-6 6-6s6 2.7 6 6"/><path d="M17 6l4 4M21 6l-4 4"/>'),
    record: svg('<circle cx="12" cy="12" r="9"/><circle cx="12" cy="12" r="4" fill="currentColor"/>'),
    stop: '<svg aria-hidden="true" width="12" height="12" viewBox="0 0 12 12"><rect x="1" y="1" width="10" height="10" rx="2" fill="currentColor"/></svg>',
    chevron: '<svg class="chevron" aria-hidden="true" viewBox="0 0 10 10" fill="none" stroke="currentColor" stroke-width="1.6"><path d="M3 2l3 3-3 3"/></svg>',
    x: '<svg aria-hidden="true" viewBox="0 0 8 8" stroke="currentColor" stroke-width="1.5"><path d="M1 1l6 6M7 1L1 7"/></svg>',
  };

  /** A button that sends `intent` when clicked. */
  function button(text, intent, { kind = "", disabled = false, testid = "" } = {}) {
    return `<button class="btn ${kind}" ${disabled ? "disabled" : ""} ${testid ? `data-testid="${testid}"` : ""}
      data-intent='${esc(JSON.stringify(intent))}'>${esc(text)}</button>`;
  }

  const startOfDay = (d) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const time = (d) => d.toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit" });

  /** "Today", "Yesterday", a weekday within the week, else "17 Nov". */
  function day(iso, { long = false } = {}) {
    const d = new Date(iso);
    const days = Math.round((startOfDay(new Date()) - startOfDay(d)) / 86400000);
    if (days === 0) return "Today";
    if (days === 1) return "Yesterday";
    if (days > 1 && days < 7) return d.toLocaleDateString(undefined, { weekday: long ? "long" : "short" });
    return d.toLocaleDateString(undefined, { day: "numeric", month: long ? "long" : "short" });
  }

  /** In the list: the time for today, else the day. */
  function when(iso) {
    if (!iso) return "";
    const label = day(iso);
    return label === "Today" ? time(new Date(iso)) : label;
  }

  /** Above a transcript: "Today, 14:30". */
  function date(iso) {
    if (!iso) return "";
    return `${day(iso, { long: true })}, ${time(new Date(iso))}`;
  }

  function duration(minutes) {
    if (minutes == null) return "";
    if (minutes < 60) return `${minutes} min`;
    return `${Math.floor(minutes / 60)} h ${minutes % 60} min`;
  }

  /** How long a live meeting has run: "Just started", "12 min so far". */
  const soFar = (iso) => (minutesSince(iso) < 1 ? "Just started" : `${duration(minutesSince(iso))} so far`);

  const minutesSince = (iso) => Math.max(0, Math.round((Date.now() - new Date(iso).getTime()) / 60000));

  const meta = (...parts) => parts.filter(Boolean).map(esc).join(" · ");

  function progress(fraction) {
    if (fraction == null) return `<div class="progress indeterminate"><span></span></div>`;
    return `<div class="progress"><span style="width:${Math.round(fraction * 100)}%"></span></div>`;
  }

  const pct = (fraction) => `${Math.round((fraction ?? 0) * 100)}%`;

  function setSkin(words) {
    if (words && document.documentElement.dataset.skin !== words.skin) document.documentElement.dataset.skin = words.skin;
  }

  const isMac = () => document.documentElement.dataset.skin === "mac";

  /** macOS in the app: the window draws its title over this strip (title bar style Overlay). */
  const titlebar = () => (inApp && isMac() && label !== "prompt" ? `<div class="titlebar" data-tauri-drag-region></div>` : "");

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
    const mac = isMac();
    const reader = v.reader;
    const rows = [];
    for (const i of v.imports) {
      rows.push(`<li class="row import ${i.failed ? "failed" : ""}" data-testid="import-row">
        <div class="row-top"><span class="title">${esc(i.name)}</span>
          ${i.failed ? button("Dismiss", { type: "dismissImport", id: i.id }, { kind: "small" }) : button("Cancel", { type: "cancelImport", id: i.id }, { kind: "small", disabled: i.cancelling })}</div>
        ${i.running ? `<div class="bar">${progress(i.progress)}${i.progress != null ? `<span class="pct">${pct(i.progress)}</span>` : ""}</div>` : `<div class="sub">${esc(i.status)}</div>`}
      </li>`);
    }
    if (v.live) {
      rows.push(`<li class="row live ${v.live.selected ? "selected" : ""}" data-testid="live-row"
          data-intent='${esc(JSON.stringify({ type: "select", id: v.live.id }))}'>
        <div class="row-top"><span class="title">${esc(v.live.title)}</span><span class="when">Now</span></div>
        <div class="sub"><span class="dot red" aria-hidden="true"></span>${meta(v.live.status, minutesSince(v.live.startedAt) < 1 ? "just started" : duration(minutesSince(v.live.startedAt)))}</div></li>`);
    }
    for (const m of v.meetings) {
      rows.push(`<li class="row ${m.selected ? "selected" : ""}" data-testid="meeting-row"
          data-intent='${esc(JSON.stringify({ type: "select", id: m.id }))}'>
        <div class="row-top"><span class="title">${esc(m.title)}</span><span class="when">${esc(when(m.startedAt))}</span></div>
        <div class="sub">${meta(duration(m.durationMinutes), m.source)}${m.partial ? ' <span class="badge">Partial</span>' : ""}${m.failed ? ' <span class="badge">Failed</span>' : ""}</div></li>`);
    }

    let right = `<div class="empty">${esc(v.emptyText || "Select a transcript")}</div>`;
    if (reader) {
      const renaming = local.renaming === reader.id;
      const heading = renaming
        ? `<form class="rename" data-testid="rename-form"><input name="title" value="${esc(local.draft)}" aria-label="Title" autofocus>
             <button class="btn primary" type="submit">Save</button><button class="btn" type="button" data-local="cancel-rename">Cancel</button></form>`
        : `<h1 data-testid="reader-title">${esc(reader.title)}</h1>`;
      const actions = reader.live
        ? `<button class="btn primary" data-testid="stop" data-intent='${esc(JSON.stringify({ type: "stopAndSave" }))}'>${icons.stop}Stop and Save</button>`
        : reader.editable && !renaming
          ? `<button class="btn" data-local="rename" data-testid="rename">Rename</button>` + button("Delete", { type: "delete", id: reader.id }, { kind: "danger", testid: "delete" })
          : "";
      // Live: the newest line is still settling, so it is drawn faded (design: "Main window ·
      // transcribing").
      const lines = reader.lines.length
        ? reader.lines
            .map(
              (l, i, all) =>
                `<p class="line${reader.live && i === all.length - 1 ? " newest" : ""}"><span class="time">${esc(l.time)}</span><span><strong>${esc(l.speaker)}</strong>: ${esc(l.text)}</span></p>`,
            )
            .join("")
        : `<p class="empty-text">${esc(reader.emptyText || "")}</p>`;
      right = `<header class="reader-head">
          <div class="top">${heading}${actions ? `<div class="actions">${actions}</div>` : ""}</div>
          <div class="meta">
            ${reader.badge ? `<span class="pill live" data-testid="live-badge"><span class="dot"></span>${esc(reader.badge)}</span>` : ""}
            ${[date(reader.startedAt), reader.live ? soFar(reader.startedAt) : duration(reader.durationMinutes), reader.source, reader.attendees.length ? `With ${reader.attendees.join(", ")}` : ""]
              .filter(Boolean)
              .map((x) => `<span>${esc(x)}</span>`)
              .join("")}
          </div>
        </header>
        <article class="transcript" data-scroll="${reader.live ? "live" : "reader"}" data-testid="transcript">${lines}
          ${reader.live ? `<div class="listening" role="status"><span class="bounce" aria-hidden="true"><span></span><span></span><span></span></span>Listening · text follows each pause</div>` : ""}</article>`;
    }

    // macOS: the list carries its own two buttons, as in the design; Start and Settings live in
    // the menu bar menu. Windows: a command bar.
    const revealIntent = { type: "showInFileManager", id: reader && !reader.live ? reader.id : null };
    const toolbar = mac
      ? ""
      : `<div class="toolbar">
          ${v.transcribing ? "" : button("Start Transcribing", { type: "startTranscribing" }, { kind: "primary", disabled: !v.canStart, testid: "start" })}
          ${button("Transcribe Audio File…", { type: "chooseAudioFiles" }, { disabled: !v.canTranscribeFile, testid: "transcribe-file" })}
          ${button(v.showInFileManager, revealIntent, { testid: "reveal" })}
          <span class="grow"></span>
          ${button("Settings…", { type: "openSettings" }, { kind: "quiet" })}
        </div>`;
    const listHead = mac ? `<div class="list-head"><h1>Transcripts</h1></div>` : "";

    keepScroll(() => {
      root.innerHTML = `${titlebar()}<div class="main-window">
        ${toolbar}
        ${v.update ? `<div class="banner info" role="status" data-testid="update-banner"><span class="grow">${esc(v.update.text)}</span>
            ${button("Not now", { type: "dismissUpdate" }, { kind: "small quiet" })}${button("What's new", { type: "openReleaseNotes", version: v.update.version }, { kind: "small" })}${button("Restart", { type: "restartToUpdate" }, { kind: "small primary" })}</div>` : ""}
        ${v.extensionUpdate ? `<div class="banner info" role="status" data-testid="extension-update"><span class="grow">${esc(v.extensionUpdate)}</span>
            ${button("Not now", { type: "dismissExtensionUpdate" }, { kind: "small quiet" })}${button("Update", { type: "connectClaudeDesktop" }, { kind: "small primary" })}</div>` : ""}
        ${v.error ? `<div class="banner" role="alert">${esc(v.error)} ${button("Dismiss", { type: "dismissError" }, { kind: "small" })}</div>` : ""}
        <div class="split">
          <section class="sidebar" aria-label="Transcripts">${listHead}
            <ul class="list" data-scroll="list" data-testid="list">${rows.join("")}</ul></section>
          <main class="reader">${right}</main>
        </div>
        ${mac && !v.transcribing
          ? `<button class="btn primary fab" data-testid="start" ${v.canStart ? "" : "disabled"}
              data-intent='${esc(JSON.stringify({ type: "startTranscribing" }))}'>${icons.record}Start transcribing</button>`
          : ""}
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

  function pillClass(p) {
    if (p.granted) return "ok";
    if (p.access === "denied") return "warn";
    return "";
  }

  function permissionRows(rows, { tags }) {
    return rows
      .map(
        (p) => `<div class="setting" data-testid="permission-${p.permission}">
          <div class="text"><span class="title">${esc(p.title)}${tags ? (p.required ? ' <span class="badge">Required</span>' : ' <span class="badge quiet">Optional</span>') : ""}</span>
          <span class="sub">${tags || p.required ? "" : "Optional · "}${esc(p.detail)}</span></div>
          <div class="controls">
            ${p.access !== "notAsked" || !p.button ? `<span class="pill ${pillClass(p)}"><span class="dot"></span>${esc(p.badge)}</span>` : ""}
            ${
              p.button
                ? button(p.button, p.access === "notAsked" ? { type: "requestPermission", permission: p.permission } : { type: "openPermissionSettings", permission: p.permission })
                : ""
            }
          </div>
        </div>`,
      )
      .join("");
  }

  function languages(l, { reset = false } = {}) {
    const chips = l.chosen
      .map(
        (c) => `<span class="chip" data-testid="chip-${c.code}">${esc(c.name)}
          ${l.canRemove ? `<button class="chip-x" aria-label="Remove ${esc(c.name)}" data-intent='${esc(JSON.stringify({ type: "removeLanguage", code: c.code }))}'>${icons.x}</button>` : ""}</span>`,
      )
      .join("");
    const options = l.available.map((c) => `<option value="${esc(c.code)}">${esc(c.name)}</option>`).join("");
    return `<div class="chips">${chips}
      <select class="add-language" data-testid="add-language" aria-label="Add language"><option value="">+ Add language</option>${options}</select>
      ${reset ? `<span class="reset">${button("Reset", { type: "resetLanguages" }, { kind: "link", disabled: !l.canReset })}</span>` : ""}</div>`;
  }

  // Names and terms Whisper should spell right; AI assistants add to it too.
  function vocabulary(v) {
    const chips = v.terms
      .map(
        (t) => `<span class="chip" data-testid="term">${esc(t)}
          <button class="chip-x" aria-label="Remove ${esc(t)}" data-intent='${esc(JSON.stringify({ type: "removeTerm", term: t }))}'>${icons.x}</button></span>`,
      )
      .join("");
    return `${v.terms.length ? `<div class="chips">${chips}</div>` : ""}
      <form class="add-term" data-testid="add-term"><input type="text" name="term" value="${esc(local.term)}" maxlength="${v.maxLength}"
        placeholder="A name or term" aria-label="Name or term" autocomplete="off" spellcheck="false">
        <button class="btn" type="submit">Add</button></form>
      ${v.error ? `<span class="error-text" role="alert">${esc(v.error)}</span>` : ""}`;
  }

  function model(m) {
    const state = m.error ? "warn" : m.text === "Ready" ? "ok" : "";
    return `<div class="setting" data-testid="model">
      <div class="text"><span class="title">${esc(m.title)}</span>
      <span class="sub">${esc(m.detail)}${m.progress != null ? ` · ${esc(m.text)}` : ""}</span>
      ${m.progress != null ? progress(m.progress) : ""}
      ${m.error ? `<div class="error-text">${esc(m.error)}${m.url ? `<br><code>${esc(m.url)}</code>` : ""}</div>` : ""}</div>
      <div class="controls">
        ${m.progress == null ? `<span class="pill ${state}"><span class="dot"></span>${esc(m.text)}</span>` : ""}
        ${m.canRetry ? button("Download again", { type: "downloadModelAgain" }) : ""}
      </div>
    </div>`;
  }

  /** Settings > General: the version, updates and the automatic check. */
  function updates(u) {
    const status = u.status
      ? `<span class="pill ${esc(u.tone)}" data-testid="update-status"><span class="dot"></span>${esc(u.status)}</span>`
      : "";
    const version = `<div class="setting" data-testid="version">
        <div class="text"><span class="title">Muesli</span>
          ${status}
          ${u.progress != null ? progress(u.progress) : ""}
          ${u.restartHint ? `<span class="sub">${esc(u.restartHint)}</span>` : ""}</div>
        <div class="controls"><span class="version-number" data-testid="version-number">${esc(u.version)}</span>${button("What's new", { type: "openReleaseNotes", version: u.version }, { kind: "link" })}${u.restart ? button("Restart", { type: "restartToUpdate" }, { kind: "primary", disabled: !u.canRestart, testid: "update-restart" }) : ""}</div>
      </div>`;
    if (!u.available) return version;
    return `${version}
      <div class="setting"><div class="text"><span class="title">Updates</span>
        <span class="sub" data-testid="last-checked">${u.lastChecked ? `Last checked: ${esc(date(u.lastChecked))}` : "Not checked yet"}</span></div>
        <div class="controls">${button(u.checkLabel, { type: "checkForUpdates" }, { disabled: !u.canCheck, testid: "check-updates" })}</div></div>
      <label class="setting"><div class="text"><span class="title">Check for updates automatically</span><span class="sub">Once a day, from GitHub. Only your IP address is seen there.</span></div>
        <input type="checkbox" role="switch" data-testid="auto-update" ${u.autoCheck ? "checked" : ""}
          data-toggle='${esc(JSON.stringify({ type: "setAutoUpdateCheck" }))}'></label>`;
  }

  function settings(v) {
    const a = v.assistants;
    const open = [...root.querySelectorAll("details[open]")].map((d) => d.dataset.key);
    const assistants = a.enabled
      ? `${
          a.claudeDesktop
            ? `<div class="setting"><div class="text"><span class="title">Claude Desktop</span><span class="sub status" data-testid="claude-status"><span class="dot ${esc(a.claudeDesktop.tone)}"></span>${esc(a.claudeDesktop.status)}</span></div>
               ${a.claudeDesktop.button ? button(a.claudeDesktop.button, { type: "connectClaudeDesktop" }, { kind: "primary" }) : ""}</div>`
            : ""
        }
        <div class="setting stack">
        <details class="other-apps" data-key="other" ${open.includes("other") ? "open" : ""}><summary><span class="disclosure">${icons.chevron}Other apps…</span></summary><div class="stack">
          <div class="segmented" role="tablist" aria-label="Assistant">${a.assistants
            .map((c) => `<button role="tab" aria-selected="${c.selected}" class="seg ${c.selected ? "on" : ""}" data-intent='${esc(JSON.stringify({ type: "pickAssistant", name: c.name }))}'>${esc(c.name)}</button>`)
            .join("")}</div>
          <span class="sub">${esc(a.instructions)}</span>
          ${
            a.command
              ? `<div class="command"><pre data-testid="command">${esc(a.command)}</pre><button class="btn" data-local="copy">Copy</button></div>`
              : `<span class="sub">muesli-mcp is not next to the app in this build.</span>`
          }
          ${a.canAddToClaudeConfig ? `<div class="row-between"><span class="sub">Claude Desktop blocked by your organisation?</span>${button("Add to Claude's config file", { type: "addToClaudeConfig" })}</div>` : ""}
          ${a.claudeConfigStatus ? `<span class="sub">${esc(a.claudeConfigStatus)}</span>` : ""}
        </div></details></div>`
      : "";
    root.innerHTML = `${titlebar()}<div class="settings" data-scroll="settings">
      <h2>General</h2>
      <section class="card">
        <div class="setting"><div class="text"><span class="title">Transcripts folder</span><span class="sub path" data-testid="folder">${esc(v.transcriptsFolder)}</span></div>
          <div class="controls">
            ${button(`Show in ${v.words.fileManager}`, { type: "showInFileManager", id: null })}
            ${button("Change…", { type: "chooseTranscriptsFolder" }, { disabled: v.transcriptsFolderLocked })}
          </div></div>
        <label class="setting"><div class="text"><span class="title">${esc(v.words.startAtLogin)}</span><span class="sub">So Muesli is there when a meeting starts</span></div>
          <input type="checkbox" role="switch" data-testid="login" ${v.startAtLogin ? "checked" : ""}
            data-toggle='${esc(JSON.stringify({ type: "setStartAtLogin" }))}'></label>
        ${updates(v.updates)}
      </section>
      <h2>Languages</h2>
      <section class="card"><div class="setting stack">
        <span class="sub">Spoken in your meetings. For each stretch of speech Muesli picks the most likely one; fewer languages, fewer mistakes.</span>
        ${languages(v.languages, { reset: true })}
      </div></section>
      <h2>Vocabulary</h2>
      <section class="card"><div class="setting stack">
        <span class="sub">Names and terms Muesli should spell right. AI assistants can add to it when you correct them.</span>
        ${vocabulary(v.vocabulary)}
      </div></section>
      <h2>AI assistants</h2>
      <section class="card">
        <label class="setting"><div class="text"><span class="title">Let AI assistants use Muesli</span><span class="sub">${esc(a.privacy)}</span></div>
          <input type="checkbox" role="switch" data-testid="assistants" ${a.enabled ? "checked" : ""}
            data-toggle='${esc(JSON.stringify({ type: "setAssistantsEnabled" }))}'></label>
        ${assistants}
      </section>
      <details class="advanced" data-key="advanced" ${open.includes("advanced") ? "open" : ""}><summary><h2 class="disclosure">${icons.chevron}Advanced</h2></summary>
        <div class="stack">
          <section class="card">${model(v.model)}</section>
          <section class="card">${permissionRows(v.permissions, { tags: false })}</section>
        </div>
      </details>
    </div>`;
  }

  // ------------------------------------------------------------------------------------------
  // Setup

  const promiseIcons = [icons.computer, icons.file, icons.noBot];

  function setup(v) {
    const last = v.finish;
    const counter = v.counter;
    const m = v.model;
    let body = "";
    if (v.step === 0) {
      body = `<ul class="promises">${v.promises
        .map(
          (p, i) => `<li><span class="icon">${promiseIcons[i] || icons.computer}</span>
            <span class="text"><span class="title">${esc(p)}</span>${v.promiseDetails?.[i] ? `<span class="sub">${esc(v.promiseDetails[i])}</span>` : ""}</span></li>`,
        )
        .join("")}</ul>`;
    } else if (v.step === 1) {
      body = languages(v.languages);
    } else if (v.step === 2) {
      body = `<section class="card">${permissionRows(v.permissions, { tags: true })}</section>${
        v.permissionsNote
          ? `<p class="hint">${esc(v.permissionsNote)} ${button("Open privacy settings", { type: "openPermissionSettings", permission: "microphone" }, { kind: "link" })}</p>`
          : ""
      }`;
    } else {
      const model = m
        ? `<div class="model-card" data-testid="setup-model"><div class="row-between"><span>${esc(m.title)}${m.progress != null ? " · downloading" : m.text === "Ready" ? " · ready" : ""}</span>
            <span class="sub">${esc(m.progress != null ? pct(m.progress) : m.text)}</span></div>
            <div class="progress thick"><span style="width:${m.progress != null ? Math.round(m.progress * 100) : m.text === "Ready" ? 100 : 0}%"></span></div></div>`
        : "";
      body = `<div class="stack">${model}${v.notice ? `<p class="hint">${esc(v.notice)}</p>` : ""}</div>`;
    }
    if (v.hint) body += `<p class="hint">${esc(v.hint)}</p>`;
    const footerModel =
      m && !last
        ? `<div class="footer-model" data-testid="setup-model">${m.progress != null ? progress(m.progress) : ""}<span>${
            m.progress != null ? `Speech model downloading in the background · ${pct(m.progress)}` : `${esc(m.title)} · ${esc(m.text)}`
          }</span></div>`
        : "";
    const primary = v.finish ? { type: "finishSetup" } : { type: "setupNext" };
    root.innerHTML = `${titlebar()}<div class="setup step-${v.step}">
      <div class="setup-body">
        <div class="setup-top"><span class="mark-tile"><img alt="" src="brand/mark.svg"></span><span class="counter">${esc(counter)}</span></div>
        <div class="setup-head">
          <h1 data-testid="setup-title">${esc(v.title)}</h1>
          ${v.subtitle ? `<p class="subtitle">${esc(v.subtitle)}</p>` : ""}
        </div>
        ${body}
      </div>
      <footer>
        <div class="left">${footerModel}</div>
        <div class="buttons">
          ${v.canGoBack ? button("Back", { type: "setupBack" }, { kind: "big" }) : ""}
          ${button(v.primary, primary, { kind: "primary big", disabled: !v.primaryEnabled, testid: "setup-primary" })}
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
      <div class="body"><span class="mark-tile"><img alt="" src="brand/mark.svg"></span>
      <div class="grow"><div class="title">${esc(v.title)}</div><div class="sub">${esc(v.body)}</div></div></div>
      <div class="buttons">${button(v.decline, { type: "answerPrompt", accept: false })}${button(v.accept, { type: "answerPrompt", accept: true }, { kind: "primary" })}</div>
    </div>`;
  }

  // ------------------------------------------------------------------------------------------

  // Every re-render keeps the scroll position (the main window does it itself, for its lists).
  const renderers = Object.fromEntries(
    Object.entries({ main, settings, setup, prompt }).map(([name, fn]) => [name, name === "main" ? fn : (v) => keepScroll(() => fn(v))]),
  );

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
    if (event.target.matches(".add-term input")) local.term = event.target.value;
  });

  root.addEventListener("submit", (event) => {
    if (event.target.matches(".add-term")) {
      event.preventDefault();
      const term = local.term.trim();
      local.term = "";
      if (term) send({ type: "addTerm", term });
      renderers[label]?.(current);
      return;
    }
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
