#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Builds Muesli as a release app on this Mac and installs it.

  scripts/install-mac.sh [--to DIR] [--no-open]

  --to DIR    install into DIR (default: ~/Applications)
  --no-open   do not start Muesli afterwards

The app is signed with MUESLI_SIGNING_IDENTITY when set, else with the first
"Apple Development" or "Developer ID Application" certificate in your keychain,
else ad hoc (then macOS may ask for permissions again after every build).
EOF
}

dest="$HOME/Applications"
open_after=1
while [ $# -gt 0 ]; do
  case "$1" in
    --to) dest="$2"; shift 2 ;;
    --no-open) open_after=0; shift ;;
    -h|--help) usage; exit 0 ;;
    *) usage; exit 1 ;;
  esac
done

[ "$(uname)" = "Darwin" ] || { echo "This script is for macOS." >&2; exit 1; }
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

for dir in /opt/homebrew/opt/rustup/bin "$HOME/.cargo/bin"; do
  [ -d "$dir" ] && PATH="$dir:$PATH"
done
export PATH
command -v cargo >/dev/null || { echo "cargo not found: install Rust (https://rustup.rs)." >&2; exit 1; }
cargo tauri --version >/dev/null 2>&1 || { echo "Installing the Tauri CLI…"; cargo install tauri-cli --version "^2" --locked; }

step() { printf '\n==> %s\n' "$*"; }

step "Submodules and the echo cancellation model"
git submodule update --init --recursive
python3 scripts/fetch-aec-model.py

target="$(rustc -vV | sed -n 's/^host: //p')"
version="$(python3 scripts/release.py version)"

step "muesli-mcp and the Claude Desktop extension ($target)"
cargo build --release -p muesli-mcp
mkdir -p apps/desktop/binaries
cp target/release/muesli-mcp "apps/desktop/binaries/muesli-mcp-$target"
python3 scripts/build-mcpb.py target/release/muesli-mcp

identity="${MUESLI_SIGNING_IDENTITY:-}"
if [ -z "$identity" ]; then
  identity="$(security find-identity -v -p codesigning 2>/dev/null \
    | sed -nE 's/.*"((Apple Development|Developer ID Application): [^"]*)".*/\1/p' | head -n 1)"
fi

step "Muesli $version, signed with ${identity:-an ad-hoc signature}"
(
  cd apps/desktop
  if [ -n "$identity" ]; then export APPLE_SIGNING_IDENTITY="$identity"; fi
  cargo tauri build --bundles app --config tauri.release.conf.json --config '{"bundle":{"createUpdaterArtifacts":false}}'
)
app="target/release/bundle/macos/Muesli.app"
codesign --verify --strict "$app"

step "Installing into $dest"
if pgrep -x muesli >/dev/null; then
  osascript -e 'tell application id "com.github.bdhoine.muesli" to quit' >/dev/null 2>&1 || true
  for _ in 1 2 3 4 5 6 7 8 9 10; do pgrep -x muesli >/dev/null || break; sleep 0.5; done
  pkill -x muesli 2>/dev/null || true
fi
mkdir -p "$dest"
rm -rf "$dest/Muesli.app"
ditto "$app" "$dest/Muesli.app"

if [ "$open_after" = 1 ]; then
  open "$dest/Muesli.app"
fi
echo
echo "Installed Muesli $version at $dest/Muesli.app"
