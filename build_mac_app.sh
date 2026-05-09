#!/bin/zsh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
PYTHON_BIN="${PYTHON_BIN:-python3}"
ICONSET_DIR="$ROOT/build/Codex Sync.iconset"
ICON_FILE="$ROOT/build/Codex Sync.icns"

cd "$ROOT"

rm -rf "$ROOT/build" "$ROOT/dist"
mkdir -p "$ROOT/build"

"$PYTHON_BIN" "$ROOT/generate_app_icon.py"
iconutil -c icns "$ICONSET_DIR" -o "$ICON_FILE"

"$PYTHON_BIN" -m PyInstaller \
  --noconfirm \
  --clean \
  --windowed \
  --name "Codex Sync" \
  --osx-bundle-identifier "cn.sai.codex-sync" \
  --icon "$ICON_FILE" \
  "$ROOT/codex_sync_gui.py"

echo
echo "Built app:"
echo "$ROOT/dist/Codex Sync.app"
