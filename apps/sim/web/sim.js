// The simulator page: the world's buttons, the tray menu, notifications and Muesli's windows
// (each the real ui/ page in a frame). All state comes from /api/sim and the engine.
const $ = (s) => document.querySelector(s);
const esc = (s) => String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);
const post = (url, body) => fetch(url, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) });
const act = (step) => post("/api/sim", step);
const intent = (i) => post("/api/intent", i);
const titles = { main: "Muesli", settings: "Muesli Settings", setup: "Welcome to Muesli", prompt: "Muesli (prompt window)" };

let state = null;

function render() {
  const s = state;
  $("#skin").value = s.skin;
  if (!$("#scenario").options.length) $("#scenario").innerHTML = s.scenarios.map((n) => `<option>${esc(n)}</option>`).join("");
  $("#scenario").value = s.scenario;
  $("#clock").textContent = new Date(s.clock).toLocaleString();
  $("#pause").textContent = s.running ? "Pause time" : "Run time";
  $("#notifications").checked = s.notifications;
  $("#model").textContent = `Speech model: ${s.model.kind}`;
  $("#tray-title").textContent = s.skin === "mac" ? "Menu bar" : "System tray";
  $("#tray-icon").classList.toggle("on", s.menu.transcribing);
  $("#menu").innerHTML = s.menu.items
    .map((i) =>
      i.kind === "separator" ? `<li class="separator"></li>`
      : i.kind === "label" ? `<li class="label">${esc(i.label)}</li>`
      : `<li class="action ${i.enabled ? "" : "disabled"}" data-tray="${esc(i.id)}" data-enabled="${i.enabled}">${esc(i.label)}${i.detail ? `<span class="detail">${esc(i.detail)}</span>` : ""}</li>`)
    .join("");
  // Notifications stay on screen until answered, as they do in Notification Center / Action Center.
  const box = $("#notification-box");
  const cards = [];
  if (s.notice) {
    cards.push(`<div class="card"><div class="t">${esc(s.notice.title)}</div><div>${esc(s.notice.body)}</div>
      <div class="buttons"><button data-do='{"do":"clickNotice"}'>${esc(s.notice.action || "Open")}</button>
      <button data-do='{"do":"dismissNotice"}'>Close</button></div></div>`);
  }
  if (s.question) {
    cards.push(`<div class="card"><div class="t">${esc(s.question.title)}</div><div>${esc(s.question.body)}</div>
      <div class="buttons"><button data-do='{"do":"answer","accept":true}'>${esc(s.question.accept)}</button>
      <button data-do='{"do":"answer","accept":false}'>${esc(s.question.decline)}</button></div></div>`);
  }
  box.hidden = cards.length === 0;
  $(".notification").innerHTML = cards.join("");
  $("#log").innerHTML = s.log.map((l) => `<li>${esc(l)}</li>`).join("");

  // Muesli's windows: keep frames that are still open, add new ones.
  const desktop = $("#desktop");
  for (const frame of [...desktop.children]) if (!s.windows.includes(frame.dataset.window)) frame.remove();
  for (const w of s.windows) {
    if (desktop.querySelector(`[data-window="${w}"]`)) continue;
    const frame = document.createElement("div");
    frame.className = `frame ${w}`;
    frame.dataset.window = w;
    frame.innerHTML = `<header><span>${titles[w]}</span><button data-do='{"do":"closeWindow","window":"${w}"}' aria-label="Close">×</button></header>
      <iframe title="${titles[w]}" name="${w}" src="/ui/index.html?window=${w}"></iframe>`;
    desktop.appendChild(frame);
  }
}

async function refresh() {
  state = await (await fetch("/api/sim")).json();
  render();
}

document.addEventListener("click", async (e) => {
  const step = e.target.closest("[data-do]");
  if (step) {
    await act(JSON.parse(step.dataset.do));
    return refresh();
  }
  const tray = e.target.closest("[data-tray]");
  if (tray && tray.dataset.enabled === "true") {
    await intent({ type: "tray", id: tray.dataset.tray });
    return refresh();
  }
});
$("#notifications").addEventListener("change", (e) => act({ do: "notifications", on: e.target.checked }).then(refresh));
$("#reset").addEventListener("click", () => act({ do: "reset", skin: $("#skin").value, scenario: $("#scenario").value }).then(refresh));
$("#skin").addEventListener("change", () => act({ do: "reset", skin: $("#skin").value, scenario: $("#scenario").value }).then(refresh));
$("#pause").addEventListener("click", () => act({ do: "pause", paused: state.running }).then(refresh));

let seen = null;
async function poll() {
  try {
    const { version } = await (await fetch("/api/version")).json();
    if (version !== seen) {
      seen = version;
      await refresh();
    }
  } catch (_) {}
  setTimeout(poll, 250);
}
poll();
