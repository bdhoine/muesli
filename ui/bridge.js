// How a page talks to the engine. Inside the app (Tauri) through two commands, `view` and
// `intent`, and the `state-changed` event. In the simulator (a plain browser) through
// `/api/view/<window>`, `/api/intent` and a change counter it polls.
//
// The page never decides anything: it asks for its window's view state and sends intents back.
(function () {
  const tauri = window.__TAURI__;
  const params = new URLSearchParams(location.search);

  function windowLabel() {
    if (!tauri) return params.get("window") || "main";
    try {
      return tauri.window.getCurrentWindow().label;
    } catch (_) {
      return window.__TAURI_INTERNALS__?.metadata?.currentWindow?.label || "main";
    }
  }

  const label = windowLabel();

  async function view() {
    if (tauri) return tauri.core.invoke("view", { window: label });
    const reply = await fetch(`/api/view/${label}`);
    return reply.json();
  }

  async function send(intent) {
    if (tauri) return tauri.core.invoke("intent", { intent });
    await fetch("/api/intent", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(intent) });
  }

  /** Calls `refresh` whenever the engine says something changed. */
  function onChange(refresh) {
    if (tauri) {
      tauri.event.listen("state-changed", refresh);
      return;
    }
    let seen = null;
    async function poll() {
      try {
        const reply = await fetch("/api/version");
        const { version } = await reply.json();
        if (version !== seen) {
          seen = version;
          refresh();
        }
      } catch (_) {
        /* the simulator restarted; keep trying */
      }
      setTimeout(poll, 200);
    }
    poll();
  }

  window.muesli = { label, view, send, onChange, inApp: Boolean(tauri) };
})();
