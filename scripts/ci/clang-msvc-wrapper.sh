#!/usr/bin/env bash
# Installed as `clang` (first in PATH) for the Windows ARM64 cross build only (build-windows-exe.sh aarch64).
#
# ring compiles its aarch64-pc-windows-msvc C code with plain `clang` (its build.rs switches away from clang-cl),
# but cargo-xwin passes the MSVC CRT/SDK include directories in clang-cl syntax (`/imsvc <dir>`) through
# CFLAGS_aarch64_pc_windows_msvc, which plain clang reads as a file name. This wrapper turns `/imsvc <dir>` into
# `-isystem <dir>` and runs the real clang; called as `clang-cl` (cargo-xwin symlinks clang-cl to the first `clang`
# in PATH) it hands everything unchanged to the real clang in clang-cl mode. cargo-xwin's own `clang` mode is not
# used because it downloads an unpinned third-party MSVC sysroot instead of the pinned CRT and SDK.
set -euo pipefail
real="${REAL_CLANG:?REAL_CLANG must name the real clang}"
if [ "$(basename "$0")" = clang-cl ]; then
  exec -a clang-cl "$real" "$@"
fi
args=()
while [ "$#" -gt 0 ]; do
  if [ "$1" = /imsvc ] && [ "$#" -ge 2 ]; then
    args+=(-isystem "$2")
    shift 2
  else
    args+=("$1")
    shift
  fi
done
exec "$real" "${args[@]}"
