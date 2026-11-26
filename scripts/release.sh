#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Muesli's release process, from your Mac.

  scripts/release.sh setup [--certificate FILE.p12] [--identity NAME]
      Once: creates the updater key (~/.tauri/muesli.key) if it is missing, stores it and its
      password as repository secrets, and writes its public key into
      apps/desktop/tauri.release.conf.json (commit and push that). With --certificate, also
      stores the macOS signing certificate (exported from Keychain Access as .p12) and its
      identity, so CI signs the Mac app with it.

  scripts/release.sh run
      Checks that main is pushed and clean, the updater key is in place and the secrets exist,
      then starts the Release workflow for the version in Cargo.toml, follows it and prints
      the draft release. Publish the draft on GitHub after trying the installers.

  scripts/release.sh test-build
      Starts the Release workflow with test_build: installers as downloadable artifacts only.
EOF
}

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
config="apps/desktop/tauri.release.conf.json"
key="$HOME/.tauri/muesli.key"

step() { printf '\n==> %s\n' "$*"; }
fail() { echo "error: $*" >&2; exit 1; }

need_gh() {
  command -v gh >/dev/null || fail "the GitHub CLI (gh) is not installed"
  gh auth status >/dev/null 2>&1 || fail "gh is not logged in: run 'gh auth login'"
}

secret_names() { gh secret list --json name --jq '.[].name'; }

setup() {
  local certificate="" identity=""
  while [ $# -gt 0 ]; do
    case "$1" in
      --certificate) certificate="$2"; shift 2 ;;
      --identity) identity="$2"; shift 2 ;;
      *) usage; exit 1 ;;
    esac
  done
  need_gh
  for dir in /opt/homebrew/opt/rustup/bin "$HOME/.cargo/bin"; do [ -d "$dir" ] && PATH="$dir:$PATH"; done

  step "Updater key"
  if [ -f "$key" ]; then
    echo "Using the existing $key"
  else
    command -v cargo >/dev/null && cargo tauri --version >/dev/null 2>&1 || fail "the Tauri CLI is needed: cargo install tauri-cli --version '^2' --locked"
    mkdir -p "$(dirname "$key")"
    echo "Choose a password for the updater key and keep both in your password manager:"
    cargo tauri signer generate -w "$key"
  fi

  step "Secrets for the updater key"
  gh secret set TAURI_SIGNING_PRIVATE_KEY < "$key"
  echo "Type the updater key's password:"
  gh secret set TAURI_SIGNING_PRIVATE_KEY_PASSWORD

  step "Public key in $config"
  KEY_PUB="$(tr -d '\n' < "$key.pub")" CONFIG="$config" python3 - <<'PY'
import json, os, re
path, pub = os.environ["CONFIG"], os.environ["KEY_PUB"]
text = open(path).read()
text = re.sub(r'("pubkey"\s*:\s*)"[^"]*"', lambda m: m.group(1) + json.dumps(pub), text, count=1)
open(path, "w").write(text)
PY
  if git diff --quiet -- "$config"; then
    echo "Already set."
  else
    echo "Updated. Commit and push it: git commit -m 'Updater public key' $config && git push"
  fi

  if [ -n "$certificate" ]; then
    step "macOS signing certificate"
    [ -f "$certificate" ] || fail "$certificate does not exist"
    if [ -z "$identity" ]; then
      identity="$(security find-identity -v -p codesigning 2>/dev/null \
        | sed -nE 's/.*"((Apple Development|Developer ID Application): [^"]*)".*/\1/p' | head -n 1)"
    fi
    [ -n "$identity" ] || fail "no signing identity found; pass --identity 'Apple Development: …'"
    base64 -i "$certificate" | tr -d '\n' | gh secret set APPLE_CERTIFICATE
    echo "Type the .p12 password:"
    gh secret set APPLE_CERTIFICATE_PASSWORD
    printf '%s' "$identity" | gh secret set APPLE_SIGNING_IDENTITY
    echo "CI signs the Mac app as: $identity"
    echo "Delete $certificate now that it is stored."
  fi
}

check_ready() {
  need_gh
  step "Checks"
  [ "$(git rev-parse --abbrev-ref HEAD)" = "main" ] || fail "switch to main first"
  git fetch -q origin main --tags
  [ "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)" ] || fail "main differs from origin/main: push or pull first"
  git diff --quiet && git diff --cached --quiet || fail "uncommitted changes"
  version="$(python3 scripts/release.py version)"
  if git rev-parse -q --verify "refs/tags/v$version" >/dev/null; then fail "v$version is already released; main should be on the next version"; fi
  local released_config; released_config="$(git show "origin/main:$config")"
  if grep -q 'TODO(barry)' <<<"$released_config"; then fail "the updater public key is not set on main: run 'scripts/release.sh setup', commit and push"; fi
  local names; names="$(secret_names)"
  for name in TAURI_SIGNING_PRIVATE_KEY TAURI_SIGNING_PRIVATE_KEY_PASSWORD; do
    grep -qx "$name" <<<"$names" || fail "secret $name is missing: run 'scripts/release.sh setup'"
  done
  grep -qx APPLE_CERTIFICATE <<<"$names" || echo "warning: no APPLE_CERTIFICATE secret, the Mac app will be signed ad hoc"
  local tests
  tests="$(gh run list --workflow Tests --branch main --commit "$(git rev-parse HEAD)" --limit 1 --json conclusion --jq '.[0].conclusion // "none"')"
  [ "$tests" = "success" ] || echo "warning: Tests on this commit: $tests"
  python3 scripts/release.py notes > /dev/null
  echo "Ready to release $version"
}

start_and_watch() {
  local started; started="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  gh workflow run Release --ref main "$@"
  local id=""
  for _ in $(seq 1 30); do
    id="$(gh run list --workflow Release --branch main --event workflow_dispatch --limit 5 \
      --json databaseId,createdAt --jq "map(select(.createdAt >= \"$started\")) | .[0].databaseId // empty")"
    [ -n "$id" ] && break
    sleep 2
  done
  [ -n "$id" ] || fail "the run did not show up; look in the Actions tab"
  gh run watch "$id" --exit-status
}

run() {
  check_ready
  read -r -p "Release Muesli $version (tag v$version, draft release, main moves to the next version)? [y/N] " answer
  [ "$answer" = "y" ] || [ "$answer" = "Y" ] || { echo "Nothing done."; exit 0; }
  step "Release workflow"
  start_and_watch
  step "Draft release"
  gh release view "v$version" --json url,assets --jq '.url, (.assets[].name | "  " + .)'
  echo "Try the installers, then publish the draft as Latest on GitHub."
  git pull -q --ff-only origin main
}

test_build() {
  need_gh
  step "Test build"
  start_and_watch -f test_build=true
  echo "Download the installers from the run's artifacts (gh run download)."
}

case "${1:-}" in
  setup) shift; setup "$@" ;;
  run) run ;;
  test-build) test_build ;;
  -h|--help|"") usage ;;
  *) usage; exit 1 ;;
esac
