#!/usr/bin/env python3
"""Release helpers for .github/workflows/release.yml (Release).

  release.py version            the workspace version in Cargo.toml
  release.py changelog DATE     date this version's CHANGELOG section (or turn [Unreleased] into it)
  release.py notes              this version's CHANGELOG section, for the release notes
  release.py bump               move Cargo.toml to the next patch version and print it
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CARGO = ROOT / "Cargo.toml"
CHANGELOG = ROOT / "CHANGELOG.md"
REPO = "https://github.com/bdhoine/muesli"
VERSION_LINE = re.compile(r'(\[workspace\.package\]\s*\nversion\s*=\s*")([^"]+)(")')


def version() -> str:
    match = VERSION_LINE.search(CARGO.read_text())
    if not match:
        sys.exit("No version under [workspace.package] in Cargo.toml")
    return match.group(2)


def changelog(date: str) -> None:
    v = version()
    text = CHANGELOG.read_text()
    section = re.compile(rf"^## \[{re.escape(v)}\][^\n]*$", re.M)
    if section.search(text):
        text = section.sub(f"## [{v}] - {date}", text, count=1)
    elif "\n## [Unreleased]\n" in text:
        text = text.replace("\n## [Unreleased]\n", f"\n## [Unreleased]\n\n## [{v}] - {date}\n", 1)
    else:
        sys.exit("CHANGELOG.md has neither a section for this version nor [Unreleased]")
    text = re.sub(r"(?m)^\[Unreleased\]: .*$", f"[Unreleased]: {REPO}/compare/v{v}...HEAD", text, count=1)
    link = f"[{v}]: "
    if not re.search(rf"(?m)^{re.escape(link)}", text):
        tags = re.findall(r"(?m)^## \[(\d+\.\d+\.\d+)\]", text)
        older = [t for t in tags if t != v]
        target = f"{REPO}/compare/v{older[0]}...v{v}" if older else f"{REPO}/releases/tag/v{v}"
        text = re.sub(r"(?m)^(\[Unreleased\]: .*)$", rf"\1\n{link}{target}", text, count=1)
    text = re.sub(r"\n{3,}", "\n\n", text)
    CHANGELOG.write_text(text)


def notes() -> str:
    v = version()
    lines, on = [], False
    for line in CHANGELOG.read_text().splitlines():
        if re.match(rf"^## \[{re.escape(v)}\]", line):
            on = True
            continue
        if on and (line.startswith("## [") or re.match(r"^\[[^\]]+\]: ", line)):
            break
        if on:
            lines.append(line)
    return "\n".join(lines).strip() or "See CHANGELOG.md."


def bump() -> str:
    major, minor, patch = (int(p) for p in version().split("."))
    nxt = f"{major}.{minor}.{patch + 1}"
    CARGO.write_text(VERSION_LINE.sub(rf"\g<1>{nxt}\g<3>", CARGO.read_text(), count=1))
    return nxt


if __name__ == "__main__":
    command = sys.argv[1] if len(sys.argv) > 1 else ""
    if command == "version":
        print(version())
    elif command == "changelog" and len(sys.argv) == 3:
        changelog(sys.argv[2])
    elif command == "notes":
        print(notes())
    elif command == "bump":
        print(bump())
    else:
        sys.exit(__doc__)
