#!/usr/bin/env bats
# scripts/ci/macos-signing.sh check-secrets (ADR 0003): all five Apple secrets or none; some is an error that names
# the missing ones and never prints a value. Runs on any OS.

setup() {
  SCRIPT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)/scripts/ci/macos-signing.sh"
  unset APPLE_CERTIFICATE APPLE_CERTIFICATE_PASSWORD APPLE_ID APPLE_TEAM_ID APPLE_APP_PASSWORD
}

@test "no secrets: unsigned build" {
  run bash "$SCRIPT" check-secrets
  [ "$status" -eq 0 ]
  [ "$output" = none ]
}

@test "all five secrets: Developer ID signing" {
  APPLE_CERTIFICATE=c APPLE_CERTIFICATE_PASSWORD=p APPLE_ID=i APPLE_TEAM_ID=T APPLE_APP_PASSWORD=a \
    run bash "$SCRIPT" check-secrets
  [ "$status" -eq 0 ]
  [ "$output" = all ]
}

@test "some secrets: an error naming the missing ones, never a value" {
  APPLE_CERTIFICATE=secret-certificate-value APPLE_ID=someone@example.org run bash "$SCRIPT" check-secrets
  [ "$status" -eq 1 ]
  [[ "$output" == *"missing: APPLE_CERTIFICATE_PASSWORD APPLE_TEAM_ID APPLE_APP_PASSWORD"* ]]
  [[ "$output" != *"secret-certificate-value"* ]]
  [[ "$output" != *"someone@example.org"* ]]
}

@test "unknown mode is a usage error" {
  run bash "$SCRIPT" sign
  [ "$status" -eq 2 ]
}
