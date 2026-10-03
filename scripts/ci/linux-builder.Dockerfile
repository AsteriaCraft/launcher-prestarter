# The Linux build environment for the prestarter (ADR 0009): Ubuntu 22.04 pinned by digest, so the glibc floor of
# both Linux artifacts is 2.34 no matter which runner image builds them, and the same image builds locally.
# Multi-arch: the same digest resolves to amd64 on ubuntu-24.04 and to arm64 on ubuntu-24.04-arm.
#
#   docker build -t prestarter-linux-builder -f scripts/ci/linux-builder.Dockerfile scripts/ci
#   scripts/ci/build-linux.sh docker      (runs the build in it, the AppImage step with --network none)
FROM ubuntu:22.04@sha256:b1066385161d28ddf6bc7e7b28a9170eec11484c821d1a5150d176cbde41d7f7

ARG NODE_VERSION=24.21.0
ARG NODE_SHA256_X64=fd8e59d5a511510f6a298afb548f18c7d2b1be404d8b4a27d94fbe49f56cb2d6
ARG NODE_SHA256_ARM64=6ad1325edbdb5649c379b75a237147a666c95d4f9ae8d340fef2d1575d289ad2
ARG RUST_TOOLCHAIN=1.98.1

# RUSTUP_TOOLCHAIN makes rustup use this toolchain without reading rust-toolchain.toml, which would try to download
# the std of all six targets (and fail in the offline AppImage step); build-linux.sh checks that the versions match.
ENV DEBIAN_FRONTEND=noninteractive \
    PATH=/usr/local/cargo/bin:/usr/local/node/bin:$PATH \
    RUSTUP_HOME=/usr/local/rustup \
    CARGO_HOME=/usr/local/cargo \
    RUSTUP_TOOLCHAIN=$RUST_TOOLCHAIN

# Tauri's Linux prerequisites (webkit2gtk 4.1, GTK 3, rsvg for icons), the binary checks (file, binutils), and
# xdg-utils: the AppImage bundler copies xdg-open for the opener plugin.
RUN apt-get update -q \
 && apt-get install -y -q --no-install-recommends \
      build-essential ca-certificates curl file binutils xz-utils git pkg-config xdg-utils desktop-file-utils \
      libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev \
 && rm -rf /var/lib/apt/lists/*

RUN set -eu; \
    case "$(uname -m)" in \
      x86_64) arch=x64; sha="$NODE_SHA256_X64" ;; \
      aarch64) arch=arm64; sha="$NODE_SHA256_ARM64" ;; \
      *) echo "unsupported CPU $(uname -m)" >&2; exit 1 ;; \
    esac; \
    curl -fsSL -o /tmp/node.tar.xz "https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-${arch}.tar.xz"; \
    echo "$sha  /tmp/node.tar.xz" | sha256sum -c -; \
    mkdir -p /usr/local/node; tar -xJf /tmp/node.tar.xz -C /usr/local/node --strip-components=1; rm /tmp/node.tar.xz; \
    node --version; corepack enable

# The toolchain from rust-toolchain.toml (installed here so the build itself needs no rustup download).
RUN set -eu; \
    curl -fsSL https://sh.rustup.rs -o /tmp/rustup.sh; \
    sh /tmp/rustup.sh -y --no-modify-path --profile minimal --default-toolchain "$RUST_TOOLCHAIN" -c clippy -c rustfmt; \
    rm /tmp/rustup.sh; rustc --version; chmod -R a+rwX "$RUSTUP_HOME" "$CARGO_HOME"
