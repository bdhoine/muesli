// Tests for plugin/server/muesli-mcp.js: node --test scripts/plugin-launcher.test.js
"use strict";
const test = require("node:test");
const assert = require("node:assert");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");
const { spawnSync } = require("node:child_process");

const launcher = path.join(__dirname, "..", "plugin", "server", "muesli-mcp.js");
const { candidates } = require(launcher);

function temp() {
  return fs.mkdtempSync(path.join(os.tmpdir(), "muesli-launcher-"));
}

test("macOS: recorded location first, then the usual ones", () => {
  const home = temp();
  const support = path.join(home, "Library", "Application Support", "Muesli");
  fs.mkdirSync(support, { recursive: true });
  fs.writeFileSync(path.join(support, "mcp-path"), "/Volumes/Apps/Muesli.app/Contents/MacOS/muesli-mcp\n");
  const list = candidates({}, "darwin", home);
  assert.strictEqual(list[0], "/Volumes/Apps/Muesli.app/Contents/MacOS/muesli-mcp");
  assert.ok(list.includes("/Applications/Muesli.app/Contents/MacOS/muesli-mcp"));
});

test("Windows: support folder in APPDATA, usual places under LOCALAPPDATA", () => {
  const home = temp();
  const appdata = path.join(home, "roaming");
  fs.mkdirSync(path.join(appdata, "Muesli"), { recursive: true });
  fs.writeFileSync(path.join(appdata, "Muesli", "mcp-path"), "D:\\Tools\\Muesli\\muesli-mcp.exe");
  const list = candidates({ APPDATA: appdata, LOCALAPPDATA: path.join(home, "local") }, "win32", home);
  assert.strictEqual(list[0], "D:\\Tools\\Muesli\\muesli-mcp.exe");
  assert.ok(list.some((p) => p.endsWith(path.join("Muesli", "muesli-mcp.exe"))));
});

test("MUESLI_MCP wins over everything", () => {
  assert.strictEqual(candidates({ MUESLI_MCP: "/x/mcp" }, "darwin", temp())[0], "/x/mcp");
});

test("runs the recorded program with stdin and stdout passed through", { skip: process.platform === "win32" }, () => {
  const dir = temp();
  const fake = path.join(dir, "muesli-mcp");
  fs.writeFileSync(fake, '#!/bin/sh\nread line\necho "got:$line"\nexit 3\n', { mode: 0o755 });
  const support = path.join(dir, "support");
  fs.mkdirSync(support);
  fs.writeFileSync(path.join(support, "mcp-path"), fake);
  const run = spawnSync(process.execPath, [launcher], {
    input: "ping\n",
    encoding: "utf8",
    env: { ...process.env, MUESLI_SUPPORT_DIR: support, MUESLI_MCP: "" },
  });
  assert.strictEqual(run.stdout.trim(), "got:ping");
  assert.strictEqual(run.status, 3, "the exit code is passed on");
});

test("says what to do when Muesli is not installed", { skip: process.platform === "win32" }, () => {
  const run = spawnSync(process.execPath, [launcher], {
    encoding: "utf8",
    env: { ...process.env, MUESLI_SUPPORT_DIR: temp(), HOME: temp(), MUESLI_MCP: "" },
  });
  if (fs.existsSync("/Applications/Muesli.app/Contents/MacOS/muesli-mcp")) return; // installed here: it would just start
  assert.strictEqual(run.status, 1);
  assert.match(run.stderr, /Install the Muesli app/);
});
