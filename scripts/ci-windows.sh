#!/usr/bin/env bash
# Build the Windows installer from Ubuntu using the MSVC cross toolchain.
#
# This is intentionally a cross-build: cargo-xwin supplies the Microsoft CRT
# and Windows SDK, clang-cl/lld-link produce the PE, and makensis packages the
# result. No Windows machine or Wine is involved.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

VERSION="$(node -p "require('./package.json').version")"
test -n "$VERSION"

TARGET="x86_64-pc-windows-msvc"
SIDECAR="src-tauri/binaries/auramail-proton-bridge-${TARGET}.exe"
APP="src-tauri/target/${TARGET}/release/auramail.exe"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

for tool in cargo-xwin clang-cl lld-link llvm-rc llvm-lib makensis; do
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "error: missing CI tool: $tool" >&2
    exit 1
  fi
done

test -f package-lock.json
npm ci --no-audit --no-fund
npm run build

AURAMAIL_SIDECAR_TARGET="$TARGET" \
  bash src-tauri/proton-bridge/build.sh

if [ ! -f "$SIDECAR" ]; then
  echo "error: Windows Proton sidecar was not built: $SIDECAR" >&2
  exit 1
fi

cargo xwin build \
  --release \
  --target "$TARGET" \
  --features tauri/custom-protocol \
  --manifest-path src-tauri/Cargo.toml

if [ ! -f "$APP" ]; then
  echo "error: Windows app was not built: $APP" >&2
  exit 1
fi

curl -L --fail --retry 3 \
  -o "$WORK/MicrosoftEdgeWebview2Setup.exe" \
  'https://go.microsoft.com/fwlink/p/?LinkId=2124703'

mkdir -p release
makensis \
  -DAPP_VERSION="$VERSION" \
  -DWEBVIEW2_BOOTSTRAPPER="$WORK/MicrosoftEdgeWebview2Setup.exe" \
  scripts/windows-installer.nsi

INSTALLER="release/AuraMail-${VERSION}-x64-setup.exe"
if [ ! -f "$INSTALLER" ]; then
  echo "error: installer was not produced: $INSTALLER" >&2
  exit 1
fi

ls -lh "$INSTALLER"
