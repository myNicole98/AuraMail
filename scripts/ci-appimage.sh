#!/usr/bin/env bash
set -euxo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
SIDECAR="src-tauri/binaries/auramail-proton-bridge-x86_64-unknown-linux-gnu"
GO_VERSION="1.26.0"

check_sidecar() {
  if [ ! -f "$SIDECAR" ] || [ ! -x "$SIDECAR" ]; then
    echo "error: sidecar missing or not executable: $SIDECAR" >&2
    echo "run: bash src-tauri/proton-bridge/build.sh" >&2
    exit 1
  fi
  echo "sidecar ok: $SIDECAR"
}

if [ "${1:-}" = "--check-sidecar" ]; then
  check_sidecar
  exit 0
fi

VERSION="$(sed -n 's/^[[:space:]]*"version":[[:space:]]*"\([^"]*\)".*/\1/p' package.json | head -n1)"
test -n "$VERSION"

export DEBIAN_FRONTEND=noninteractive
SUDO=""; [ "$(id -u)" -ne 0 ] && SUDO="sudo"

# ---------- build ----------
$SUDO apt-get update
$SUDO apt-get install -y --no-install-recommends \
  build-essential curl wget file ca-certificates pkg-config patchelf \
  libssl-dev libgtk-3-dev libwebkit2gtk-4.1-dev libayatana-appindicator3-dev \
  librsvg2-dev libxdo-dev libfuse2t64 libdbus-1-dev libsecret-1-dev zlib1g-dev \
  squashfs-tools python3 desktop-file-utils
command -v node >/dev/null || $SUDO apt-get install -y --no-install-recommends nodejs npm

curl -L --fail --retry 3 -o /tmp/go.tar.gz "https://go.dev/dl/go${GO_VERSION}.linux-amd64.tar.gz"
$SUDO rm -rf /usr/local/go
$SUDO tar -C /usr/local -xzf /tmp/go.tar.gz
export PATH="/usr/local/go/bin:$PATH"
go version

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
. "$HOME/.cargo/env"

test -f package-lock.json
rm -rf node_modules src-tauri/target dist
npm ci --no-audit --no-fund

bash src-tauri/proton-bridge/build.sh
check_sidecar

set +e
npm run tauri build -- --bundles appimage
echo "tauri build exited with $?"
set -e

appdir="$(find src-tauri/target/release/bundle/appimage -maxdepth 1 -type d -name '*.AppDir' | head -n1)"
test -n "$appdir"
rm -rf /tmp/AppDir
cp -a "$appdir" /tmp/AppDir

# ---------- package ----------
curl -L --fail --retry 3 -o /tmp/linuxdeploy.AppImage \
  https://github.com/tauri-apps/binary-releases/releases/download/linuxdeploy-07333c6/linuxdeploy-x86_64.AppImage
chmod +x /tmp/linuxdeploy.AppImage
if ! /tmp/linuxdeploy.AppImage --appimage-extract-and-run --appdir /tmp/AppDir; then
  offset="$(grep -abo hsqs /tmp/linuxdeploy.AppImage | tail -1 | cut -d: -f1)"
  rm -rf /tmp/linuxdeploy-root
  unsquashfs -o "$offset" -d /tmp/linuxdeploy-root /tmp/linuxdeploy.AppImage
  /tmp/linuxdeploy-root/AppRun --appdir /tmp/AppDir
fi

webkit_proc="/tmp/AppDir/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1"
test -d "$webkit_proc"
ln -sf injected-bundle/libwebkit2gtkinjectedbundle.so \
  "$webkit_proc/libwebkit2gtkinjectedbundle.so"

lib="$(find /tmp/AppDir/usr/lib -name 'libwebkit2gtk-4.1.so.0' -print -quit)"
test -n "$lib"
python3 - "$lib" <<'PY'
import re, sys
from pathlib import Path

lib = Path(sys.argv[1])
data = lib.read_bytes()
new = b"/tmp/auramail-webkit-576f0a7f"
pattern = re.compile(rb"(?:[A-Za-z0-9_.-]*/)+webkit2gtk-4\.1(?![A-Za-z0-9_.-])")

def repl(m):
    old = m.group(0)
    if len(new) > len(old):
        raise SystemExit(f"Replacement path is longer than {old!r}")
    print(f"patching {old!r}")
    return new + b"\0" * (len(old) - len(new))

patched, n = pattern.subn(repl, data)
if n == 0:
    raise SystemExit(f"WebKit process path not found in {lib}")
lib.write_bytes(patched)
print(f"patched {n} occurrence(s)")
PY

cat > /tmp/AppDir/AppRun <<'RUN'
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
else
  unset GIO_EXTRA_MODULES
fi
webkit_dir="$this_dir/usr/lib/x86_64-linux-gnu/webkit2gtk-4.1"
webkit_link="/tmp/auramail-webkit-576f0a7f"
rm -f "$webkit_link" 2>/dev/null || true
ln -s "$webkit_dir" "$webkit_link"
exec "$this_dir/usr/bin/auramail" "$@"
RUN
chmod +x /tmp/AppDir/AppRun

curl -L --fail --retry 3 -o /tmp/runtime \
  https://github.com/AppImage/type2-runtime/releases/download/continuous/runtime-x86_64
curl -L --fail --retry 3 -o /tmp/appimagetool.AppImage \
  https://github.com/AppImage/appimagetool/releases/download/continuous/appimagetool-x86_64.AppImage
chmod +x /tmp/appimagetool.AppImage
offset="$(grep -abo hsqs /tmp/appimagetool.AppImage | tail -1 | cut -d: -f1)"
rm -rf /tmp/appimagetool-root
unsquashfs -o "$offset" -d /tmp/appimagetool-root /tmp/appimagetool.AppImage

mkdir -p "$ROOT/release"
OUT="$ROOT/release/AuraMail-${VERSION}-x86_64.AppImage"
ARCH=x86_64 /tmp/appimagetool-root/usr/bin/appimagetool \
  --runtime-file /tmp/runtime /tmp/AppDir "$OUT"
chmod 755 "$OUT"

offset="$(grep -abo hsqs "$OUT" | tail -1 | cut -d: -f1)"
listing="$(unsquashfs -o "$offset" -l "$OUT")"
if ! grep -qE '(^|/)usr/bin/auramail-proton-bridge$' <<<"$listing"; then
  echo "error: $OUT does not contain usr/bin/auramail-proton-bridge" >&2
  exit 1
fi
echo "verified usr/bin/auramail-proton-bridge in $OUT"

ls -lh "$OUT"
sha256sum "$OUT"
