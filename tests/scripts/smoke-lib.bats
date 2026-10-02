#!/usr/bin/env bats
# scripts/smoke/lib.sh and scripts/smoke/serve.py: the helpers every smoke script relies on (ADR 0006, 0009).

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  WORK="$(mktemp -d)"
  export SMOKE_OUT="$WORK/out"
  # shellcheck source=scripts/smoke/lib.sh
  source "$ROOT/scripts/smoke/lib.sh"
}

teardown() {
  stop_servers
  rm -rf "$WORK"
}

@test "Java reads Cyrillic folders outside Windows" {
  [ "$(host_os)" != windows ] || skip "the answer depends on the code page"
  [ -z "$(windows_ansi_code_page)" ]
  [ "$(readable_folder)" = "Ігри з пробілом" ]
}

@test "wait_log finds a line that appears later" {
  (sleep 1; echo "the page is ready after 812 ms; showing the window" >> "$WORK/p.log") &
  run wait_log "$WORK/p.log" "showing the window" 10
  [ "$status" -eq 0 ]
}

@test "wait_log gives up on a missing line" {
  echo "the page did not report ready in 15 s; starting anyway" > "$WORK/p.log"
  run wait_log "$WORK/p.log" "showing the window" 1
  [ "$status" -eq 1 ]
}

@test "screenshot_window records whether the window came" {
  screenshot() { echo "shot $1" >> "$WORK/shots"; }
  echo "INFO [app::commands] the page is ready after 640 ms; showing the window" > "$WORK/p.log"
  screenshot_window "$WORK/p.log" first 5
  [ "$WINDOW_SHOWN" = 1 ]
  : > "$WORK/q.log"
  screenshot_window "$WORK/q.log" second 1
  [ "$WINDOW_SHOWN" = 0 ]
  [ "$(cat "$WORK/shots")" = "$(printf 'shot first\nshot second')" ]
}

@test "wait_prestarter returns the exit code of a process that ends" {
  bash -c 'sleep 1; exit 3' &
  local waited=0
  wait_prestarter "$!" "$WORK/p.log" 10 || waited=$?
  [ "$waited" -eq 0 ]
  [ "$EXIT_CODE" -eq 3 ]
}

@test "wait_prestarter stops a window that shows an error" {
  screenshot() { :; }
  sleep 30 &
  local pid=$! waited=0
  echo "2026-10-02T18:26:40.074Z ERROR [app::worker] JreInstall (exit 3): Access is denied. (os error 5)" > "$WORK/p.log"
  wait_prestarter "$pid" "$WORK/p.log" 20 || waited=$?
  [ "$waited" -eq 2 ]
  ! kill -0 "$pid" 2>/dev/null
  [ "$(shown_error "$WORK/p.log")" = "JreInstall (exit 3): Access is denied. (os error 5)" ]
}

@test "wait_prestarter gives up after its time" {
  sleep 30 &
  local pid=$! waited=0
  wait_prestarter "$pid" "$WORK/p.log" 1 || waited=$?
  [ "$waited" -eq 1 ]
  [ "$EXIT_CODE" -eq 124 ]
}

@test "serve_dir serves a directory on loopback" {
  mkdir -p "$WORK/www"
  echo jar > "$WORK/www/Asterium.jar"
  serve_dir "$WORK/www"
  [ "$SERVE_PORT" -gt 0 ]
  [ "$(curl -fsS "http://127.0.0.1:$SERVE_PORT/Asterium.jar")" = jar ]
}
