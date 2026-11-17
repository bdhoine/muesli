#!/usr/bin/env node
// Starts Muesli's MCP server for the plugin, on macOS and on Windows.
//
// A plugin's MCP command cannot differ per operating system, so the plugin starts this file with
// Node, and this file finds the `muesli-mcp` program that ships inside the installed Muesli app
// and runs it, passing standard input and output straight through. It opens no connection and
// downloads nothing.
//
// Where the program is, in this order:
//   1. MUESLI_MCP, if set (a path).
//   2. The file `mcp-path` in Muesli's support folder, which the app rewrites on every start with
//      the exact location of its own copy.
//   3. The usual install locations.
"use strict";

const { spawn } = require("node:child_process");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

const windows = process.platform === "win32";
const programName = windows ? "muesli-mcp.exe" : "muesli-mcp";

function supportFolder(env, platform, home) {
  if (env.MUESLI_SUPPORT_DIR) return env.MUESLI_SUPPORT_DIR;
  if (platform === "win32") return path.join(env.APPDATA || path.join(home, "AppData", "Roaming"), "Muesli");
  return path.join(home, "Library", "Application Support", "Muesli");
}

function usualLocations(env, platform, home) {
  if (platform === "win32") {
    const local = env.LOCALAPPDATA || path.join(home, "AppData", "Local");
    return [
      path.join(local, "Muesli", "muesli-mcp.exe"),
      path.join(local, "Programs", "Muesli", "muesli-mcp.exe"),
      path.join(env.ProgramFiles || "C:\\Program Files", "Muesli", "muesli-mcp.exe"),
    ];
  }
  return [
    "/Applications/Muesli.app/Contents/MacOS/muesli-mcp",
    path.join(home, "Applications", "Muesli.app", "Contents", "MacOS", "muesli-mcp"),
  ];
}

function isFile(p) {
  try {
    return fs.statSync(p).isFile();
  } catch {
    return false;
  }
}

/** Candidate programs, best first. Exported for the tests. */
function candidates(env = process.env, platform = process.platform, home = os.homedir()) {
  const list = [];
  if (env.MUESLI_MCP) list.push(env.MUESLI_MCP);
  try {
    const recorded = fs.readFileSync(path.join(supportFolder(env, platform, home), "mcp-path"), "utf8").trim();
    if (recorded) list.push(recorded);
  } catch {
    // the app has not recorded a location yet
  }
  return list.concat(usualLocations(env, platform, home));
}

function find(env, platform, home) {
  return candidates(env, platform, home).find(isFile);
}

function main() {
  const program = find();
  if (!program) {
    process.stderr.write(
      "Muesli's MCP server was not found. Install the Muesli app and open it once, then try again.\n" +
        "Looked in:\n  " + candidates().join("\n  ") + "\n"
    );
    process.exit(1);
  }
  const child = spawn(program, process.argv.slice(2), { stdio: "inherit", windowsHide: true });
  child.on("error", (e) => {
    process.stderr.write(`Could not start ${program}: ${e.message}\n`);
    process.exit(1);
  });
  child.on("exit", (code, signal) => process.exit(signal ? 1 : code ?? 0));
  for (const signal of ["SIGINT", "SIGTERM"]) process.on(signal, () => child.kill(signal));
}

if (require.main === module) main();
module.exports = { candidates, find, supportFolder, programName };
