#!/usr/bin/env python3
"""Downloads LocalVQE's echo cancellation model, which the app bundles (never committed).

    python3 scripts/fetch-aec-model.py           into crates/aec/model/ (skipped when present)
    python3 scripts/fetch-aec-model.py <dir>     into <dir>

The file is checked against its SHA-256 (the one LocalVQE itself allows, also MODEL_SHA256 in
crates/aec/src/lib.rs). Run it before building the app; the Release workflow does.
"""
import hashlib
import os
import sys
import urllib.request

NAME = "localvqe-v1.4-aec-200K-f32.gguf"
SHA256 = "b6e43138588a83bfe903ab5e143b4020b91c1e1629f5a575ac5855ff0003c731"
URL = f"https://huggingface.co/LocalAI-io/LocalVQE/resolve/main/{NAME}"

root = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
target_dir = sys.argv[1] if len(sys.argv) > 1 else os.path.join(root, "crates", "aec", "model")
target = os.path.join(target_dir, NAME)


def digest(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for block in iter(lambda: f.read(1 << 20), b""):
            h.update(block)
    return h.hexdigest()


if os.path.exists(target) and digest(target) == SHA256:
    print(f"{NAME} is already there")
    sys.exit(0)

os.makedirs(target_dir, exist_ok=True)
partial = target + ".part"
print(f"Downloading {URL}")
with urllib.request.urlopen(URL) as response, open(partial, "wb") as out:
    while block := response.read(1 << 20):
        out.write(block)
got = digest(partial)
if got != SHA256:
    os.remove(partial)
    sys.exit(f"{NAME}: SHA-256 {got}, expected {SHA256}")
os.replace(partial, target)
print(f"Saved {target}")
