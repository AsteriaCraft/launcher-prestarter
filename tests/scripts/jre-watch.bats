#!/usr/bin/env bats
# scripts/ci/jre_webkit_watch.py on recorded file lists (ADR 0004).

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
}

@test "the native Windows ARM64 jre-full has no WebKit yet" {
  run python3 "$ROOT/scripts/ci/jre_webkit_watch.py" --names-file "$ROOT/tests/fixtures/jre-watch/windows-aarch64-dlls.txt"
  [ "$status" -eq 1 ]
  [[ "$output" == *"still has no jfxwebkit.dll"* ]]
}

@test "a jre-full with jfxwebkit.dll is reported" {
  run python3 "$ROOT/scripts/ci/jre_webkit_watch.py" --names-file "$ROOT/tests/fixtures/jre-watch/windows-x64-dlls.txt"
  [ "$status" -eq 0 ]
}

@test "an unreadable list is an error, not an answer" {
  run python3 "$ROOT/scripts/ci/jre_webkit_watch.py" --names-file "$ROOT/tests/fixtures/jre-watch/missing.txt"
  [ "$status" -eq 2 ]
}
