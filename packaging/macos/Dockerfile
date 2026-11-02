# syntax=docker/dockerfile:1
#
# Cross-compile AuraMail for macOS from Linux. Nothing here needs a Mac:
# the first stage is the public osxcross image, which already carries the
# cross toolchain and a macOS SDK; the second stage wraps it with Node,
# Rust (both Darwin targets) and rcodesign for ad-hoc code signing.
#
# Build:  docker build -f packaging/macos/Dockerfile -t auramail-macos packaging/macos
# Run:    docker run --rm -v "$PWD:/src" -w /src auramail-macos bash scripts/ci-macos.sh

ARG OSXCROSS_IMAGE=crazymax/osxcross:15.5-ubuntu

FROM --platform=$BUILDPLATFORM ${OSXCROSS_IMAGE} AS toolchain

FROM ubuntu:24.04

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
  && apt-get install -y --no-install-recommends \
    build-essential \
    ca-certificates \
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
    zlib1g-dev \
  && rm -rf /var/lib/apt/lists/*

COPY --from=toolchain /osxcross /osxcross
ENV PATH="/osxcross/bin:${PATH}"
ENV LD_LIBRARY_PATH="/osxcross/lib"
# Tauri 2 needs WKWebView, and Apple Silicon needs macOS 11 as the floor.
ENV MACOSX_DEPLOYMENT_TARGET="11.0"

# Cargo and the cc crate must pick the cross linker for each Darwin target.
ENV CC_aarch64_apple_darwin=oa64-clang \
    CXX_aarch64_apple_darwin=oa64-clang++ \
    CARGO_TARGET_AARCH64_APPLE_DARWIN_LINKER=oa64-clang \
    CC_x86_64_apple_darwin=o64-clang \
    CXX_x86_64_apple_darwin=o64-clang++ \
    CARGO_TARGET_X86_64_APPLE_DARWIN_LINKER=o64-clang

ARG NODE_VERSION=22.23.4
RUN curl -fsSL "https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-x64.tar.xz" \
      | tar -xJ -C /usr/local --strip-components=1

# The Proton bridge is Go and cross-compiles to Darwin with CGO disabled.
ARG GO_VERSION=1.26.0
RUN curl -fsSL "https://go.dev/dl/go${GO_VERSION}.linux-amd64.tar.gz" \
      | tar -xz -C /usr/local
ENV PATH="/usr/local/go/bin:${PATH}"

ARG RUST_VERSION=1.98.1
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --profile minimal --default-toolchain "${RUST_VERSION}" \
  && /root/.cargo/bin/rustup target add aarch64-apple-darwin x86_64-apple-darwin
ENV PATH="/root/.cargo/bin:${PATH}"

# rcodesign signs Mach-O bundles from any host. Ad-hoc is its default.
ARG RCODESIGN_VERSION=0.29.0
RUN curl -fsSL -o /tmp/rcodesign.tar.gz \
      "https://github.com/indygreg/apple-platform-rs/releases/download/apple-codesign/${RCODESIGN_VERSION}/apple-codesign-${RCODESIGN_VERSION}-x86_64-unknown-linux-musl.tar.gz" \
  && tar -xzf /tmp/rcodesign.tar.gz -C /usr/local/bin --strip-components=1 \
      "apple-codesign-${RCODESIGN_VERSION}-x86_64-unknown-linux-musl/rcodesign" \
  && chmod 755 /usr/local/bin/rcodesign \
  && rm /tmp/rcodesign.tar.gz

# libdmg-hfsplus builds the compressed DMG from an HFS+ image. It is built
# without OpenSSL: the FileVault code does not compile against OpenSSL 3, and
# encrypted images are not needed here.
ARG LIBDMG_COMMIT=7ac55ec64c96f7800d9818ce64c79670e7f02b67
RUN git clone "https://github.com/planetbeing/libdmg-hfsplus" /tmp/libdmg \
  && git -C /tmp/libdmg checkout -q "${LIBDMG_COMMIT}" \
  && cmake -S /tmp/libdmg -B /tmp/libdmg/build \
      -DCMAKE_BUILD_TYPE=Release \
      -DCMAKE_DISABLE_FIND_PACKAGE_OpenSSL=ON \
  && make -C /tmp/libdmg/build -j"$(nproc)" \
  && install -m 755 /tmp/libdmg/build/dmg/dmg /usr/local/bin/dmg \
  && install -m 755 /tmp/libdmg/build/hdutil/hdutil /usr/local/bin/hdutil \
  && rm -rf /tmp/libdmg

WORKDIR /src
