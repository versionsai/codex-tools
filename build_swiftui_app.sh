#!/bin/zsh
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")" && pwd)"
APP_NAME="Codex Sync"
PRODUCT_NAME="CodexSyncNative"
BUILD_DIR="$ROOT_DIR/.build/release"
DIST_DIR="$ROOT_DIR/dist-swiftui"
APP_DIR="$DIST_DIR/$APP_NAME.app"
ICON_PATH="$ROOT_DIR/build/Codex Sync.icns"

cd "$ROOT_DIR"
swift build -c release

rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources"

cp "$BUILD_DIR/$PRODUCT_NAME" "$APP_DIR/Contents/MacOS/$APP_NAME"

cat > "$APP_DIR/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>zh_CN</string>
    <key>CFBundleDisplayName</key>
    <string>Codex Sync</string>
    <key>CFBundleExecutable</key>
    <string>Codex Sync</string>
    <key>CFBundleIconFile</key>
    <string>Codex Sync.icns</string>
    <key>CFBundleIdentifier</key>
    <string>cn.sai.codex-sync.native</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>Codex Sync</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>1.0.0</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>LSMinimumSystemVersion</key>
    <string>14.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
PLIST

if [[ -f "$ICON_PATH" ]]; then
  cp "$ICON_PATH" "$APP_DIR/Contents/Resources/Codex Sync.icns"
fi

codesign --force --deep --sign - "$APP_DIR"

echo "Built app:"
echo "$APP_DIR"
