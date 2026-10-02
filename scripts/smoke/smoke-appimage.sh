#!/usr/bin/env bash
# Smoke test of the AppImage (COPY mode, ADR 0001, 0002, 0006): the jar comes from a loopback server (ADR 0008), Java
# from the real Liberica API; the launcher (FX probe) must see none of the variables AppRun set.
#
# Usage: smoke-appimage.sh <AppImage> <label> <expected os.arch>     (under a display: xvfb-run)
# FUSE is used when the runner has it; otherwise APPIMAGE_EXTRACT_AND_RUN=1, which players without FUSE use too.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=scripts/smoke/lib.sh
source "$here/lib.sh"

appimage="${1:?AppImage}"
label="${2:?label}"
expected_arch="${3:?expected os.arch}"
probe="${PROBE_JAR:-dist/fixtures/FxProbe.jar}"
if [ ! -s "$appimage" ] || [ ! -s "$probe" ]; then fail "missing $appimage or $probe"; fi
trap stop_servers EXIT

tmp="$(temp_root)/smoke-$label-$$"
mkdir -p "$tmp/www" "$tmp/Downloads"
cp "$probe" "$tmp/www/Asterium.jar"
cp "$appimage" "$tmp/Downloads/Asterium.AppImage"
chmod +x "$tmp/Downloads/Asterium.AppImage"
serve_dir "$tmp/www"
port="$SERVE_PORT"
markers="$SMOKE_OUT/$label/markers"
mkdir -p "$markers"

export ASTERIUM_PRESTARTER_NONINTERACTIVE=1
export ASTERIUM_PRESTARTER_STORE="$tmp/store"
export ASTERIUM_PRESTARTER_LAUNCHER_URL="http://127.0.0.1:$port/Asterium.jar"
export ASTERIUM_SMOKE_DIR="$markers"
export ASTERIUM_SMOKE_HOLD_MS=6000
if [ ! -e /dev/fuse ] || ! command -v fusermount3 >/dev/null 2>&1; then
  export APPIMAGE_EXTRACT_AND_RUN=1
  log "[$label] no FUSE here: APPIMAGE_EXTRACT_AND_RUN=1"
fi

cd "$tmp/Downloads"
log "[$label] first start"
./Asterium.AppImage &
pid=$!
screenshot_window "$tmp/store/logs/prestarter-1.log" "$label-1-prestarter" 60
wait_pid "$pid" 900 || fail "[$label] the AppImage did not finish in 15 minutes"
collect_store "$tmp/store" "$label/run-1"
[ "$EXIT_CODE" = 0 ] || fail "[$label] the AppImage exited with $EXIT_CODE"
wait_file "$markers/fx-0.json" 120 || fail "[$label] the FX probe never reported"
screenshot "$label-2-probe"

[ "$(json_get "$markers/fx-0.json" 'd["record"]["osArch"]')" = "$expected_arch" ] || fail "[$label] wrong os.arch"
[ "$(basename "$(json_get "$markers/wrapper-0.json" 'd["jar"]')")" = Asterium.jar ] || fail "[$label] not the jar copy"
appdir="$(grep -o 'appimage_extracted_[A-Za-z0-9]*\|\.mount_[A-Za-z0-9]*' "$SMOKE_OUT/$label/run-1/prestarter-1.log" | head -n 1 || true)"
assert_clean_env "$markers/fx-0.json" "${appdir:-.mount_}"
cwd="$(json_get "$markers/fx-0.json" 'd["record"]["cwd"]')"
# OWD (where the player started the AppImage) when the runtime sets it, the home directory otherwise; never the
# mount or extraction directory, which disappears when the prestarter exits.
case "$cwd" in
  "$tmp/Downloads") log "[$label] the launcher starts in OWD ($cwd)" ;;
  "$HOME") log "[$label] the launcher starts in the home directory (no OWD from the runtime)" ;;
  *) fail "[$label] the launcher runs in $cwd, not in OWD or the home directory" ;;
esac
grep -q "wrapper outcome: Started" "$SMOKE_OUT/$label/run-1/prestarter-1.log" || fail "[$label] the wrapper was not watched"
[ "$WINDOW_SHOWN" = 1 ] || fail "[$label] the window's page never reported ready (IPC, CSP or its script)"

log "[$label] second start (fast path)"
./Asterium.AppImage &
pid=$!
wait_pid "$pid" 90 || fail "[$label] the fast path took more than 90 s"
collect_store "$tmp/store" "$label/run-2"
[ "$EXIT_CODE" = 0 ] || fail "[$label] the second start exited with $EXIT_CODE"
wait_file "$markers/fx-1.json" 120 || fail "[$label] the second launch did not open the probe"
log "[$label] OK (JRE $(json_get "$tmp/store/state.json" 'd["jre"]["version"]'), jar $(json_get "$tmp/store/state.json" 'd["jar"]["sha256"]'))"
