#!/usr/bin/env bash
# Build the AuraMail macOS app on Linux.
#
# Tauri's macOS bundler shells out to macOS-only tools (hdiutil, codesign), so
# this compiles both Darwin targets with --no-bundle and assembles the .app
# itself, then ad-hoc signs it with rcodesign. The toolchain comes either from
# the image built from packaging/macos/Dockerfile or, in CI, from
# scripts/ci-macos-setup.sh:
#
#   docker build -f packaging/macos/Dockerfile -t auramail-macos packaging/macos
#   docker run --rm -v "$PWD:/src" -w /src auramail-macos bash scripts/ci-macos.sh
#
# The result is release/AuraMail-<version>-macos.dmg: an HFS+ image with
# the app and an Applications shortcut, compressed by libdmg-hfsplus. It is
# unsigned apart from the ad-hoc signature macOS requires on Apple Silicon, so
# Gatekeeper asks the user to confirm the first launch.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

VERSION="$(sed -n 's/^[[:space:]]*"version":[[:space:]]*"\([^"]*\)".*/\1/p' package.json | head -n1)"
test -n "$VERSION"

TARGETS=(aarch64-apple-darwin x86_64-apple-darwin)
APP="release/AuraMail.app"
DMG="release/AuraMail-${VERSION}-macos.dmg"
UPDATE_ZIP="release/AuraMail-${VERSION}-macos-update.zip"

require() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "error: $1 not found; run this inside the macOS build image" >&2
    echo "  docker build -f packaging/macos/Dockerfile -t auramail-macos packaging/macos" >&2
    exit 1
  fi
}

require lipo
require oa64-clang
require o64-clang
require rcodesign
require mkfs.hfsplus
require hdutil
require dmg

if [ ! -d node_modules ]; then
  echo "error: node_modules is missing; run npm ci first" >&2
  exit 1
fi

echo "==> building the Proton bridge for macOS"
for target in "${TARGETS[@]}"; do
  AURAMAIL_SIDECAR_TARGET="$target" bash src-tauri/proton-bridge/build.sh
done

echo "==> building the app for ${TARGETS[*]}"
for target in "${TARGETS[@]}"; do
  npm run tauri build -- --target "$target" --no-bundle
  test -f "src-tauri/target/$target/release/auramail"
done

echo "==> assembling AuraMail.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" \
  "$APP/Contents/Resources/matugen" \
  "$APP/Contents/Resources/logo"

lipo -create \
  "src-tauri/target/aarch64-apple-darwin/release/auramail" \
  "src-tauri/target/x86_64-apple-darwin/release/auramail" \
  -output "$APP/Contents/MacOS/auramail"
chmod 755 "$APP/Contents/MacOS/auramail"

# The Rust side looks for the bridge next to its own executable.
lipo -create \
  src-tauri/binaries/auramail-proton-bridge-aarch64-apple-darwin \
  src-tauri/binaries/auramail-proton-bridge-x86_64-apple-darwin \
  -output "$APP/Contents/MacOS/auramail-proton-bridge"
chmod 755 "$APP/Contents/MacOS/auramail-proton-bridge"

cp src-tauri/icons/icon.icns "$APP/Contents/Resources/icon.icns"
cp resources/matugen/auramail.toml resources/matugen/auramail.json \
  "$APP/Contents/Resources/matugen/"
cp resources/logo/aura_mail_icon_only.svg "$APP/Contents/Resources/logo/"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleDisplayName</key>
  <string>AuraMail</string>
  <key>CFBundleExecutable</key>
  <string>auramail</string>
  <key>CFBundleIconFile</key>
  <string>icon.icns</string>
  <key>CFBundleIdentifier</key>
  <string>dev.auramail.app</string>
  <key>CFBundleInfoDictionaryVersion</key>
  <string>6.0</string>
  <key>CFBundleName</key>
  <string>AuraMail</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>$VERSION</string>
  <key>CFBundleVersion</key>
  <string>$VERSION</string>
  <key>LSApplicationCategoryType</key>
  <string>public.app-category.productivity</string>
  <key>LSMinimumSystemVersion</key>
  <string>11.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>NSPrincipalClass</key>
  <string>NSApplication</string>
  <key>NSSupportsAutomaticGraphicsSwitching</key>
  <true/>
</dict>
</plist>
PLIST

echo "==> signing (ad-hoc)"
# Ad-hoc is rcodesign's default: it adds the signatures Apple Silicon needs
# without a Developer ID certificate.
rcodesign sign "$APP"

echo "==> packaging"
rm -f "$UPDATE_ZIP"
(cd release && zip -qry "AuraMail-${VERSION}-macos-update.zip" "AuraMail.app")

# The drag-and-drop installer is an HFS+ image holding the app plus an
# Applications symlink, compressed into a DMG. libdmg-hfsplus writes both
# without macOS tooling; hdutil fills the volume file by file because its
# bulk copy helpers do not carry symlinks or permissions over.
echo "==> building the disk image"
stage="$(mktemp -d)"
work="$(mktemp -d)"
trap 'rm -rf "$stage" "$work"' EXIT

cp -a "$APP" "$stage/AuraMail.app"
ln -s /Applications "$stage/Applications"

bytes="$(du -sb "$stage" | cut -f1)"
megabytes=$(((bytes + 48 * 1024 * 1024 + 1048575) / 1048576))
image="$work/AuraMail.hfs"
truncate -s "${megabytes}M" "$image"
mkfs.hfsplus -v AuraMail "$image" >/dev/null

(
  cd "$stage"
  while IFS= read -r directory; do
    hdutil "$image" mkdir "/${directory#./}" >/dev/null
  done < <(find . -mindepth 1 -type d | sort)
  while IFS= read -r file; do
    path="/${file#./}"
    hdutil "$image" add "$file" "$path" >/dev/null
    hdutil "$image" chmod "$(stat -c %a "$file")" "$path" >/dev/null
  done < <(find . -type f | sort)
)
hdutil "$image" symlink /Applications /Applications >/dev/null

rm -f "$DMG"
dmg build "$image" "$DMG" >/dev/null

echo "built $UPDATE_ZIP"
echo "built $DMG"
