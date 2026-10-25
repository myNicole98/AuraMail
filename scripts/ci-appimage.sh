#!/usr/bin/env bash
set -euxo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
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

curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
. "$HOME/.cargo/env"

test -f package-lock.json
rm -rf node_modules src-tauri/target dist
npm ci --no-audit --no-fund

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

lib="$(find /tmp/AppDir/usr/lib -name 'libwebkit2gtk-4.1.so.0' -print -quit)"
test -n "$lib"
python3 - "$lib" <<'PY'
import re, sys
from pathlib import Path

lib = Path(sys.argv[1])
data = lib.read_bytes()
new = b"/tmp/auramail-webkit-576f0a7f"

print("strings mentioning webkit2gtk-4.1:")
for s in sorted(set(re.findall(rb"[ -~]*webkit2gtk-4\.1[ -~]*", data)))[:20]:
    print("  ", s)

match = re.search(rb"/[A-Za-z0-9_./-]*/webkit2gtk-4\.1(?![A-Za-z0-9_.-])", data)
if match is None:
    raise SystemExit(f"WebKit process path not found in {lib}")
old = match.group(0)
if len(new) > len(old):
    raise SystemExit(f"Replacement path is longer than {old!r}")
lib.write_bytes(data.replace(old, new + b"\0" * (len(old) - len(new))))
print(f"patched {old!r} -> {new!r}")
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

ls -lh "$OUT"
sha256sum "$OUT"