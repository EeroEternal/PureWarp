#!/usr/bin/env bash
# sync_version.sh — Sync app/Info.plist version from app/Cargo.toml
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

CARGO_TOML="$PROJECT_ROOT/app/Cargo.toml"
PLIST="$PROJECT_ROOT/app/Info.plist"

if [[ ! -f "$CARGO_TOML" ]]; then
  echo "Error: $CARGO_TOML not found" >&2
  exit 1
fi

if [[ ! -f "$PLIST" ]]; then
  echo "Error: $PLIST not found" >&2
  exit 1
fi

# Extract version from Cargo.toml (first occurrence of 'version = "..."')
VERSION=$(grep -m1 '^version' "$CARGO_TOML" | sed 's/version = "\(.*\)"/\1/')

if [[ -z "$VERSION" ]]; then
  echo "Error: Could not extract version from $CARGO_TOML" >&2
  exit 1
fi

# Compute build number from semver: major*100 + minor*10 + patch
IFS='.' read -r MAJOR MINOR PATCH <<< "$VERSION"
BUILD_NUMBER=$(( MAJOR * 100 + MINOR * 10 + PATCH ))

echo "Syncing Info.plist to version $VERSION (build $BUILD_NUMBER)"

# Patch CFBundleShortVersionString
sed -i '' '/<key>CFBundleShortVersionString<\/key>/{n;s|<string>[^<]*</string>|<string>'"$VERSION"'</string>|;}' "$PLIST"

# Patch CFBundleVersion
sed -i '' '/<key>CFBundleVersion<\/key>/{n;s|<string>[^<]*</string>|<string>'"$BUILD_NUMBER"'</string>|;}' "$PLIST"

echo "Done. Info.plist updated to version $VERSION, build $BUILD_NUMBER"
