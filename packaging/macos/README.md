# macOS build

Builds `AuraMail.app` and a drag-and-drop `.dmg` on Linux, without a Mac.
Tauri's macOS bundler cannot run here (it shells out to `hdiutil`, `codesign`,
and other macOS-only tools), so `scripts/ci-macos.sh` compiles both Darwin
targets with `--no-bundle` and assembles the bundle itself.

## Requirements

- Docker, for the local image below
- `node_modules` in the checkout (`npm ci` once, on the host)

## Build with Docker

```sh
docker build -f packaging/macos/Dockerfile -t auramail-macos packaging/macos
docker run --rm -v "$PWD:/src" -w /src auramail-macos bash scripts/ci-macos.sh
```

## Build in CI (no Docker)

The CI runner has no Docker daemon, so the workflow calls
`scripts/ci-macos-setup.sh` first. It installs the same pieces on the runner:

- Clang, CMake, `hfsprogs` from apt, Go and Node from the workflow actions
- Rust with both Darwin targets from rustup
- `rcodesign` and `libdmg-hfsplus` from source
- the osxcross toolchain and macOS SDK, pulled from the public
  `crazymax/osxcross:15.5-ubuntu` image with `crane` and unpacked into
  `$RUNNER_TEMP` (the toolchain locates its SDK relative to its own `bin/`)

## Output

- `release/AuraMail-<version>-universal.dmg` — the installer: the app plus an
  `Applications` shortcut, as a compressed HFS+ image
- `release/AuraMail-<version>-universal.zip` — the same app as a plain archive

`mkfs.hfsplus` formats the DMG volume, `hdutil` adds the files one by one (its
bulk copy helpers drop symlinks and permissions), and `dmg build` compresses it.
Finder shows the two icons side by side; there is no custom background or icon
layout yet (that needs a `.DS_Store`, which we do not generate).

## Signing

The app is ad-hoc signed, which is enough for the kernel on Apple Silicon but
not for Gatekeeper. On first launch macOS says the developer cannot be verified;
the user opens it with **right-click → Open**, or clears the quarantine flag:

```sh
xattr -dr com.apple.quarantine /Applications/AuraMail.app
```

The DMG itself carries no signature; only the app inside it does. A Developer ID
certificate removes the warning, and it does not require a Mac:
`rcodesign sign --p12-file ... AuraMail.app` signs, and `rcodesign notarize`
submits the result to Apple's notary service. Both would run in this same build.

## Notes

- The Rust side must have the `apple-native` keyring feature (macOS uses the
  Keychain); without it every credential call falls back to a mock store.
- The Proton bridge is pure Go and built for `darwin/amd64` and `darwin/arm64`,
  then `lipo`'d with the app binary.
- The tray icon is currently drawn as-is in the menu bar; a proper macOS
  template image is still to do.
