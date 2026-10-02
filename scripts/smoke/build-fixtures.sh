#!/usr/bin/env bash
# Builds the smoke fixtures (ADR 0009) into dist/fixtures:
#   Hello.jar    - tests/fixtures/hello, javac --release 17 (Java SE only): prints what a launcher jar sees
#   FxProbe.jar  - tests/fixtures/fxprobe: a Gravit-like wrapper plus a JavaFX + WebView window
# Needs a JDK 17 or newer with JavaFX (CI: Liberica jdk+fx) for FxProbe.jar.
set -euo pipefail
cd "$(dirname "$0")/../.."
out=dist/fixtures
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
mkdir -p "$out" "$work/hello" "$work/fxprobe"

javac --release 17 -encoding UTF-8 -d "$work/hello" tests/fixtures/hello/Hello.java
jar --create --file "$out/Hello.jar" --main-class Hello -C "$work/hello" .

if [ "${SKIP_FX:-0}" != 1 ]; then
  javac -encoding UTF-8 -source 17 -target 17 -Xlint:-options -d "$work/fxprobe" tests/fixtures/fxprobe/*.java
  jar --create --file "$out/FxProbe.jar" --main-class FxProbe -C "$work/fxprobe" .
fi
ls -l "$out"
