#!/usr/bin/env bash
# Smoke test of a raw prestarter in the EMBEDDED mode (Windows exe, Linux single file), on its native runner
# (ADR 0006, 0009): the jar is appended exactly like the LaunchServer's PrestarterTask does, the real Liberica API
# and download are used, and the launcher is the FX probe (Gravit-like wrapper + JavaFX/WebView window).
#
# Usage: smoke-embedded.sh <raw prestarter> <label> <expected os.arch of the JRE> [default-store|no-webview]
#   default-store  use the real default store (%LOCALAPPDATA%\Asterium\Prestarter, ~/.local/share/...)
#   no-webview     Windows: WEBVIEW2_BROWSER_EXECUTABLE_FOLDER points to an empty folder (the no-WebView mode)
# Needs dist/fixtures/FxProbe.jar. On Linux it must run under a display (xvfb-run).
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=scripts/smoke/lib.sh
source "$here/lib.sh"

raw="${1:?raw prestarter}"
label="${2:?label}"
expected_arch="${3:?expected os.arch}"
mode="${4:-}"
probe="${PROBE_JAR:-dist/fixtures/FxProbe.jar}"
if [ ! -s "$raw" ] || [ ! -s "$probe" ]; then fail "missing $raw or $probe"; fi
trap stop_servers EXIT

tmp="$(temp_root)/smoke-$label-$$"
dir="$tmp/Ігри з пробілом"
mkdir -p "$dir"
case "$(host_os)" in windows) exe="$dir/Asterium.exe" ;; *) exe="$dir/Asterium_linux" ;; esac
cat "$raw" "$probe" > "$exe"
chmod +x "$exe"
markers="$SMOKE_OUT/$label/markers"
mkdir -p "$markers"

export ASTERIUM_PRESTARTER_NONINTERACTIVE=1
export ASTERIUM_SMOKE_DIR
ASTERIUM_SMOKE_DIR="$(native_path "$markers")"
export ASTERIUM_SMOKE_HOLD_MS=6000
if [ "$mode" = default-store ]; then
  case "$(host_os)" in
    windows) store="$(cygpath -u "$LOCALAPPDATA")/Asterium/Prestarter" ;;
    *) store="${XDG_DATA_HOME:-$HOME/.local/share}/asterium/prestarter" ;;
  esac
  unset ASTERIUM_PRESTARTER_STORE
else
  store="$tmp/store"
  export ASTERIUM_PRESTARTER_STORE
  ASTERIUM_PRESTARTER_STORE="$(native_path "$store")"
fi
if [ "$mode" = no-webview ]; then
  mkdir -p "$tmp/no-webview2"
  export WEBVIEW2_BROWSER_EXECUTABLE_FOLDER
  WEBVIEW2_BROWSER_EXECUTABLE_FOLDER="$(native_path "$tmp/no-webview2")"
fi

log "[$label] first start: $exe"
started=$(date +%s)
"$exe" --smoke-arg "з пробілом" &
pid=$!
sleep 6
screenshot "$label-1-prestarter"
wait_pid "$pid" 900 || fail "[$label] the prestarter did not finish in 15 minutes"
collect_store "$store" "$label/run-1"
[ "$EXIT_CODE" = 0 ] || fail "[$label] the prestarter exited with $EXIT_CODE (logs in $SMOKE_OUT/$label/run-1)"
log "[$label] first start finished in $(( $(date +%s) - started )) s with exit code 0"
if [ "$mode" != no-webview ] && grep -q "did not report ready" "$SMOKE_OUT/$label/run-1/prestarter-1.log"; then
  fail "[$label] the window's page never reported its first frame (IPC or CSP problem)"
fi

wait_file "$markers/fx-0.json" 180 || fail "[$label] the FX probe window never reported (see launcher-start.log)"
screenshot "$label-2-probe"
fx_arch="$(json_get "$markers/fx-0.json" 'd["record"]["osArch"]')"
[ "$fx_arch" = "$expected_arch" ] || fail "[$label] the launcher JVM reports os.arch=$fx_arch, expected $expected_arch"
json_get "$markers/fx-0.json" 'd["record"]["title"]' | grep -qx Asterium || fail "[$label] wrong window title"
ua="$(json_get "$markers/fx-0.json" 'd["record"]["userAgent"]')"
[ -n "$ua" ] || fail "[$label] WebKit gave no user agent"
jar_seen="$(json_get "$markers/wrapper-0.json" 'd["jar"]')"
case "$jar_seen" in \\\\\?\\*) fail "[$label] the wrapper saw a verbatim path: $jar_seen" ;; esac
[ "$(basename "$jar_seen")" = "$(basename "$exe")" ] || fail "[$label] the wrapper ran $jar_seen, not $exe"
json_get "$markers/fx-0.json" 'd["args"]' | grep -q -- "--smoke-arg" || fail "[$label] the prestarter's arguments were not passed on"
target="$(json_get "$store/state.json" 'd["jre"]["target"]')"
log "[$label] JRE $(json_get "$store/state.json" 'd["jre"]["version"]') ($target), os.arch=$fx_arch, scene3d=$(json_get "$markers/fx-0.json" 'd["record"]["scene3d"]'), WebKit: $ua"
if [ "$mode" = no-webview ]; then
  grep -q "running without a window" "$SMOKE_OUT/$label/run-1/prestarter-1.log" || fail "[$label] the no-WebView mode was not used"
  grep -q "dialog (info)" "$SMOKE_OUT/$label/run-1/prestarter-1.log" || fail "[$label] the no-WebView notice was not logged"
  log "[$label] no-WebView mode: the notice went to the log and Java was installed without a window"
fi

log "[$label] second start (fast path: no window, no network)"
"$exe" &
pid=$!
wait_pid "$pid" 60 || fail "[$label] the fast path took more than 60 s"
collect_store "$store" "$label/run-2"
[ "$EXIT_CODE" = 0 ] || fail "[$label] the second start exited with $EXIT_CODE"
wait_file "$markers/fx-1.json" 120 || fail "[$label] the second launch did not open the probe"
grep -q "launcher started (Detached) without a window" "$SMOKE_OUT/$label/run-2/prestarter-1.log" \
  || fail "[$label] the second start did not take the fast path"
log "[$label] OK"
