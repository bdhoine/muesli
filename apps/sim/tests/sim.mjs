// Starts muesli-sim on a free port and opens its pages in Chromium.
import { spawn } from "node:child_process";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";

const require = createRequire(import.meta.url);
const { chromium } = require("playwright");
const here = path.dirname(fileURLToPath(import.meta.url));
const binary = process.env.MUESLI_SIM || path.join(here, "../../../target/debug/muesli-sim" + (process.platform === "win32" ? ".exe" : ""));

export async function startSim({ scenario, skin = "mac", colorScheme = "light" }) {
  const child = spawn(binary, ["--port", "0", "--scenario", scenario, "--skin", skin, "--paused"], { stdio: ["ignore", "pipe", "inherit"] });
  const url = await new Promise((resolve, reject) => {
    child.on("error", reject);
    child.on("exit", (code) => reject(new Error(`muesli-sim exited with ${code}; build it with cargo build -p muesli-sim`)));
    child.stdout.on("data", (d) => {
      const m = String(d).match(/http:\/\/\S+/);
      if (m) resolve(m[0]);
    });
  });
  const browser = await chromium.launch();
  const context = await browser.newContext({ colorScheme, viewport: { width: 1000, height: 700 } });
  // Pages get the app's Content Security Policy from the simulator; anything it blocks would be
  // missing in the real app too, so every test fails on a violation.
  const violations = [];
  const post = (route, body) => fetch(url + route, { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) });
  return {
    url,
    /** Opens one of Muesli's windows, as the app would. */
    async open(window) {
      const page = await context.newPage();
      page.on("console", (m) => {
        if (/Content Security Policy/i.test(m.text())) violations.push(`${window}: ${m.text()}`);
      });
      await page.goto(`${url}ui/index.html?window=${window}`, { waitUntil: "domcontentloaded" });
      return page;
    },
    async simPage() {
      const page = await context.newPage();
      await page.goto(url, { waitUntil: "domcontentloaded" });
      return page;
    },
    /** Plays a step of the outside world (see apps/sim/src/world.rs). */
    async act(step) {
      const reply = await post("api/sim", step);
      if (!reply.ok) throw new Error(await reply.text());
    },
    async state() {
      return (await fetch(url + "api/sim")).json();
    },
    async view(window) {
      return (await fetch(`${url}api/view/${window}`)).json();
    },
    async stop() {
      await browser.close();
      child.kill();
      if (violations.length) throw new Error(`blocked by the app's Content Security Policy:\n${violations.join("\n")}`);
    },
  };
}

export async function eventually(check, what, timeout = 5000) {
  const start = Date.now();
  for (;;) {
    try {
      if (await check()) return;
    } catch (_) {}
    if (Date.now() - start > timeout) throw new Error(`timed out: ${what}`);
    await new Promise((r) => setTimeout(r, 50));
  }
}
