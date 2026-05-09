#!/bin/bash
set -euo pipefail

# ── Configuration ──
APP_NAME="PureWarp"
BINARY="$1"                          # path to compiled purewarp binary
PLIST="app/Info.plist"               # Info.plist for the .app bundle
OUTPUT_DIR="${2:-dist}"              # where to place the final .dmg
VERSION="${3:-0.1.0}"               # version string

# ── Create .app bundle ──
APP_BUNDLE="$OUTPUT_DIR/$APP_NAME.app"
mkdir -p "$APP_BUNDLE/Contents/MacOS"
mkdir -p "$APP_BUNDLE/Contents/Resources"

cp "$BINARY" "$APP_BUNDLE/Contents/MacOS/purewarp"
cp "$PLIST"   "$APP_BUNDLE/Contents/Info.plist"
cp "app/assets/purewarp.icns" "$APP_BUNDLE/Contents/Resources/purewarp.icns"

# ── Create DMG ──
DMG_NAME="${APP_NAME}-${VERSION}-macOS.dmg"
DMG_PATH="$OUTPUT_DIR/$DMG_NAME"

echo "Creating DMG at $DMG_PATH ..."
hdiutil create \
    -volname "$APP_NAME" \
    -srcfolder "$APP_BUNDLE" \
    -ov -format UDZO \
    "$DMG_PATH"

echo "Done: $DMG_PATH"
