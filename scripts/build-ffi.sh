#!/usr/bin/env bash
# Builds the shared Rust core for the macOS app: windows/crates/ffi becomes
#   macos/Frameworks/MuesliFFI.xcframework        (static library + C header + module map)
#   macos/Sources/MuesliFFI/muesli_ffi.swift      (generated Swift bindings)
# Both are generated and git-ignored. Run before `swift build` / `swift test`; macos/build.sh does it.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export PATH="$PATH:$HOME/.cargo/bin:/opt/homebrew/opt/rustup/bin"
command -v cargo >/dev/null || { echo "cargo not found: install Rust (https://rustup.rs)." >&2; exit 1; }
TARGET=aarch64-apple-darwin
cd "$ROOT/windows"

cargo build --release -p muesli-ffi --features whisper --target "$TARGET"
LIB="target/$TARGET/release/libmuesli_ffi.a"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
cargo run --quiet --release -p muesli-ffi --features cli --bin uniffi-bindgen -- \
  generate --library "$LIB" --language swift --out-dir "$WORK/gen"

mkdir -p "$ROOT/macos/Sources/MuesliFFI" "$WORK/headers"
cp "$WORK/gen/muesli_ffi.swift" "$ROOT/macos/Sources/MuesliFFI/muesli_ffi.swift"
cp "$WORK/gen/muesli_ffiFFI.h" "$WORK/headers/"
cp "$WORK/gen/muesli_ffiFFI.modulemap" "$WORK/headers/module.modulemap"

rm -rf "$ROOT/macos/Frameworks/MuesliFFI.xcframework"
xcodebuild -create-xcframework -library "$LIB" -headers "$WORK/headers" \
  -output "$ROOT/macos/Frameworks/MuesliFFI.xcframework" >/dev/null
echo "Built macos/Frameworks/MuesliFFI.xcframework and macos/Sources/MuesliFFI/muesli_ffi.swift"
