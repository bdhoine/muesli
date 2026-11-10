#!/usr/bin/env bash
# Builds MeetingScribe.app (menu bar app; notifications and permissions need a bundle).
set -euo pipefail
swift build -c release
APP=MeetingScribe.app
rm -rf "$APP"; mkdir -p "$APP/Contents/MacOS"
cp .build/release/MeetingScribe "$APP/Contents/MacOS/"
cp Info.plist "$APP/Contents/Info.plist"
codesign --force --deep --sign - "$APP"     # ad-hoc; use your Developer ID for a stable permission identity
echo "Built $APP. Move it to /Applications and open it once."
