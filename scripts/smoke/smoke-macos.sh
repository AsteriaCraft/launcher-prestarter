#!/usr/bin/env bash
# macOS smoke (ADR 0003, 0009) on a macos-15 runner (Apple Silicon): the DMG as a player gets it.
#   - mount, layout (app + Applications link + background), copy to a temporary "Applications"
#   - lipo, codesign verification, the Gatekeeper verdict on a quarantined copy (what a player sees unsigned)
#   - Understand §8 once more: appending to the Mach-O or adding to Resources breaks the bundle seal
#   - the app in copy mode, natively (arm64) and under Rosetta (x86_64): Java from Liberica, the jar from loopback,
#     the FX probe window, xattr on what the prestarter wrote, the Dock name of the launcher's JVM
#
# Usage: smoke-macos.sh <Asterium-macos-universal.dmg>
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=scripts/smoke/lib.sh
source "$here/lib.sh"

dmg="${1:?dmg}"
probe="${PROBE_JAR:-dist/fixtures/FxProbe.jar}"
out="$SMOKE_OUT/macos"
mkdir -p "$out"
tmp="$(mktemp -d)"
mnt="$tmp/mnt"
trap 'stop_servers; hdiutil detach "$mnt" -quiet 2>/dev/null || true' EXIT

hdiutil attach -nobrowse -readonly -mountpoint "$mnt" "$dmg" >/dev/null
ls -la "$mnt" > "$out/dmg-contents.txt"
[ -d "$mnt/Asterium.app" ] || fail "the DMG has no Asterium.app"
[ "$(readlink "$mnt/Applications")" = /Applications ] || fail "the DMG has no Applications link"
ls "$mnt"/.background* >/dev/null 2>&1 || fail "the DMG has no background"
mkdir -p "$tmp/Applications"
ditto "$mnt/Asterium.app" "$tmp/Applications/Asterium.app"
hdiutil detach "$mnt" -quiet
app="$tmp/Applications/Asterium.app"
binary="$app/Contents/MacOS/$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$app/Contents/Info.plist")"

lipo -info "$binary" | tee "$out/lipo.txt"
codesign -dv --verbose=4 "$app" > "$out/codesign-dv.txt" 2>&1 || true
codesign --verify --strict --deep -vv "$app" 2>&1 | tee "$out/codesign-verify.txt"

cp -R "$app" "$tmp/Quarantined.app"
xattr -w com.apple.quarantine "0081;$(printf %x "$(date +%s)");Safari;" "$tmp/Quarantined.app"
set +e
spctl -a -vv -t exec "$tmp/Quarantined.app" > "$out/spctl-quarantined.txt" 2>&1
echo "spctl exit $?" >> "$out/spctl-quarantined.txt"
set -e
log "Gatekeeper on a quarantined copy: $(tr '\n' ' ' < "$out/spctl-quarantined.txt")"

{
  cp -R "$app" "$tmp/Appended.app"
  cat "$probe" >> "$tmp/Appended.app/Contents/MacOS/$(basename "$binary")"
  echo "== jar appended to the Mach-O:"
  codesign --verify --strict --deep -vv "$tmp/Appended.app" 2>&1 && echo "verify exit 0" || echo "verify exit $?"
  cp -R "$app" "$tmp/Resource.app"
  cp "$probe" "$tmp/Resource.app/Contents/Resources/Asterium.jar"
  echo "== jar added to Contents/Resources:"
  codesign --verify --strict --deep -vv "$tmp/Resource.app" 2>&1 && echo "verify exit 0" || echo "verify exit $?"
} > "$out/understand-8.txt"
cat "$out/understand-8.txt"

softwareupdate --install-rosetta --agree-to-license >/dev/null 2>&1 || true
arch -x86_64 /usr/bin/true || fail "Rosetta is not available on this runner"

mkdir -p "$tmp/www"
cp "$probe" "$tmp/www/Asterium.jar"
serve_dir "$tmp/www"
port="$SERVE_PORT"
export ASTERIUM_PRESTARTER_NONINTERACTIVE=1
export ASTERIUM_PRESTARTER_LAUNCHER_URL="http://127.0.0.1:$port/Asterium.jar"
export ASTERIUM_SMOKE_HOLD_MS=8000

run_slice() { # <label> <arch prefix...>
  local label="$1"
  shift
  local store="$tmp/store-$label" markers="$out/$label/markers"
  mkdir -p "$markers"
  export ASTERIUM_PRESTARTER_STORE="$store" ASTERIUM_SMOKE_DIR="$markers"
  "$@" "$binary" &
  local pid=$!
  screenshot_window "$store/logs/prestarter-1.log" "macos-$label-1-prestarter" 60
  local waited=0
  wait_prestarter "$pid" "$store/logs/prestarter-1.log" 900 || waited=$?
  collect_store "$store" "macos/$label"
  case "$waited" in
    1) fail "[$label] the app did not finish" ;;
    2) fail "[$label] the window shows an error: $(shown_error "$out/$label/prestarter-1.log")" ;;
  esac
  [ "$EXIT_CODE" = 0 ] || fail "[$label] the app exited with $EXIT_CODE"
  [ "$WINDOW_SHOWN" = 1 ] || fail "[$label] the window's page never reported ready (IPC, CSP or its script)"
  wait_file "$markers/fx-0.json" 120 || fail "[$label] the FX probe never reported"
  sleep 1
  screenshot "macos-$label-2-probe"
  lsappinfo list > "$out/$label/lsappinfo.txt" 2>&1 || true
  if grep -q '"Asterium"' "$out/$label/lsappinfo.txt"; then
    log "[$label] Dock shows the launcher's JVM as \"Asterium\" (JDK_JAVA_OPTIONS -Xdock:name)"
  else
    log "[$label] Dock name of the launcher's JVM: $(grep -o '"java[^"]*"' "$out/$label/lsappinfo.txt" | head -n 1) (open question 9)"
  fi
  {
    echo "== xattr -lr on the JRE"
    xattr -lr "$store/jre" | grep -v '^$' | head -n 20 || true
    echo "== xattr -l on the jar copy"
    xattr -l "$store/launcher/Asterium.jar" || true
  } > "$out/$label/xattr.txt"
  local fx_arch target
  fx_arch="$(json_get "$markers/fx-0.json" 'd["record"]["osArch"]')"
  target="$(json_get "$store/state.json" 'd["jre"]["target"]')"
  log "[$label] JRE $target, launcher os.arch=$fx_arch, WebKit $(json_get "$markers/fx-0.json" 'd["record"]["userAgent"]')"
  echo "$target $fx_arch"
}

native="$(run_slice arm64)"
[ "${native##*$'\n'}" = "macos-arm64 aarch64" ] || fail "native slice: $native"
rosetta="$(run_slice x86_64 arch -x86_64)"
[ "${rosetta##*$'\n'}" = "macos-x64 x86_64" ] || fail "Rosetta slice: $rosetta"
log "macOS OK"
