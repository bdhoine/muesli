#!/usr/bin/env bash
# Regenerates fixtures/ from the Swift app, the reference for the file formats and the MCP replies.
# Run this only when a format or an MCP tool changes on purpose, then make the Windows app's
# tests (windows/: cargo test) pass against the new files.
#
#   fixtures/core/           written by MuesliCore's encoders (FixtureGenerator.swift)
#   fixtures/mcp/support/    hand-written input data          (not touched)
#   fixtures/mcp/requests.jsonl  hand-written requests        (not touched)
#   fixtures/mcp/expected.jsonl  what the Swift muesli-mcp answers to those requests
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT/macos"

FIXTURE_OUT="$ROOT/fixtures/core" swift test --filter FixtureGenerator >/dev/null
echo "Wrote fixtures/core"

swift build --product muesli-mcp >/dev/null
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
cp -R "$ROOT/fixtures/mcp/support" "$WORK/support"       # the requests rename and delete meetings
MUESLI_SUPPORT_DIR="$WORK/support" TRANSCRIPT_DIR="$WORK/transcripts" \
  .build/debug/muesli-mcp < "$ROOT/fixtures/mcp/requests.jsonl" > "$ROOT/fixtures/mcp/expected.jsonl"
echo "Wrote fixtures/mcp/expected.jsonl"
