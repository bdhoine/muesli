#!/usr/bin/env bash
# Builds Muesli.app (menu bar app; notifications and permissions need a bundle).
set -euo pipefail
swift build -c release
APP=Muesli.app
rm -rf "$APP"; mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp .build/release/Muesli "$APP/Contents/MacOS/"
iconutil -c icns assets/brand/app-icon/AppIcon.iconset -o "$APP/Contents/Resources/Muesli.icns"
cp assets/brand/menubar/muesliTemplate.png assets/brand/menubar/muesliTemplate@2x.png "$APP/Contents/Resources/"
cp Info.plist "$APP/Contents/Info.plist"
IDENTITY="${SIGN_IDENTITY:-$(security find-identity -v -p codesigning | awk -F'"' '/Apple Development|Developer ID Application/ {print $2; exit}')}"
if [ -n "$IDENTITY" ]; then
  codesign --force --deep --sign "$IDENTITY" "$APP"
  echo "Signed with: $IDENTITY"
else
  codesign --force --deep --sign - "$APP"
  echo "No signing certificate found, signed ad hoc: macOS permissions reset on every rebuild."
fi
echo "Built $APP. Move it to /Applications and open it once."
