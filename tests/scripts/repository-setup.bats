#!/usr/bin/env bats
# The owner's repository setup (ADR 0009): scripts/setup-repository.sh and make-release-signing-key.sh against a fake
# GitHub CLI that records every call, .github/protect.json, and the "CI result" job it requires.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  export FAKE_GH_DIR="$BATS_TEST_TMPDIR/gh"
  mkdir -p "$FAKE_GH_DIR" "$BATS_TEST_TMPDIR/bin"
  : > "$FAKE_GH_DIR/calls.log"
  cat > "$BATS_TEST_TMPDIR/bin/gh" <<'GH'
#!/usr/bin/env bash
# Records each call as one line; keeps the secrets of the repository and of the environment release in files.
printf '%s\n' "$*" >> "$FAKE_GH_DIR/calls.log"
list() { [ -f "$1" ] || return 0; while read -r name; do [ -n "$name" ] && printf '%s\t2026-10-03T00:00:00Z\n' "$name"; done < "$1"; }
case "$*" in
  "secret set RELEASE_SIGNING_KEY --env release "*)
    cat > "$FAKE_GH_DIR/secret-stdin"
    echo RELEASE_SIGNING_KEY >> "$FAKE_GH_DIR/env-secrets"
    ;;
  "secret set "*) cat > "$FAKE_GH_DIR/secret-stdin" ;;
  "secret list --env release "*) list "$FAKE_GH_DIR/env-secrets" ;;
  "secret list -R "*) list "$FAKE_GH_DIR/repo-secrets" ;;
  "secret delete RELEASE_SIGNING_KEY -R "*)
    grep -vx RELEASE_SIGNING_KEY "$FAKE_GH_DIR/repo-secrets" > "$FAKE_GH_DIR/repo-secrets.new" || true
    mv "$FAKE_GH_DIR/repo-secrets.new" "$FAKE_GH_DIR/repo-secrets"
    ;;
  "api repos/"*"/deployment-branch-policies "*) cat "$FAKE_GH_DIR/policies" 2>/dev/null || true ;;
  "api repos/"*"/protection "*) echo "CI result" ;;
esac
exit 0
GH
  chmod +x "$BATS_TEST_TMPDIR/bin/gh"
  export PATH="$BATS_TEST_TMPDIR/bin:$PATH"
  # The "committed" key of these tests, and a stranger's key.
  openssl genpkey -algorithm ed25519 -out "$BATS_TEST_TMPDIR/trusted.pem" 2>/dev/null
  openssl pkey -in "$BATS_TEST_TMPDIR/trusted.pem" -pubout -out "$BATS_TEST_TMPDIR/trusted.pub.pem"
  openssl genpkey -algorithm ed25519 -out "$BATS_TEST_TMPDIR/new.pem" 2>/dev/null
  export RELEASE_SIGNING_PUBKEY="$BATS_TEST_TMPDIR/trusted.pub.pem"
  echo RELEASE_SIGNING_KEY > "$FAKE_GH_DIR/repo-secrets"
}

line_of() { grep -nF -- "$1" "$FAKE_GH_DIR/calls.log" | head -n 1 | cut -d: -f1; }
# A bare `! grep` never fails a bats test (set -e ignores negated commands); this does.
refute_grep() {
  if grep "$@"; then
    echo "unexpected match: grep $*" >&2
    return 1
  fi
}

@test "protect.json: one check, CI result from GitHub Actions, up to date; a PR without approvals; admins too" {
  run jq -e '
    .required_status_checks.strict == true
    and .required_status_checks.checks == [{"context": "CI result", "app_id": 15368}]
    and .enforce_admins == true
    and .required_pull_request_reviews.required_approving_review_count == 0
    and .allow_force_pushes == false and .allow_deletions == false and .restrictions == null
  ' "$ROOT/.github/protect.json"
  [ "$status" -eq 0 ]
}

@test "ci.yml: the result job is \"CI result\" for PRs and dispatches, \"CI result (push)\" for pushes, and needs every other job" {
  ci="$ROOT/.github/workflows/ci.yml"
  jobs="$(awk '/^jobs:/ {in_jobs = 1; next} in_jobs && /^  [a-z][a-z0-9-]*:$/ {sub(/:$/, ""); sub(/^  /, ""); print}' "$ci" | sort)"
  needs="$(awk '/^  result:$/ {in_result = 1; next} in_result && /^    needs:/ {print; exit}' "$ci" \
    | sed -e 's/.*\[//' -e 's/\].*//' | tr ',' '\n' | tr -d ' ' | sort)"
  expected="$(grep -vx result <<< "$jobs")"
  [ -n "$needs" ]
  [ "$needs" = "$expected" ]
  name="$(awk '/^  result:$/ {in_result = 1; next} in_result && /^    name:/ {print; exit}' "$ci")"
  context="$(jq -r '.required_status_checks.checks[0].context' "$ROOT/.github/protect.json")"
  [[ "$name" == *"github.event_name == 'push' && '$context (push)' || '$context'"* ]]
}

@test "setup-repository.sh refuses a key that is not the committed one, before changing anything" {
  run bash "$ROOT/scripts/setup-repository.sh" o/r --release-key "$BATS_TEST_TMPDIR/new.pem"
  [ "$status" -eq 1 ]
  [[ "$output" == *"is not $RELEASE_SIGNING_PUBKEY"* ]]
  [[ "$output" == *"Nothing was changed"* ]]
  refute_grep -q -- "secret set\|secret delete\|--method" "$FAKE_GH_DIR/calls.log"
}

@test "setup-repository.sh moves the existing key into the environment, then deletes the repository secret" {
  run bash "$ROOT/scripts/setup-repository.sh" o/r --release-key "$BATS_TEST_TMPDIR/trusted.pem"
  [ "$status" -eq 0 ]
  key_b64="$(openssl pkey -in "$BATS_TEST_TMPDIR/trusted.pem" -outform DER | base64 | tr -d '\r\n')"
  [ "$(cat "$FAKE_GH_DIR/secret-stdin")" = "$key_b64" ]
  refute_grep -qF -- "$key_b64" "$FAKE_GH_DIR/calls.log"
  [[ "$output" != *"$key_b64"* ]]
  set_at="$(line_of "secret set RELEASE_SIGNING_KEY --env release -R o/r")"
  delete_at="$(line_of "secret delete RELEASE_SIGNING_KEY -R o/r")"
  [ -n "$set_at" ] && [ -n "$delete_at" ] && [ "$set_at" -lt "$delete_at" ]
  refute_grep -qx "secret set RELEASE_SIGNING_KEY -R o/r" "$FAKE_GH_DIR/calls.log"
  grep -qF "api --method PUT repos/o/r/environments/release --silent" "$FAKE_GH_DIR/calls.log"
  grep -qF "api --method POST repos/o/r/environments/release/deployment-branch-policies --silent -f name=release -f type=branch" "$FAKE_GH_DIR/calls.log"
  for branch in main release; do
    grep -qF "api --method PUT repos/o/r/branches/$branch/protection --silent --input $ROOT/.github/protect.json" "$FAKE_GH_DIR/calls.log"
  done
  grep -qF "api --method PUT repos/o/r/actions/permissions/workflow --silent -f default_workflow_permissions=read -F can_approve_pull_request_reviews=true" "$FAKE_GH_DIR/calls.log"
  [ ! -s "$FAKE_GH_DIR/repo-secrets" ]
  [[ "$output" == *"Environment release secrets: RELEASE_SIGNING_KEY"* ]]
}

@test "setup-repository.sh run again: no second branch policy, no repository secret to delete" {
  echo "branch:release" > "$FAKE_GH_DIR/policies"
  : > "$FAKE_GH_DIR/repo-secrets"
  run bash "$ROOT/scripts/setup-repository.sh" o/r --release-key "$BATS_TEST_TMPDIR/trusted.pem"
  [ "$status" -eq 0 ]
  refute_grep -q "deployment-branch-policies --silent" "$FAKE_GH_DIR/calls.log"
  refute_grep -q "secret delete" "$FAKE_GH_DIR/calls.log"
}

@test "setup-repository.sh --dry-run checks the key and changes nothing" {
  run bash "$ROOT/scripts/setup-repository.sh" o/r --release-key "$BATS_TEST_TMPDIR/trusted.pem" --dry-run
  [ "$status" -eq 0 ]
  [[ "$output" == *"would run: gh secret delete RELEASE_SIGNING_KEY -R o/r"* ]]
  [[ "$output" == *"would run: openssl pkey -in $BATS_TEST_TMPDIR/trusted.pem -outform DER"*"--env release -R o/r"* ]]
  refute_grep -q -- "secret set\|secret delete\|--method" "$FAKE_GH_DIR/calls.log"
}

@test "setup-repository.sh without a key is a usage error" {
  run bash "$ROOT/scripts/setup-repository.sh" o/r
  [ "$status" -eq 2 ]
  [ ! -s "$FAKE_GH_DIR/calls.log" ]
}

@test "make-release-signing-key.sh uploads a new key into the environment release, never as a repository secret" {
  mkdir -p "$BATS_TEST_TMPDIR/work"
  cd "$BATS_TEST_TMPDIR/work"
  run bash "$ROOT/scripts/make-release-signing-key.sh" o/r prestarter
  [ "$status" -eq 0 ]
  grep -qx "secret set RELEASE_SIGNING_KEY --env release -R o/r" "$FAKE_GH_DIR/calls.log"
  refute_grep -qx "secret set RELEASE_SIGNING_KEY -R o/r" "$FAKE_GH_DIR/calls.log"
  key="$(ls prestarter-release-*.pem)"
  key_b64="$(openssl pkey -in "$key" -outform DER | base64 | tr -d '\r\n')"
  [ "$(cat "$FAKE_GH_DIR/secret-stdin")" = "$key_b64" ]
  refute_grep -qF -- "$key_b64" "$FAKE_GH_DIR/calls.log"
  [[ "$output" != *"$key_b64"* ]]
}
