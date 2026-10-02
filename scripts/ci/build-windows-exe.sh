#!/usr/bin/env bash
# Builds the raw Windows prestarter on LINUX with cargo-xwin (ADR 0009): the same script runs in CI and locally on any
# Linux box (`docker run ubuntu:24.04`), so a release build is reproducible off CI.
#
# Usage: build-windows-exe.sh [x86_64|aarch64]      (default x86_64)
#   x86_64  -> dist/Prestarter.exe                  (the name the production LaunchServer config expects)
#   aarch64 -> dist/Prestarter-windows-aarch64.exe
#
# Needs: Node.js with corepack (yarn comes from package.json "packageManager", hash-checked), rustup,
#        clang + lld + llvm (clang-cl, lld-link, llvm-lib, llvm-rc for the Windows resources), file.
#
# cargo-xwin downloads the MSVC CRT and the Windows SDK from Microsoft on first use (license: see
# https://github.com/rust-cross/cargo-xwin). Their versions are pinned below so the bytes do not change with
# Microsoft's current manifest; XWIN_CACHE_DIR may point at a shared cache.
set -euo pipefail

CARGO_XWIN_VERSION="${CARGO_XWIN_VERSION:-0.23.1}"
export XWIN_VERSION="${XWIN_VERSION:-17}"
export XWIN_CRT_VERSION="${XWIN_CRT_VERSION:-14.44.17.14}"
export XWIN_SDK_VERSION="${XWIN_SDK_VERSION:-10.0.26100}"

arch="${1:-x86_64}"
case "$arch" in
  x86_64) target=x86_64-pc-windows-msvc; out=Prestarter.exe; machine='x86-64' ;;
  aarch64) target=aarch64-pc-windows-msvc; out=Prestarter-windows-aarch64.exe; machine='Aarch64|ARM64' ;;
  *) echo "usage: $0 [x86_64|aarch64]" >&2; exit 2 ;;
esac

cd "$(dirname "$0")/../.."

for tool in node corepack rustup cargo clang lld-link llvm-rc file; do
  command -v "$tool" >/dev/null || { echo "build-windows-exe: missing tool: $tool" >&2; exit 1; }
done

rustup target add "$target"
if [ "$(cargo xwin --version 2>/dev/null | awk '{print $2}')" != "$CARGO_XWIN_VERSION" ]; then
  cargo install --locked cargo-xwin --version "$CARGO_XWIN_VERSION"
fi

corepack enable
yarn install --frozen-lockfile
# bundle.active=false in tauri.conf.json: only the raw exe is produced (no NSIS/MSI installer).
node_modules/.bin/tauri build --runner cargo-xwin --target "$target" --no-bundle

exe="${CARGO_TARGET_DIR:-src-tauri/target}/$target/release/Prestarter.exe"
test -s "$exe" || { echo "build-windows-exe: $exe was not produced" >&2; exit 1; }
kind="$(file -b "$exe")"
if ! [[ "$kind" == "PE32+ executable"* ]] || ! grep -Eq "$machine" <<< "$kind"; then
  echo "build-windows-exe: unexpected binary type for $arch: $kind" >&2
  exit 1
fi
mkdir -p dist
cp "$exe" "dist/$out"
echo "build-windows-exe: dist/$out ($(stat -c %s "dist/$out") bytes, $kind, CRT $XWIN_CRT_VERSION, SDK $XWIN_SDK_VERSION)"
