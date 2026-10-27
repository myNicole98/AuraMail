#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
read -r -a DOCKER_CMD <<< "${DOCKER:-docker}"
VERSION="$(sed -n 's/^[[:space:]]*"version":[[:space:]]*"\([^"]*\)".*/\1/p' "$ROOT/package.json" | head -n1)"

if [ -z "$VERSION" ]; then
  echo "Could not read version from package.json" >&2
  exit 1
fi

if ! command -v "${DOCKER_CMD[0]}" >/dev/null 2>&1; then
  echo "Docker is required to build the portable AppImage." >&2
  exit 1
fi

if ! "${DOCKER_CMD[@]}" info >/dev/null 2>&1 && [ -S /run/docker.sock ]; then
  export DOCKER_HOST=unix:///run/docker.sock
fi

if ! "${DOCKER_CMD[@]}" info >/dev/null 2>&1; then
  echo "Docker is not accessible." >&2
  echo "Try: sudo chmod 666 /run/docker.sock" >&2
  echo "Or: DOCKER='sudo docker' $0" >&2
  exit 1
fi

WORK="$(mktemp -d "$ROOT/.appimage-build.XXXXXX")"
BUILD_CONTAINER=""
PKG_CONTAINER=""
cleanup() {
  if [ -n "$BUILD_CONTAINER" ]; then
    "${DOCKER_CMD[@]}" rm -f "$BUILD_CONTAINER" >/dev/null 2>&1 || true
  fi
  if [ -n "$PKG_CONTAINER" ]; then
    "${DOCKER_CMD[@]}" rm -f "$PKG_CONTAINER" >/dev/null 2>&1 || true
  fi
  if [ "${KEEP_WORK:-0}" = "1" ]; then
    echo "Keeping build workspace: $WORK"
  else
    rm -rf "$WORK"
  fi
}
trap cleanup EXIT

mkdir -p "$WORK/src" "$WORK/out"
tar -C "$ROOT" \
  --exclude='node_modules' \
  --exclude='src-tauri/target' \
  --exclude='dist' \
  --exclude='release' \
  --exclude='.git' \
  --exclude='.appimage-build.*' \
  -cf - . | tar -C "$WORK/src" -xf -

BUILD_SCRIPT="$(cat <<'EOF'
set -euxo pipefail
cd /app
export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y --no-install-recommends \
  build-essential curl wget file ca-certificates pkg-config patchelf \
  libssl-dev libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev \
  librsvg2-dev libxdo-dev libfuse2t64 nodejs npm libdbus-1-dev libsecret-1-dev zlib1g-dev
curl -L --fail --retry 3 -o /tmp/go.tar.gz "https://go.dev/dl/go1.26.0.linux-amd64.tar.gz"
rm -rf /usr/local/go
tar -C /usr/local -xzf /tmp/go.tar.gz
export PATH="/usr/local/go/bin:$PATH"
go version
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
. "$HOME/.cargo/env"
test -f package-lock.json
rm -rf node_modules src-tauri/target dist
npm ci --no-audit --no-fund
check_sidecar() {
  sidecar=src-tauri/binaries/auramail-proton-bridge-x86_64-unknown-linux-gnu
  if [ ! -f "$sidecar" ] || [ ! -x "$sidecar" ]; then
    echo "error: sidecar missing or not executable: $sidecar" >&2
    echo "run: bash src-tauri/proton-bridge/build.sh" >&2
    exit 1
  fi
  echo "sidecar ok: $sidecar"
}
bash src-tauri/proton-bridge/build.sh
check_sidecar
set +e
npm run tauri build -- --bundles appimage
status=$?
set -e
echo "tauri build exited with $status"
appdir="$(find src-tauri/target/release/bundle/appimage -maxdepth 1 -type d -name '*.AppDir' | head -n1)"
test -n "$appdir"
cp -a "$appdir" /app/AppDir
chown -R "${HOST_UID}:${HOST_GID}" /app
EOF
)"

BUILD_CONTAINER="auramail-build-$$-$RANDOM"
"${DOCKER_CMD[@]}" create --name "$BUILD_CONTAINER" --platform linux/amd64 \
  -e HOST_UID="$(id -u)" \
  -e HOST_GID="$(id -g)" \
  ubuntu:24.04 \
  bash -euxo pipefail -c "$BUILD_SCRIPT"
"${DOCKER_CMD[@]}" cp "$WORK/src/." "$BUILD_CONTAINER:/app"
"${DOCKER_CMD[@]}" start -a "$BUILD_CONTAINER"
"${DOCKER_CMD[@]}" cp "$BUILD_CONTAINER:/app/AppDir" - | tar -C "$WORK" -xf -
"${DOCKER_CMD[@]}" rm -f "$BUILD_CONTAINER"
BUILD_CONTAINER=""

PACKAGE_SCRIPT="$(cat <<'EOF'
set -euxo pipefail
cd /
mkdir -p /out
export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y --no-install-recommends \
  ca-certificates curl file libfuse2t64 squashfs-tools python3 desktop-file-utils \
  libgtk-3-0t64 libwebkit2gtk-4.1-0 libayatana-appindicator3-1 \
  librsvg2-2 libxdo3 libssl3t64 libdbus-1-3 libsecret-1-0
curl -L --fail --retry 3 -o /tmp/linuxdeploy.AppImage \
  https://github.com/tauri-apps/binary-releases/releases/download/linuxdeploy-07333c6/linuxdeploy-x86_64.AppImage
chmod +x /tmp/linuxdeploy.AppImage
if ! /tmp/linuxdeploy.AppImage --appimage-extract-and-run --appdir /AppDir; then
  offset="$(grep -abo hsqs /tmp/linuxdeploy.AppImage | tail -1 | cut -d: -f1)"
  rm -rf /tmp/linuxdeploy-root
  unsquashfs -o "$offset" -d /tmp/linuxdeploy-root /tmp/linuxdeploy.AppImage
  /tmp/linuxdeploy-root/AppRun --appdir /AppDir
fi

webkit_proc="/AppDir/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1"
test -d "$webkit_proc"
ln -sf injected-bundle/libwebkit2gtkinjectedbundle.so \
  "$webkit_proc/libwebkit2gtkinjectedbundle.so"

lib="$(find /AppDir/usr/lib -name 'libwebkit2gtk-4.1.so.0' -print -quit)"
test -n "$lib"
python3 - "$lib" <<'PY'
import re
import sys
from pathlib import Path

lib = Path(sys.argv[1])
data = lib.read_bytes()
new = b"/tmp/auramail-webkit-576f0a7f"
match = re.search(rb"/usr/[A-Za-z0-9_./-]*webkit2gtk-4\.1", data)
if match is None:
    raise SystemExit(f"WebKit process path not found in {lib}")
old = match.group(0)
if len(new) > len(old):
    raise SystemExit(f"Replacement path is longer than {old!r}")
lib.write_bytes(data.replace(old, new + b"\0" * (len(old) - len(new))))
print(f"patched {old!r} -> {new!r}")
PY

cat > /AppDir/AppRun <<'RUN'
#!/usr/bin/env bash
set -e
this_dir="$(readlink -f "$(dirname "$0")")"
export APPDIR="$this_dir"
export PATH="$this_dir/usr/bin:$PATH"
export LD_LIBRARY_PATH="$this_dir/usr/lib:$this_dir/usr/lib/x86_64-linux-gnu:${LD_LIBRARY_PATH:-}"
export XDG_DATA_DIRS="$this_dir/usr/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"
export GSETTINGS_SCHEMA_DIR="$this_dir/usr/share/glib-2.0/schemas:${GSETTINGS_SCHEMA_DIR:-}"
if [ -d "$this_dir/usr/lib/x86_64-linux-gnu/gio/modules" ]; then
  export GIO_EXTRA_MODULES="$this_dir/usr/lib/x86_64-linux-gnu/gio/modules"
fi
webkit_dir="$this_dir/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1"
webkit_link="/tmp/auramail-webkit-576f0a7f"
rm -f "$webkit_link" 2>/dev/null || true
ln -s "$webkit_dir" "$webkit_link"
exec "$this_dir/usr/bin/auramail" "$@"
RUN
chmod +x /AppDir/AppRun

curl -L --fail --retry 3 -o /tmp/runtime \
  https://github.com/AppImage/type2-runtime/releases/download/continuous/runtime-x86_64
curl -L --fail --retry 3 -o /tmp/appimagetool.AppImage \
  https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
chmod +x /tmp/appimagetool.AppImage
offset="$(grep -abo hsqs /tmp/appimagetool.AppImage | tail -1 | cut -d: -f1)"
rm -rf /tmp/appimagetool-root
unsquashfs -o "$offset" -d /tmp/appimagetool-root /tmp/appimagetool.AppImage

ARCH=x86_64 /tmp/appimagetool-root/usr/bin/appimagetool \
  --runtime-file /tmp/runtime \
  /AppDir "/out/AuraMail-${VERSION}-x86_64.AppImage"
chmod 755 "/out/AuraMail-${VERSION}-x86_64.AppImage"

appimage="/out/AuraMail-${VERSION}-x86_64.AppImage"
offset="$(grep -abo hsqs "$appimage" | tail -1 | cut -d: -f1)"
listing="$(unsquashfs -o "$offset" -l "$appimage")"
if ! grep -qE '(^|/)usr/bin/auramail-proton-bridge$' <<<"$listing"; then
  echo "error: $appimage does not contain usr/bin/auramail-proton-bridge" >&2
  exit 1
fi
echo "verified usr/bin/auramail-proton-bridge in $appimage"
chown -R "${HOST_UID}:${HOST_GID}" /AppDir /out
EOF
)"

PKG_CONTAINER="auramail-package-$$-$RANDOM"
"${DOCKER_CMD[@]}" create --name "$PKG_CONTAINER" --platform linux/amd64 \
  -e VERSION="$VERSION" \
  -e HOST_UID="$(id -u)" \
  -e HOST_GID="$(id -g)" \
  ubuntu:24.04 \
  bash -euxo pipefail -c "$PACKAGE_SCRIPT"
"${DOCKER_CMD[@]}" cp "$WORK/AppDir/." "$PKG_CONTAINER:/AppDir"
"${DOCKER_CMD[@]}" start -a "$PKG_CONTAINER"
"${DOCKER_CMD[@]}" cp "$PKG_CONTAINER:/out/AuraMail-${VERSION}-x86_64.AppImage" - | tar -C "$WORK/out" -xf -
"${DOCKER_CMD[@]}" rm -f "$PKG_CONTAINER"
PKG_CONTAINER=""

mkdir -p "$ROOT/release"
install -m 755 "$WORK/out/AuraMail-${VERSION}-x86_64.AppImage" \
  "$ROOT/release/AuraMail-${VERSION}-x86_64.AppImage"

echo
ls -lh "$ROOT/release/AuraMail-${VERSION}-x86_64.AppImage"
sha256sum "$ROOT/release/AuraMail-${VERSION}-x86_64.AppImage"
