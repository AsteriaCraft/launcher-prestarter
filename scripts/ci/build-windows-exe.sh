#!/usr/bin/env bash
# Builds Prestarter.exe (x86_64-pc-windows-msvc) on LINUX with cargo-xwin: the release CI runs on Linux only
# (private repos pay 2x Actions minutes on Windows runners). The same script is what the workflow runs and what a
# maintainer runs locally (any Linux box or `docker run ubuntu:24.04`), so a release build is reproducible off CI.
#
# Needs: Node.js with corepack (yarn comes from package.json "packageManager", hash-checked), rustup,
#        clang + lld + llvm (clang-cl, lld-link, llvm-lib, llvm-rc for the Windows resources), file.
# Output: dist/Prestarter.exe
#
# cargo-xwin downloads the MSVC CRT + Windows SDK headers/libs from Microsoft on first use (license: see
# https://github.com/rust-cross/cargo-xwin). XWIN_CACHE_DIR may point at a shared cache.
set -euo pipefail

CARGO_XWIN_VERSION="${CARGO_XWIN_VERSION:-0.23.1}"
TARGET=x86_64-pc-windows-msvc

cd "$(dirname "$0")/../.."

for tool in node corepack rustup cargo clang lld-link llvm-rc file; do
  command -v "$tool" >/dev/null || { echo "build-windows-exe: missing tool: $tool" >&2; exit 1; }
done

rustup target add "$TARGET"
if [ "$(cargo xwin --version 2>/dev/null | awk '{print $2}')" != "$CARGO_XWIN_VERSION" ]; then
  cargo install --locked cargo-xwin --version "$CARGO_XWIN_VERSION"
fi

corepack enable
yarn install --frozen-lockfile
# bundle.active=false in tauri.conf.json: only the raw exe is produced (no NSIS/MSI installer).
yarn tauri build --runner cargo-xwin --target "$TARGET" --no-bundle

exe="src-tauri/target/$TARGET/release/Prestarter.exe"
test -s "$exe" || { echo "build-windows-exe: $exe was not produced" >&2; exit 1; }
kind="$(file -b "$exe")"
case "$kind" in
  "PE32+ executable"*"x86-64"*) ;;
  *) echo "build-windows-exe: unexpected binary type: $kind" >&2; exit 1 ;;
esac
mkdir -p dist
cp "$exe" dist/Prestarter.exe
echo "build-windows-exe: dist/Prestarter.exe ($(stat -c %s dist/Prestarter.exe) bytes, $kind)"
