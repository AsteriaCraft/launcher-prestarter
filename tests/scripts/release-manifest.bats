#!/usr/bin/env bats
# scripts/ci/release-manifest.sh (ADR 0009): asset metadata, the old plain format, refusals.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  SCRIPT="$ROOT/scripts/ci/release-manifest.sh"
  DIST="$(mktemp -d)"
  printf 'a' > "$DIST/Prestarter.exe"
  printf 'bb' > "$DIST/Prestarter-linux-aarch64"
  printf '{}' > "$DIST/release-notes.json"
  COMMIT=8b9cd54532caac25102dc23280b50e8b22e69021
}

teardown() {
  rm -rf "$DIST"
}

@test "plain names give the schema-1 manifest without metadata" {
  run bash "$SCRIPT" "$DIST" AsteriaCraft/launcher-prestarter prestarter 0.3.0 "$COMMIT" Prestarter.exe
  [ "$status" -eq 0 ]
  run jq -c '.assets' "$DIST/release.json"
  [ "$output" = '[{"name":"Prestarter.exe","size":1,"sha256":"ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb"}]' ]
  run jq -r '.schema, .tag, .channel, .component' "$DIST/release.json"
  [ "$output" = $'1\nv0.3.0\nstable\nprestarter' ]
  grep -q '  Prestarter.exe$' "$DIST/SHA256SUMS.txt"
}

@test "metadata is added per asset and '-' leaves a field out" {
  run bash "$SCRIPT" "$DIST" AsteriaCraft/launcher-prestarter prestarter 0.3.0 "$COMMIT" \
    Prestarter.exe:windows:x86_64:exe:prestarter Prestarter-linux-aarch64:linux:aarch64:elf:prestarter release-notes.json:-:-:json:notes
  [ "$status" -eq 0 ]
  run jq -c '.assets[1] | {os, arch, format, role}' "$DIST/release.json"
  [ "$output" = '{"os":"linux","arch":"aarch64","format":"elf","role":"prestarter"}' ]
  run jq -c '.assets[2] | keys' "$DIST/release.json"
  [ "$output" = '["format","name","role","sha256","size"]' ]
}

@test "a pre-release version is the prerelease channel" {
  run bash "$SCRIPT" "$DIST" AsteriaCraft/launcher-prestarter prestarter 0.3.0-rc.1 "$COMMIT" Prestarter.exe
  [ "$status" -eq 0 ]
  [ "$(jq -r .channel "$DIST/release.json")" = prerelease ]
}

@test "refuses bad specs, names and versions" {
  run bash "$SCRIPT" "$DIST" AsteriaCraft/launcher-prestarter prestarter 0.3.0 "$COMMIT" Prestarter.exe:windows:x86_64
  [ "$status" -eq 2 ]
  run bash "$SCRIPT" "$DIST" AsteriaCraft/launcher-prestarter prestarter 0.3.0 "$COMMIT" Prestarter.exe:Windows:x86_64:exe:prestarter
  [ "$status" -eq 2 ]
  run bash "$SCRIPT" "$DIST" AsteriaCraft/launcher-prestarter prestarter 0.3.0 "$COMMIT" release.json
  [ "$status" -eq 2 ]
  run bash "$SCRIPT" "$DIST" AsteriaCraft/launcher-prestarter prestarter 0.3.0 "$COMMIT" missing.bin
  [ "$status" -eq 2 ]
  run bash "$SCRIPT" "$DIST" AsteriaCraft/launcher-prestarter prestarter v0.3.0 "$COMMIT" Prestarter.exe
  [ "$status" -eq 2 ]
  run bash "$SCRIPT" "$DIST" AsteriaCraft/launcher-prestarter prestarter 0.3.0 nothex Prestarter.exe
  [ "$status" -eq 2 ]
}
