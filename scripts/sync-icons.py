#!/usr/bin/env python3
"""Generates apps/desktop/icons/ and ui/brand/ from assets/brand, the single source for Muesli's icons.

    python3 scripts/sync-icons.py           write the icons
    python3 scripts/sync-icons.py --check   fail if they are out of date (used in CI)

Never edit apps/desktop/icons by hand: change assets/brand and run this script.
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
out = os.path.join(root, "apps", "desktop", "icons")

# Target name -> source file.
COPIES = {
    "32x32.png": os.path.join(iconset, "icon_32x32.png"),
    "128x128.png": os.path.join(iconset, "icon_128x128.png"),
    "128x128@2x.png": os.path.join(iconset, "icon_128x128@2x.png"),
    "icon.png": os.path.join(brand, "app-icon", "app-icon-1024.png"),
    "tray.png": os.path.join(brand, "menubar", "muesliTemplate@2x.png"),
    "tray-transcribing.png": os.path.join(brand, "menubar", "muesliTranscribing@2x.png"),
    "tray-light.png": os.path.join(brand, "menubar", "muesliTrayLight@2x.png"),
    "tray-light-transcribing.png": os.path.join(brand, "menubar", "muesliTrayLightTranscribing@2x.png"),
    "tray-update.png": os.path.join(brand, "menubar", "muesliUpdate@2x.png"),
    "tray-light-update.png": os.path.join(brand, "menubar", "muesliTrayLightUpdate@2x.png"),
}
# The web UI's copies of the logo (ui/brand/), also generated.
UI_OUT = os.path.join(root, "ui", "brand")
UI_COPIES = {
    "mark.svg": os.path.join(brand, "logo", "mark.svg"),
    "mark-on-dark.svg": os.path.join(brand, "logo", "mark-on-dark.svg"),
}
# Windows icon: PNG images of these pixel sizes in one .ico container. They are the tile without
# the macOS icon grid's transparent margin (assets/brand/app-icon/windows), so the icon fills its
# square in the taskbar like other Windows apps.
windows_icons = os.path.join(brand, "app-icon", "windows")
ICO_SOURCES = [(n, f"icon_{n}.png") for n in (16, 24, 32, 48, 64, 256)]


def read(path):
    with open(path, "rb") as f:
        return f.read()


def ico():
    images = [(size, read(os.path.join(windows_icons, name))) for size, name in ICO_SOURCES]
    header = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    entries, body = b"", b""
    for size, data in images:
        entries += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
        body += data
    return header + entries + body


def wanted():
    files = {os.path.join(out, name): read(src) for name, src in COPIES.items()}
    files[os.path.join(out, "icon.ico")] = ico()
    files.update({os.path.join(UI_OUT, name): read(src) for name, src in UI_COPIES.items()})
    return files


def main():
    files = wanted()
    if "--check" in sys.argv:
        stale = [os.path.relpath(p, root) for p, data in files.items() if not os.path.exists(p) or read(p) != data]
        if stale:
            sys.exit(f"Out of date: {', '.join(stale)}. Run python3 scripts/sync-icons.py")
        print("Icons are up to date.")
        return
    for path, data in files.items():
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "wb") as f:
            f.write(data)
    if shutil.which("iconutil"):
        subprocess.run(["iconutil", "-c", "icns", iconset, "-o", os.path.join(out, "icon.icns")], check=True)
    else:
        print("iconutil not found (not macOS): icon.icns left as it is.")
    print(f"Wrote {len(files)} files to apps/desktop/icons and ui/brand")


if __name__ == "__main__":
    main()
