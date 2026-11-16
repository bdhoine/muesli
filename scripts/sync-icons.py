#!/usr/bin/env python3
"""Generates windows/src-tauri/icons/ from assets/brand, the single source for Muesli's icons.

    python3 scripts/sync-icons.py           write the icons
    python3 scripts/sync-icons.py --check   fail if they are out of date (used in CI)

Never edit windows/src-tauri/icons by hand: change assets/brand and run this script.
The .icns needs macOS (iconutil), so --check skips it.
"""
import os
import shutil
import struct
import subprocess
import sys

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
brand = os.path.join(root, "assets", "brand")
iconset = os.path.join(brand, "app-icon", "AppIcon.iconset")
out = os.path.join(root, "windows", "src-tauri", "icons")

# Target name -> source file.
COPIES = {
    "32x32.png": os.path.join(iconset, "icon_32x32.png"),
    "128x128.png": os.path.join(iconset, "icon_128x128.png"),
    "128x128@2x.png": os.path.join(iconset, "icon_128x128@2x.png"),
    "icon.png": os.path.join(brand, "app-icon", "app-icon-1024.png"),
    "tray.png": os.path.join(brand, "menubar", "muesliTemplate@2x.png"),
}
# Windows icon: PNG images of these pixel sizes in one .ico container.
ICO_SOURCES = [(16, "icon_16x16.png"), (32, "icon_32x32.png"), (64, "icon_32x32@2x.png"), (128, "icon_128x128.png"), (256, "icon_256x256.png")]


def read(path):
    with open(path, "rb") as f:
        return f.read()


def ico():
    images = [(size, read(os.path.join(iconset, name))) for size, name in ICO_SOURCES]
    header = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    entries, body = b"", b""
    for size, data in images:
        entries += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
        body += data
    return header + entries + body


def wanted():
    files = {name: read(src) for name, src in COPIES.items()}
    files["icon.ico"] = ico()
    return files


def main():
    files = wanted()
    if "--check" in sys.argv:
        stale = [n for n, data in files.items() if not os.path.exists(os.path.join(out, n)) or read(os.path.join(out, n)) != data]
        if stale:
            sys.exit(f"Out of date: {', '.join(stale)}. Run python3 scripts/sync-icons.py")
        print("Icons are up to date.")
        return
    os.makedirs(out, exist_ok=True)
    for name, data in files.items():
        with open(os.path.join(out, name), "wb") as f:
            f.write(data)
    if shutil.which("iconutil"):
        subprocess.run(["iconutil", "-c", "icns", iconset, "-o", os.path.join(out, "icon.icns")], check=True)
    else:
        print("iconutil not found (not macOS): icon.icns left as it is.")
    print(f"Wrote {len(files)} files to {os.path.relpath(out, root)}")


if __name__ == "__main__":
    main()
