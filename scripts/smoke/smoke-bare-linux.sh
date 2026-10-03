#!/usr/bin/env bash
# "Bare" Linux (ADR 0002, 0006): a fresh ubuntu:24.04 container without GTK or WebKitGTK.
#   1. The single file cannot even load (exit 127): recorded behaviour, the site offers the AppImage by default.
#   2. The AppImage starts with the minimal host libraries (M2), installs Java without a display, then refuses to
#      launch and names the missing JavaFX packages (exit 5).
#   3. Exactly those packages are installed; under Xvfb the AppImage starts the launcher (FX probe).
#
# Usage: smoke-bare-linux.sh <Prestarter-linux-x86_64> <Asterium-linux-x86_64.AppImage>   (needs Docker)
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=scripts/smoke/lib.sh
source "$here/lib.sh"

elf="$(realpath "${1:?ELF}")"
appimage="$(realpath "${2:?AppImage}")"
probe="$(realpath "${PROBE_JAR:-dist/fixtures/FxProbe.jar}")"
image="ubuntu:24.04"
out="$SMOKE_OUT/bare-linux"
mkdir -p "$out"

set +e
docker run --rm -v "$elf:/opt/Asterium_linux:ro" "$image" /opt/Asterium_linux
code=$?
set -e
log "single file in a bare Ubuntu 24.04: exit $code"
[ "$code" = 127 ] || fail "the single file without WebKitGTK exited with $code, expected 127"

cat > "$out/inside.sh" <<'SCRIPT'
#!/usr/bin/env bash
set -uo pipefail
export DEBIAN_FRONTEND=noninteractive
apt-get update -q >/dev/null
# M2: what a desktop already has and the AppImage does not bundle (the GL, EGL and GLES libraries come from the host's
# graphics driver; linuxdeploy's excludelist never bundles them); ca-certificates for HTTPS; python3 for the jar server.
apt-get install -y -q --no-install-recommends ca-certificates libfontconfig1 libharfbuzz0b libfribidi0 libgl1 libegl1 libgles2 python3 >/dev/null
mkdir -p /www /work && cp /in/FxProbe.jar /www/Asterium.jar
python3 -m http.server 8765 --bind 127.0.0.1 --directory /www >/dev/null 2>&1 &
export APPIMAGE_EXTRACT_AND_RUN=1 ASTERIUM_PRESTARTER_NONINTERACTIVE=1 ASTERIUM_PRESTARTER_STORE=/work/store
export ASTERIUM_PRESTARTER_LAUNCHER_URL=http://127.0.0.1:8765/Asterium.jar ASTERIUM_SMOKE_DIR=/out/markers ASTERIUM_SMOKE_HOLD_MS=3000
cd /work
/in/Asterium.AppImage 2> /out/first-stderr.txt
echo "first-exit=$?" > /out/first-exit.txt
command="$(grep -o 'sudo apt install [a-z0-9 .+-]*' /out/first-stderr.txt | head -n 1)"
echo "$command" > /out/hint.txt
[ -n "$command" ] || exit 0
apt-get install -y -q --no-install-recommends ${command#sudo apt install } xvfb xauth >/dev/null
xvfb-run -a -s "-screen 0 1280x800x24" /in/Asterium.AppImage
echo "second-exit=$?" > /out/second-exit.txt
for i in $(seq 1 60); do [ -s /out/markers/fx-0.json ] && break; sleep 1; done
cp -r /work/store/logs /out/logs 2>/dev/null || true
SCRIPT
chmod +x "$out/inside.sh"
mkdir -p "$out/in"
cp "$appimage" "$out/in/Asterium.AppImage"
cp "$probe" "$out/in/FxProbe.jar"
chmod +x "$out/in/Asterium.AppImage"
docker run --rm -v "$out/in:/in:ro" -v "$out:/out" -v "$out/inside.sh:/inside.sh:ro" "$image" bash /inside.sh

grep -qx "first-exit=5" "$out/first-exit.txt" || fail "without GTK the AppImage exited with $(cat "$out/first-exit.txt"), expected first-exit=5"
hint="$(cat "$out/hint.txt")"
[ -n "$hint" ] || fail "no package hint on stderr: $(cat "$out/first-stderr.txt")"
log "package hint: $hint"
grep -qx "second-exit=0" "$out/second-exit.txt" || fail "after installing the hinted packages: $(cat "$out/second-exit.txt" 2>/dev/null)"
[ -s "$out/markers/fx-0.json" ] || fail "after installing the hinted packages the probe did not open"
log "bare Ubuntu 24.04: single file exit 127; AppImage hint '$hint'; after it the launcher opens"
