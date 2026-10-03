#!/usr/bin/env bash
# The single file on Wayland (ADR 0002, 0007): weston headless, GDK_BACKEND=wayland, the stock tao from crates.io (no
# GravitLauncher6 fork). A screenshot of the prestarter window is the evidence: a GTK header bar on top of our
# borderless window would mean the tao patch has to come back (pinned by rev).
#
# Usage: smoke-wayland.sh <Prestarter-linux-x86_64>     (ubuntu-24.04 with weston and libwebkit2gtk-4.1-0)
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"
# shellcheck source=scripts/smoke/lib.sh
source "$here/lib.sh"

elf="$(realpath "${1:?ELF}")"
out="$SMOKE_OUT/wayland"
mkdir -p "$out"
export XDG_RUNTIME_DIR
XDG_RUNTIME_DIR="$(mktemp -d)"
chmod 700 "$XDG_RUNTIME_DIR"
weston --backend=headless --renderer=pixman --width=1024 --height=768 --socket=wayland-smoke --debug \
  >"$out/weston.log" 2>&1 &
weston_pid=$!
trap 'kill "$weston_pid" 2>/dev/null || true' EXIT
for _ in $(seq 1 50); do [ -S "$XDG_RUNTIME_DIR/wayland-smoke" ] && break; sleep 0.2; done
[ -S "$XDG_RUNTIME_DIR/wayland-smoke" ] || fail "weston did not start: $(cat "$out/weston.log")"

store="$(mktemp -d)"
# The raw file is in copy mode; a loopback launcher URL keeps it away from the production host (no policy fetch,
# and nothing listens on port 9: the run is stopped after the screenshot anyway).
env -u DISPLAY WAYLAND_DISPLAY=wayland-smoke GDK_BACKEND=wayland ASTERIUM_PRESTARTER_NONINTERACTIVE=1 \
  ASTERIUM_PRESTARTER_STORE="$store" ASTERIUM_PRESTARTER_LAUNCHER_URL=http://127.0.0.1:9/Asterium.jar \
  "$elf" >"$out/prestarter-stdout.txt" 2>&1 &
pid=$!
WAYLAND_SCREENSHOT=wayland-smoke screenshot_window "$store/logs/prestarter-1.log" "wayland-prestarter" 45
kill "$pid" 2>/dev/null || true
wait "$pid" 2>/dev/null || true
cp -f "$store"/logs/*.log "$out/" 2>/dev/null || true
[ -s "$SMOKE_OUT/wayland-prestarter.png" ] || fail "no Wayland screenshot"
[ "$WINDOW_SHOWN" = 1 ] || fail "the prestarter's window never appeared on Wayland (its page never reported ready)"
log "Wayland screenshot: $SMOKE_OUT/wayland-prestarter.png"
