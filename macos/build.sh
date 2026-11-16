#!/usr/bin/env bash
# Builds Muesli.app (menu bar app; notifications and permissions need a bundle).
# The bundle also carries muesli-mcp, the MCP server Claude talks to. It is the Rust
# binary from windows/crates/mcp, shared with the Windows app (needs a Rust toolchain).
set -euo pipefail
cd "$(dirname "$0")"                                      # works from any folder
BRAND=../assets/brand
export PATH="$PATH:$HOME/.cargo/bin:/opt/homebrew/opt/rustup/bin"   # rustup shims, if not already on PATH
command -v cargo >/dev/null || { echo "cargo not found: install Rust (https://rustup.rs)." >&2; exit 1; }
RUST_TARGET=aarch64-apple-darwin
../scripts/build-ffi.sh                                   # shared Rust core (XCFramework + Swift bindings)
swift build -c release
(cd ../windows && cargo build --release -p muesli-mcp --target "$RUST_TARGET")
APP=Muesli.app
rm -rf "$APP"; mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp .build/release/Muesli "$APP/Contents/MacOS/"
cp "../windows/target/$RUST_TARGET/release/muesli-mcp" "$APP/Contents/MacOS/"
iconutil -c icns $BRAND/app-icon/AppIcon.iconset -o "$APP/Contents/Resources/Muesli.icns"
cp $BRAND/menubar/muesliTemplate.png $BRAND/menubar/muesliTemplate@2x.png "$APP/Contents/Resources/"
cp Info.plist "$APP/Contents/Info.plist"

IDENTITY="${SIGN_IDENTITY:-$(security find-identity -v -p codesigning | awk -F'"' '/Apple Development|Developer ID Application/ {print $2; exit}')}"
SIGN_AS="${IDENTITY:--}"
codesign --force --sign "$SIGN_AS" "$APP/Contents/MacOS/muesli-mcp"

MCPB_STAGE=.build/mcpb
rm -rf "$MCPB_STAGE" .build/Muesli.mcpb; mkdir -p "$MCPB_STAGE/server"
cp "$APP/Contents/MacOS/muesli-mcp" "$MCPB_STAGE/server/"
"$APP/Contents/MacOS/muesli-mcp" --mcpb-manifest > "$MCPB_STAGE/manifest.json"
cp $BRAND/app-icon/AppIcon.iconset/icon_256x256@2x.png "$MCPB_STAGE/icon.png"
(cd "$MCPB_STAGE" && zip -X -q -r ../Muesli.mcpb manifest.json icon.png server)
cp .build/Muesli.mcpb "$APP/Contents/Resources/Muesli.mcpb"

codesign --force --deep --sign "$SIGN_AS" "$APP"
if [ -n "$IDENTITY" ]; then
  echo "Signed with: $IDENTITY"
else
  echo "No signing certificate found, signed ad hoc: macOS permissions reset on every rebuild."
fi
echo "Built $APP. Move it to /Applications and open it once."
