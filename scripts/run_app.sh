#!/bin/bash
set -euo pipefail

cd "$(dirname "$0")/.."

APP_NAME="PureWarp"
BUNDLE_DIR="target/$APP_NAME.app"

echo "🔨 Building binary..."
cargo build -p pure_warp

echo "📦 Creating app bundle..."
rm -rf "$BUNDLE_DIR"
mkdir -p "$BUNDLE_DIR/Contents/MacOS"
mkdir -p "$BUNDLE_DIR/Contents/Resources"

cp "target/debug/purewarp" "$BUNDLE_DIR/Contents/MacOS/purewarp"
cp "app/Info.plist"            "$BUNDLE_DIR/Contents/Info.plist"
cp "app/assets/purewarp.icns"  "$BUNDLE_DIR/Contents/Resources/purewarp.icns"

echo "🚀 Launching $APP_NAME..."
open "$BUNDLE_DIR"
