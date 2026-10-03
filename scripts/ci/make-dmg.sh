#!/usr/bin/env bash
# Packs Asterium.app into a DMG with an Applications link and the first-start steps as the window background
# (ADR 0003). dmgbuild writes the .DS_Store itself, so the layout works on a headless CI runner, where Tauri's own DMG
# bundler skips the Finder AppleScript. macOS only (hdiutil).
#
# Usage: make-dmg.sh <Asterium.app> <out.dmg>
# Needs python3; dmgbuild and its dependencies come from requirements-dmg.txt with --require-hashes into a venv.
set -euo pipefail

app="${1:?usage: make-dmg.sh <Asterium.app> <out.dmg>}"
out="${2:?usage: make-dmg.sh <Asterium.app> <out.dmg>}"
here="$(cd "$(dirname "$0")" && pwd)"
[ -d "$app/Contents/MacOS" ] || { echo "make-dmg: $app is not an app bundle" >&2; exit 1; }
[ "$(uname -s)" = Darwin ] || { echo "make-dmg: macOS only" >&2; exit 1; }

venv="${DMGBUILD_VENV:-$(mktemp -d)/venv}"
if [ ! -x "$venv/bin/dmgbuild" ]; then
  python3 -m venv "$venv"
  "$venv/bin/python" -m pip install --quiet --disable-pip-version-check --require-hashes --no-deps -r "$here/requirements-dmg.txt"
fi

rm -f "$out"
"$venv/bin/dmgbuild" -s "$here/dmg/dmg-settings.py" -D app="$app" -D background="$here/dmg/background.png" "Asterium" "$out"
hdiutil verify "$out" >/dev/null
echo "make-dmg: $out ($(stat -f %z "$out") bytes, sha256 $(shasum -a 256 "$out" | cut -d' ' -f1))"
