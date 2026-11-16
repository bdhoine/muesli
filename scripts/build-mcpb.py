#!/usr/bin/env python3
"""Builds dist/Muesli.mcpb, the Claude Desktop extension, from a release muesli-mcp binary.

    python3 scripts/build-mcpb.py [path/to/muesli-mcp]

Works from any folder. Without an argument it takes windows/target/release/muesli-mcp, so run
`cargo build --release -p muesli-mcp` in windows/ first. The result goes to windows/dist/.
"""
import os
import subprocess
import sys
import zipfile

windows = sys.platform.startswith("win")
exe = "muesli-mcp.exe" if windows else "muesli-mcp"
root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
windows = os.path.join(root, "windows")
binary = os.path.abspath(sys.argv[1]) if len(sys.argv) > 1 else os.path.join(windows, "target", "release", exe)
icon = os.path.join(root, "assets", "brand", "app-icon", "AppIcon.iconset", "icon_128x128@2x.png")

manifest = subprocess.run([binary, "--mcpb-manifest"], check=True, capture_output=True, text=True).stdout
os.makedirs(os.path.join(windows, "dist"), exist_ok=True)
out = os.path.join(windows, "dist", "Muesli.mcpb")
with zipfile.ZipFile(out, "w", zipfile.ZIP_DEFLATED) as z:
    z.writestr("manifest.json", manifest)
    z.write(icon, "icon.png")
    info = zipfile.ZipInfo(f"server/{exe}")
    info.external_attr = 0o755 << 16
    info.compress_type = zipfile.ZIP_DEFLATED
    with open(binary, "rb") as f:
        z.writestr(info, f.read())
print(f"Wrote {out}")
