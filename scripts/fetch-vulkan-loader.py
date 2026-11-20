#!/usr/bin/env python3
"""Downloads the Vulkan loader (vulkan-1.dll) that the Windows app bundles next to muesli.exe.

    python3 scripts/fetch-vulkan-loader.py           into apps/desktop/resources/vulkan/
    python3 scripts/fetch-vulkan-loader.py <dir>     into <dir>

Muesli's Windows build uses the graphics card through Vulkan and links vulkan-1.dll. GPU drivers
install it, but a PC without one (a virtual machine, the basic display driver) would not start
Muesli at all; with the loader bundled it starts and transcribes on the CPU. The file comes from
LunarG's Vulkan Runtime components (open source: Apache 2.0 and MIT; the licence is copied too),
checked by SHA-256 and never committed. The Release and Tests workflows run this on Windows.
"""
import hashlib
import io
import os
import sys
import urllib.request
import zipfile

VERSION = "1.4.328.1"
URL = f"https://sdk.lunarg.com/sdk/download/{VERSION}/windows/vulkan-runtime-components.zip"
SHA256 = "293efbb2c11e60429df7770f40e3c5c811a7fa42af81de0cc41e8718ddb2b373"
FILES = {f"VulkanRT-X64-{VERSION}-Components/x64/vulkan-1.dll": "vulkan-1.dll", f"VulkanRT-X64-{VERSION}-Components/VulkanRT-License.txt": "VulkanRT-License.txt"}

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
target = sys.argv[1] if len(sys.argv) > 1 else os.path.join(root, "apps", "desktop", "resources", "vulkan")
stamp = os.path.join(target, "VERSION")

if all(os.path.exists(os.path.join(target, name)) for name in FILES.values()) and os.path.exists(stamp) and open(stamp).read().strip() == VERSION:
    print(f"The Vulkan loader {VERSION} is already there")
    sys.exit(0)

print(f"Downloading {URL}")
# LunarG's server turns away Python's default user agent.
request = urllib.request.Request(URL, headers={"User-Agent": "muesli-build"})
with urllib.request.urlopen(request) as response:
    data = response.read()
got = hashlib.sha256(data).hexdigest()
if got != SHA256:
    sys.exit(f"vulkan-runtime-components.zip: SHA-256 {got}, expected {SHA256}")
os.makedirs(target, exist_ok=True)
with zipfile.ZipFile(io.BytesIO(data)) as archive:
    for member, name in FILES.items():
        with archive.open(member) as source, open(os.path.join(target, name), "wb") as out:
            out.write(source.read())
with open(stamp, "w") as f:
    f.write(VERSION + "\n")
print(f"Saved vulkan-1.dll and its licence in {target}")
