#!/usr/bin/env bash
# Install the Linux toolchain needed to cross-build AuraMail for macOS.
#
# This is the Docker-less path used by CI, where no Docker daemon is
# available. It pulls the public osxcross image with crane and unpacks it;
# the toolchain finds its SDK relative to its own bin/ directory, so the
# image can live anywhere. Everything else matches packaging/macos/Dockerfile.
#
# Run on Ubuntu 24.04 with passwordless sudo. Exports the compiler
# environment for later workflow steps through GITHUB_ENV / GITHUB_PATH.
set -euo pipefail

ROOT="${RUNNER_TEMP:-/tmp}/auramail-osxcross"
OSXCROSS_IMAGE="${OSXCROSS_IMAGE:-crazymax/osxcross:15.5-ubuntu}"
CRANE_VERSION="${CRANE_VERSION:-0.20.2}"
RUST_VERSION="${RUST_VERSION:-1.98.1}"
RCODESIGN_VERSION="${RCODESIGN_VERSION:-0.29.0}"
LIBDMG_COMMIT="${LIBDMG_COMMIT:-7ac55ec64c96f7800d9818ce64c79670e7f02b67}"

export DEBIAN_FRONTEND=noninteractive
sudo apt-get update
sudo apt-get install -y --no-install-recommends \
  build-essential \
  clang \
  cmake \
  curl \
  file \
  git \
  hfsprogs \
  lld \
  unzip \
  xz-utils \
  zip \
  zlib1g-dev

if ! command -v crane >/dev/null 2>&1; then
  curl -fsSL \
    "https://github.com/google/go-containerregistry/releases/download/v${CRANE_VERSION}/go-containerregistry_Linux_x86_64.tar.gz" \
    | sudo tar -xz -C /usr/local/bin crane
fi

if [ ! -x "$ROOT/osxcross/bin/o64-clang" ]; then
  mkdir -p "$ROOT"
  crane export "$OSXCROSS_IMAGE" - | tar -x -C "$ROOT"
fi

if ! command -v rustup >/dev/null 2>&1; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
    | sh -s -- -y --profile minimal --default-toolchain "$RUST_VERSION"
fi
export PATH="$HOME/.cargo/bin:$PATH"
rustup target add aarch64-apple-darwin x86_64-apple-darwin

if ! command -v rcodesign >/dev/null 2>&1; then
  curl -fsSL -o /tmp/rcodesign.tar.gz \
    "https://github.com/indygreg/apple-platform-rs/releases/download/apple-codesign/${RCODESIGN_VERSION}/apple-codesign-${RCODESIGN_VERSION}-x86_64-unknown-linux-musl.tar.gz"
  sudo tar -xzf /tmp/rcodesign.tar.gz -C /usr/local/bin --strip-components=1 \
    "apple-codesign-${RCODESIGN_VERSION}-x86_64-unknown-linux-musl/rcodesign"
  sudo chmod 755 /usr/local/bin/rcodesign
fi

if ! command -v hdutil >/dev/null 2>&1 || ! command -v dmg >/dev/null 2>&1; then
  rm -rf /tmp/libdmg
  git clone https://github.com/planetbeing/libdmg-hfsplus /tmp/libdmg
  git -C /tmp/libdmg checkout -q "$LIBDMG_COMMIT"
  # Built without OpenSSL: the FileVault code does not compile against
  # OpenSSL 3 and encrypted images are not needed here.
  cmake -S /tmp/libdmg -B /tmp/libdmg/build \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_DISABLE_FIND_PACKAGE_OpenSSL=ON
  make -C /tmp/libdmg/build -j"$(nproc)"
  sudo install -m 755 /tmp/libdmg/build/dmg/dmg /usr/local/bin/dmg
  sudo install -m 755 /tmp/libdmg/build/hdutil/hdutil /usr/local/bin/hdutil
fi

# The compiler wrappers need their bundled libxar/libc++ at run time, and the
# cc crate needs the cross linker for each Darwin target.
if [ -n "${GITHUB_PATH:-}" ]; then
  echo "$ROOT/osxcross/bin" >> "$GITHUB_PATH"
  echo "$HOME/.cargo/bin" >> "$GITHUB_PATH"
fi
if [ -n "${GITHUB_ENV:-}" ]; then
  {
    echo "LD_LIBRARY_PATH=$ROOT/osxcross/lib"
    echo "MACOSX_DEPLOYMENT_TARGET=11.0"
    echo "CC_aarch64_apple_darwin=oa64-clang"
    echo "CXX_aarch64_apple_darwin=oa64-clang++"
    echo "CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER=oa64-clang"
    echo "CC_x86_64_apple_darwin=o64-clang"
    echo "CXX_x86_64_apple_darwin=o64-clang++"
    echo "CARGO_TARGET_X86_64_APPLE_DARWIN_LINKER=o64-clang"
  } >> "$GITHUB_ENV"
fi
