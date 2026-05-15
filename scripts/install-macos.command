#!/bin/bash
set -euo pipefail

APP_NAME="Codex Tools.app"
SOURCE_DIR="$(cd "$(dirname "$0")" && pwd)"
SOURCE_APP="${SOURCE_DIR}/${APP_NAME}"
DEST_APP="/Applications/${APP_NAME}"

echo "Codex Tools macOS installer"
echo

if [[ ! -d "${SOURCE_APP}" ]]; then
  echo "Cannot find ${APP_NAME} next to this installer."
  echo "Please keep install.command and ${APP_NAME} in the same folder."
  echo
  read -r -p "Press Enter to close..."
  exit 1
fi

echo "Installing ${APP_NAME} to /Applications..."
rm -rf "${DEST_APP}"
ditto "${SOURCE_APP}" "${DEST_APP}"

echo "Removing macOS quarantine attribute..."
xattr -dr com.apple.quarantine "${DEST_APP}" 2>/dev/null || true

echo "Opening Codex Tools..."
open "${DEST_APP}"

echo
echo "Done. You can close this window."
