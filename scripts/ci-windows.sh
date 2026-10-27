#!/usr/bin/env bash
# Build the Windows NSIS installer in CI.
#
# Tauri's Windows bundler is used here so the installer includes WebView2
# handling, shortcuts, and the standard uninstaller. The Proton sidecar is
# built for the MSVC target before Tauri bundles it.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

VERSION="$(node -p "require('./package.json').version")"
test -n "$VERSION"

SIDECAR="src-tauri/binaries/auramail-proton-bridge-x86_64-pc-windows-msvc.exe"
INSTALLER_DIR="src-tauri/target/release/bundle/nsis"

test -f package-lock.json
npm ci --no-audit --no-fund

AURAMAIL_SIDECAR_TARGET=x86_64-pc-windows-msvc \
  bash src-tauri/proton-bridge/build.sh

if [ ! -f "$SIDECAR" ]; then
  echo "error: Windows Proton sidecar was not built: $SIDECAR" >&2
  exit 1
fi

npm run tauri build -- --bundles nsis

INSTALLER="$(find "$INSTALLER_DIR" -maxdepth 1 -type f -name '*.exe' -print -quit)"
if [ -z "$INSTALLER" ]; then
  echo "error: Tauri did not produce an NSIS installer in $INSTALLER_DIR" >&2
  exit 1
fi

mkdir -p release
cp "$INSTALLER" "release/AuraMail-${VERSION}-x64-setup.exe"
ls -lh "release/AuraMail-${VERSION}-x64-setup.exe"
