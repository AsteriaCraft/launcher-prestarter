#!/usr/bin/env bash
# Windows: the prestarter in a folder whose name the ANSI code page cannot write (Cyrillic under 1252, as on GitHub's
# runners). Java reads its command line in that code page, so it would get "?" in the path and fail with "Unable to
# access jarfile", and Gravit's own relaunch the same way (ADR 0006). The prestarter must refuse up front: exit code
# 6, the sentence that says what to do, nothing downloaded, Java never started.
#   1. no-WebView mode: the sentence goes to the log, the exit code is read directly
#   2. with the window: a screenshot of the error as the player sees it (the window waits for the player; stopped)
#   3. the file in an ASCII folder, the store under such a name (as a Windows user name puts it into the profile):
#      the refusal names the store and does not tell the player to move Asterium, which could not help
#
# Usage: smoke-ansi-path.sh <raw Prestarter.exe> <label>     (Needs dist/fixtures/FxProbe.jar.)
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=scripts/smoke/lib.sh
source "$here/lib.sh"

raw="${1:?raw prestarter}"
label="${2:?label}"
probe="${PROBE_JAR:-dist/fixtures/FxProbe.jar}"
if [ ! -s "$raw" ] || [ ! -s "$probe" ]; then fail "missing $raw or $probe"; fi
[ "$(host_os)" = windows ] || fail "Windows only"
code_page="$(windows_ansi_code_page)"
case "$code_page" in
  1251 | 65001)
    log "[$label] the ANSI code page $code_page writes Cyrillic: nothing to refuse here"
    exit 0
    ;;
esac

tmp="$(temp_root)/smoke-$label-$$"
dir="$tmp/Ігри з пробілом"
exe="$dir/Asterium.exe"
store="$tmp/store"
mkdir -p "$dir" "$tmp/no-webview2"
cat "$raw" "$probe" > "$exe"
markers="$SMOKE_OUT/$label/markers"
mkdir -p "$markers"
export ASTERIUM_PRESTARTER_NONINTERACTIVE=1 ASTERIUM_PRESTARTER_STORE ASTERIUM_SMOKE_DIR
ASTERIUM_PRESTARTER_STORE="$(native_path "$store")"
ASTERIUM_SMOKE_DIR="$(native_path "$markers")"

log "[$label] code page $code_page, no-WebView mode: $exe"
WEBVIEW2_BROWSER_EXECUTABLE_FOLDER="$(native_path "$tmp/no-webview2")" "$exe" &
pid=$!
wait_pid "$pid" 120 || fail "[$label] the prestarter did not exit in 2 minutes"
collect_store "$store" "$label/headless"
[ "$EXIT_CODE" = 6 ] || fail "[$label] exit code $EXIT_CODE, expected 6 (logs in $SMOKE_OUT/$label/headless)"
log_file="$SMOKE_OUT/$label/headless/prestarter-1.log"
grep -q "PathEncoding (exit 6): the ANSI code page $code_page cannot write" "$log_file" \
  || fail "[$label] the log does not name the refusal"
grep -q "Language for non-Unicode programs" "$log_file" || fail "[$label] the player's sentence was not logged"
[ -z "$(ls -A "$store/jre" 2>/dev/null)" ] || fail "[$label] Java was downloaded although the start was refused"
[ ! -e "$markers/wrapper-0.json" ] || fail "[$label] Java was started although the start was refused"
log "[$label] refused with exit code 6 before any download"

log "[$label] with the window: the error as the player sees it"
"$exe" &
pid=$!
screenshot_window "$store/logs/prestarter-1.log" "$label-error-window" 45
kill "$pid" 2>/dev/null || true
wait "$pid" 2>/dev/null || true
collect_store "$store" "$label/window"
[ "$WINDOW_SHOWN" = 1 ] || fail "[$label] the error window's page never reported ready"
grep -q "PathEncoding" "$SMOKE_OUT/$label/window/prestarter-1.log" || fail "[$label] the window run did not refuse"

log "[$label] the store under a user name the code page cannot write, the file in an ASCII folder"
user_store="$tmp/Користувач/AppData/Local/Asterium/Prestarter"
ascii_exe="$tmp/ascii/Asterium.exe"
mkdir -p "$tmp/ascii" "$tmp/Користувач"
cp "$exe" "$ascii_exe"
ASTERIUM_PRESTARTER_STORE="$(native_path "$user_store")" \
  WEBVIEW2_BROWSER_EXECUTABLE_FOLDER="$(native_path "$tmp/no-webview2")" "$ascii_exe" &
pid=$!
wait_pid "$pid" 120 || fail "[$label] the prestarter did not exit in 2 minutes (store case)"
collect_store "$user_store" "$label/store"
[ "$EXIT_CODE" = 6 ] || fail "[$label] store case: exit code $EXIT_CODE, expected 6 (logs in $SMOKE_OUT/$label/store)"
store_log="$SMOKE_OUT/$label/store/prestarter-1.log"
grep -q "StorePathEncoding (exit 6): the ANSI code page $code_page cannot write" "$store_log" \
  || fail "[$label] store case: the log does not name the store's refusal"
grep -q "Moving Asterium to another folder will not help" "$store_log" \
  || fail "[$label] store case: the player's sentence was not the store's"
if grep -qF 'C:\Games' "$store_log"; then fail "[$label] store case: the sentence tells the player to move Asterium"; fi
[ -z "$(ls -A "$user_store/jre" 2>/dev/null)" ] || fail "[$label] store case: Java was downloaded although the start was refused"
log "[$label] the store's refusal names the store and the code page, not the file's folder"
log "[$label] OK"
