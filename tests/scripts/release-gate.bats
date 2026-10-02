#!/usr/bin/env bats
# scripts/ci/release-checks.sh and scripts/ci/make-policy.sh (ADR 0001, 0009, 0012), run on a copy of the repository.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  WORK="$(mktemp -d)"
  mkdir -p "$WORK/scripts/ci" "$WORK/src-tauri" "$WORK/release-notes" "$WORK/policy"
  cp "$ROOT/scripts/ci/release-checks.sh" "$ROOT/scripts/ci/make-policy.sh" "$WORK/scripts/ci/"
  cp "$ROOT/src-tauri/tauri.conf.json" "$ROOT/src-tauri/Cargo.toml" "$ROOT/src-tauri/Cargo.lock" "$WORK/src-tauri/"
  cp "$ROOT/package.json" "$ROOT/CHANGELOG.md" "$WORK/"
  cp "$ROOT"/release-notes/*.json "$WORK/release-notes/"
  cp "$ROOT/policy/prestarter-policy.json" "$WORK/policy/"
  VERSION="$(jq -r .version "$WORK/src-tauri/tauri.conf.json")"
}

teardown() {
  rm -rf "$WORK"
}

@test "the repository passes the branch gate" {
  run bash "$WORK/scripts/ci/release-checks.sh"
  [ "$status" -eq 0 ]
  [ "$output" = "$VERSION" ]
}

@test "the release gate wants a dated changelog and the same date in the notes" {
  sed -i "s/^## \[$VERSION\] - Unreleased$/## [$VERSION] - 2026-10-20/" "$WORK/CHANGELOG.md"
  run bash "$WORK/scripts/ci/release-checks.sh" --release
  [ "$status" -eq 1 ]
  [[ "$output" == *"date is null"* ]]
  jq '.date = "2026-10-20"' "$WORK/release-notes/$VERSION.json" > "$WORK/n.json" && mv "$WORK/n.json" "$WORK/release-notes/$VERSION.json"
  run bash "$WORK/scripts/ci/release-checks.sh" --release
  [ "$status" -eq 0 ]
}

@test "an undated changelog fails the release gate" {
  run bash "$WORK/scripts/ci/release-checks.sh" --release
  [ "$status" -eq 1 ]
  [[ "$output" == *"is not dated"* ]]
}

@test "different versions fail" {
  jq '.version = "9.9.9"' "$WORK/package.json" > "$WORK/p.json" && mv "$WORK/p.json" "$WORK/package.json"
  run bash "$WORK/scripts/ci/release-checks.sh"
  [ "$status" -eq 1 ]
  [[ "$output" == *"versions differ"* ]]
}

@test "notes without English fail" {
  jq 'del(.notes.en)' "$WORK/release-notes/$VERSION.json" > "$WORK/n.json" && mv "$WORK/n.json" "$WORK/release-notes/$VERSION.json"
  run bash "$WORK/scripts/ci/release-checks.sh"
  [ "$status" -eq 1 ]
}

@test "a note longer than 300 characters fails" {
  long="$(printf 'x%.0s' $(seq 1 301))"
  jq --arg l "$long" '.notes.uk.added += [$l]' "$WORK/release-notes/$VERSION.json" > "$WORK/n.json" && mv "$WORK/n.json" "$WORK/release-notes/$VERSION.json"
  run bash "$WORK/scripts/ci/release-checks.sh"
  [ "$status" -eq 1 ]
}

@test "the policy asset is the repository policy plus the release version" {
  run bash "$WORK/scripts/ci/make-policy.sh" "$VERSION" "$WORK/out/prestarter-policy.json"
  [ "$status" -eq 0 ]
  run jq -r '.latestWrapperVersion, .jarUrl, .schema' "$WORK/out/prestarter-policy.json"
  [ "$output" = "$VERSION"$'\nhttps://launcher.asterium.pro/Asterium.jar\n1' ]
}

@test "the policy refuses a minimum above the release" {
  run bash "$WORK/scripts/ci/make-policy.sh" 0.0.1 "$WORK/out/p.json"
  [ "$status" -eq 1 ]
  jq '.jarUrl = "http://insecure.example/A.jar"' "$WORK/policy/prestarter-policy.json" > "$WORK/p.json" && mv "$WORK/p.json" "$WORK/policy/prestarter-policy.json"
  run bash "$WORK/scripts/ci/make-policy.sh" "$VERSION" "$WORK/out/p.json"
  [ "$status" -eq 1 ]
}
