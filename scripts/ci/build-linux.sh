#!/usr/bin/env bash
# Builds both Linux artifacts for this machine's CPU from ONE compile (ADR 0002, 0007, 0009):
#   dist/Prestarter-linux-<arch>            the raw single file (the LaunchServer appends the jar: Asterium_linux*)
#   dist/Asterium-linux-<arch>.AppImage     the self-contained wrapper (copy mode)
# <arch> is x86_64 or aarch64 (uname -m).
#
# Usage:
#   build-linux.sh docker     on a host with Docker (CI: ubuntu-24.04 / ubuntu-24.04-arm): builds the pinned
#                             Ubuntu 22.04 image (linux-builder.Dockerfile) and runs the steps below in it, the
#                             AppImage step with --network none
#   build-linux.sh tools      download the AppImage tools listed in appimage-tools.lock and check their sha256
#   build-linux.sh compile    yarn install, tauri build --no-bundle, checks on the ELF
#   build-linux.sh appimage   tauri bundle --bundles appimage from the compiled binary, with the pinned tools only
#   build-linux.sh verify     file type, glibc ceiling and libssl checks of both artifacts
#
# Checks: the ELF needs at most GLIBC_2.34 (objdump -T) and links no libssl/libcrypto (readelf -d); the AppImage's
# own glibc requirement is measured and printed. Exit status is non-zero on any failed check.
set -euo pipefail

cd "$(dirname "$0")/../.."
root="$PWD"
step="${1:-}"
arch="$(uname -m)"
case "$arch" in
  x86_64 | aarch64) ;;
  *) echo "build-linux: unsupported CPU $arch" >&2; exit 1 ;;
esac
GLIBC_CEILING="2.34"
TOOLS_DIR="$root/.tools/appimage-$arch"
TARGET_DIR="${CARGO_TARGET_DIR:-$root/src-tauri/target}"
ELF_OUT="dist/Prestarter-linux-$arch"
APPIMAGE_OUT="dist/Asterium-linux-$arch.AppImage"
IMAGE="${BUILDER_IMAGE:-prestarter-linux-builder}"

die() { echo "build-linux: $*" >&2; exit 1; }

# Highest GLIBC_x.y version an ELF file needs (empty if none: a static ELF such as the AppImage runtime, or a
# library without versioned glibc symbols, is not an error under pipefail).
glibc_needed() {
  { objdump -T "$1" 2>/dev/null || true; } | { grep -oE 'GLIBC_[0-9]+\.[0-9]+(\.[0-9]+)?' || true; } \
    | sed 's/GLIBC_//' | sort -uV | tail -n 1
}
version_le() { [ "$(printf '%s\n%s\n' "$1" "$2" | sort -V | head -n 1)" = "$1" ]; }

check_toolchain() {
  local want
  want="$(sed -n 's/^channel = "\(.*\)"$/\1/p' rust-toolchain.toml)"
  rustc --version | grep -q "^rustc $want " || die "rustc $(rustc --version) is not the pinned $want (rust-toolchain.toml)"
}

check_elf() { # <file> <label>
  local file="$1" label="$2" kind needed glibc
  kind="$(file -b "$file")"
  case "$arch:$kind" in
    x86_64:"ELF 64-bit LSB"*"x86-64"*) ;;
    aarch64:"ELF 64-bit LSB"*"ARM aarch64"*) ;;
    *) die "$label: unexpected binary type: $kind" ;;
  esac
  needed="$(readelf -d "$file" | sed -n 's/.*Shared library: \[\(.*\)\]/\1/p' | tr '\n' ' ')"
  case " $needed " in *" libssl"* | *" libcrypto"*) die "$label links OpenSSL: $needed" ;; esac
  glibc="$(glibc_needed "$file")"
  version_le "$glibc" "$GLIBC_CEILING" || die "$label needs GLIBC_$glibc, above the $GLIBC_CEILING ceiling"
  echo "build-linux: $label OK: $kind; GLIBC_$glibc; NEEDED: $needed"
}

step_tools() {
  mkdir -p "$TOOLS_DIR"
  local a name sha url
  while read -r a name sha url; do
    case "$a" in '' | '#'*) continue ;; esac
    [ "$a" = "$arch" ] || continue
    if [ ! -f "$TOOLS_DIR/$name" ] || ! echo "$sha  $TOOLS_DIR/$name" | sha256sum -c --status -; then
      curl -fsSL --retry 3 --proto '=https' -o "$TOOLS_DIR/$name.part" "$url"
      mv "$TOOLS_DIR/$name.part" "$TOOLS_DIR/$name"
    fi
    echo "$sha  $TOOLS_DIR/$name" | sha256sum -c - || die "$name does not match appimage-tools.lock"
  done < scripts/ci/appimage-tools.lock
}

step_compile() {
  check_toolchain
  corepack enable
  yarn install --frozen-lockfile
  node_modules/.bin/tauri build --no-bundle
  local exe="$TARGET_DIR/release/Prestarter"
  [ -s "$exe" ] || die "$exe was not produced"
  mkdir -p dist
  cp "$exe" "$ELF_OUT"
  check_elf "$ELF_OUT" "$ELF_OUT"
}

step_appimage() {
  check_toolchain
  # Only the verified tools: the bundler finds them in its cache and downloads nothing (this step runs offline).
  export XDG_CACHE_HOME="$root/.tools/xdg-cache"
  local cache="$XDG_CACHE_HOME/tauri" a name sha url
  rm -rf "$cache"
  mkdir -p "$cache"
  while read -r a name sha url; do
    case "$a" in '' | '#'*) continue ;; esac
    [ "$a" = "$arch" ] || continue
    echo "$sha  $TOOLS_DIR/$name" | sha256sum -c --status - || die "$name is missing or changed: run 'build-linux.sh tools' first"
    install -m 0755 "$TOOLS_DIR/$name" "$cache/$name"
  done < scripts/ci/appimage-tools.lock
  export APPIMAGE_EXTRACT_AND_RUN=1
  # The pinned runtime instead of appimagetool's download of `continuous`; verbose so a failure is explained.
  export LDAI_RUNTIME_FILE="$cache/type2-runtime-$arch" LDAI_VERBOSE=1
  node_modules/.bin/tauri bundle --bundles appimage --verbose
  local version built
  version="$(sed -n 's/^  "version": "\(.*\)",$/\1/p' src-tauri/tauri.conf.json | head -n 1)"
  case "$arch" in x86_64) built="Asterium_${version}_amd64.AppImage" ;; aarch64) built="Asterium_${version}_aarch64.AppImage" ;; esac
  [ -s "$TARGET_DIR/release/bundle/appimage/$built" ] || die "$built was not produced"
  mkdir -p dist
  cp "$TARGET_DIR/release/bundle/appimage/$built" "$APPIMAGE_OUT"
  chmod +x "$APPIMAGE_OUT"
}

step_verify() {
  [ -s "$ELF_OUT" ] || die "$ELF_OUT is missing"
  [ -s "$APPIMAGE_OUT" ] || die "$APPIMAGE_OUT is missing"
  check_elf "$ELF_OUT" "$ELF_OUT"
  local work max=0 file need
  work="$(mktemp -d)"
  (cd "$work" && APPIMAGE_EXTRACT_AND_RUN=1 "$root/$APPIMAGE_OUT" --appimage-extract >/dev/null)
  [ -x "$work/squashfs-root/usr/bin/Prestarter" ] || die "the AppImage has no usr/bin/Prestarter"
  check_elf "$work/squashfs-root/usr/bin/Prestarter" "AppImage usr/bin/Prestarter"
  # The AppImage carries Ubuntu 22.04 libraries; its real glibc floor is the highest any of them needs.
  while IFS= read -r -d '' file; do
    if [ "$(head -c 4 "$file" | od -An -tx1 | tr -d ' \n')" = 7f454c46 ]; then
      need="$(glibc_needed "$file")"
      if [ -n "$need" ] && ! version_le "$need" "$max"; then max="$need"; fi
    fi
  done < <(find "$work/squashfs-root" -type f -print0)
  rm -rf "$work"
  echo "build-linux: $APPIMAGE_OUT OK: $(file -b "$APPIMAGE_OUT" | cut -c1-60); needs GLIBC_$max on the host"
  for f in "$ELF_OUT" "$APPIMAGE_OUT"; do
    echo "build-linux: $f $(stat -c %s "$f") bytes sha256 $(sha256sum "$f" | cut -d' ' -f1)"
  done
}

step_docker() {
  command -v docker >/dev/null || die "docker is required"
  docker build -t "$IMAGE" -f scripts/ci/linux-builder.Dockerfile scripts/ci
  local run=(docker run --rm -v "$root:/src" -w /src -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)")
  [ -n "${CARGO_TARGET_DIR:-}" ] && run+=(-e "CARGO_TARGET_DIR=$CARGO_TARGET_DIR")
  # shellcheck disable=SC2016 # expanded by the shell inside the container
  "${run[@]}" "$IMAGE" bash -c 'scripts/ci/build-linux.sh tools && scripts/ci/build-linux.sh compile; rc=$?; chown -R "$HOST_UID:$HOST_GID" /src; exit $rc'
  # shellcheck disable=SC2016 # expanded by the shell inside the container
  "${run[@]}" --network none "$IMAGE" bash -c 'scripts/ci/build-linux.sh appimage; rc=$?; chown -R "$HOST_UID:$HOST_GID" /src; exit $rc'
  "${run[@]}" --network none "$IMAGE" scripts/ci/build-linux.sh verify
}

case "$step" in
  docker) step_docker ;;
  tools) step_tools ;;
  compile) step_compile ;;
  appimage) step_appimage ;;
  verify) step_verify ;;
  *) echo "usage: $0 docker|tools|compile|appimage|verify" >&2; exit 2 ;;
esac
