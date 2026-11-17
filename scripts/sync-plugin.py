#!/usr/bin/env python3
"""Keeps the Claude plugin in plugin/ in step with the rest of the repo.

    python3 scripts/sync-plugin.py           write what is out of date
    python3 scripts/sync-plugin.py --check   fail if anything is out of date (used in CI)

What it syncs: the plugin's version (from the Cargo workspace version, also the app's version),
the plugin icon (from assets/brand) and the license (from the root LICENSE).
"""
import json
import os
import re
import shutil
import sys

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
plugin = os.path.join(root, "plugin")
manifest = os.path.join(plugin, ".claude-plugin", "plugin.json")

with open(os.path.join(root, "windows", "Cargo.toml")) as f:
    version = re.search(r'^version\s*=\s*"([^"]+)"', f.read(), re.M).group(1)

COPIES = {
    os.path.join(plugin, "icon.png"): os.path.join(root, "assets", "brand", "app-icon", "extension-icon-512.png"),
    os.path.join(plugin, "LICENSE"): os.path.join(root, "LICENSE"),
}


def same(a, b):
    with open(a, "rb") as x, open(b, "rb") as y:
        return x.read() == y.read()


def main():
    check = "--check" in sys.argv
    problems = []

    with open(manifest) as f:
        data = json.load(f)
    if data.get("version") != version:
        problems.append(f"plugin.json version is {data.get('version')}, the workspace version is {version}")
        if not check:
            data["version"] = version
            with open(manifest, "w") as f:
                json.dump(data, f, indent=2, ensure_ascii=False)
                f.write("\n")

    for target, source in COPIES.items():
        if not os.path.exists(target) or not same(target, source):
            problems.append(f"{os.path.relpath(target, root)} differs from {os.path.relpath(source, root)}")
            if not check:
                shutil.copyfile(source, target)

    if check and problems:
        print("The plugin is out of date; run python3 scripts/sync-plugin.py:")
        for p in problems:
            print("  -", p)
        sys.exit(1)
    print("Plugin is up to date." if not problems else "Updated: " + "; ".join(problems))


main()
