#!/usr/bin/env bash
# Regenerates fixtures/core from the Swift app's encoders (the reference for the file formats
# until the Swift core is replaced by the shared Rust library).
# Run this only when a format changes on purpose, then make the Rust tests (windows/: cargo test) pass.
#
#   fixtures/core/               written by MuesliCore's encoders (FixtureGenerator.swift)
#   fixtures/mcp/support/        hand-written input data          (not touched)
#   fixtures/mcp/requests.jsonl  hand-written requests            (not touched)
#   fixtures/mcp/expected.jsonl  not generated: muesli-mcp is Rust on both platforms, so the replies
#                                are the frozen contract. When a tool changes on purpose, update
#                                this file by hand and keep crates/mcp/tests/contract.rs passing.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT/macos"

FIXTURE_OUT="$ROOT/fixtures/core" swift test --filter FixtureGenerator >/dev/null
echo "Wrote fixtures/core"
