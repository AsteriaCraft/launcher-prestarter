#!/usr/bin/env bats
# scripts/ci/check-workflow-permissions.py: a job that calls a reusable workflow grants what that workflow asks for.

setup() {
  CHECK="$BATS_TEST_DIRNAME/../../scripts/ci/check-workflow-permissions.py"
  WORK="$(mktemp -d)"
}

teardown() {
  rm -rf "$WORK"
}

called_workflow() { # a reusable workflow whose job reads the repository
  cat > "$WORK/called.yml" <<'EOF'
on:
  workflow_call:
permissions:
  contents: read
jobs:
  job:
    runs-on: ubuntu-latest
    steps:
      - run: true
EOF
}

@test "the repository's own workflows give every called workflow what it asks for" {
  run python3 "$CHECK" "$BATS_TEST_DIRNAME/../../.github/workflows"
  [ "$status" -eq 0 ]
  [ -z "$output" ]
}

@test "a caller job under 'permissions: {}' that grants nothing is refused (the v0.3.0-rc.1 startup_failure)" {
  called_workflow
  cat > "$WORK/caller.yml" <<'EOF'
on: push
permissions: {}
jobs:
  build:
    uses: ./.github/workflows/called.yml
EOF
  run python3 "$CHECK" "$WORK"
  [ "$status" -eq 1 ]
  [[ "$output" == *"caller.yml: job 'build' calls called.yml, which asks for contents: read, but the job grants contents: none"* ]]
}

@test "a caller job that grants the scope at job level passes" {
  called_workflow
  cat > "$WORK/caller.yml" <<'EOF'
on: push
permissions: {}
jobs:
  build:
    uses: ./.github/workflows/called.yml
    permissions:
      contents: read
EOF
  run python3 "$CHECK" "$WORK"
  [ "$status" -eq 0 ]
}

@test "a workflow-wide grant covers its calling jobs" {
  called_workflow
  cat > "$WORK/caller.yml" <<'EOF'
on: push
permissions:
  contents: read
jobs:
  build:
    uses: ./.github/workflows/called.yml
EOF
  run python3 "$CHECK" "$WORK"
  [ "$status" -eq 0 ]
}
