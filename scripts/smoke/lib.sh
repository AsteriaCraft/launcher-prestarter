#!/usr/bin/env bash
# Shared helpers of the smoke scripts (ADR 0009). Sourced, not run. Works in bash on Linux, macOS and Git Bash on
# Windows. Everything a run produces (logs, probe records, screenshots) goes to $SMOKE_OUT for upload.

# Globals the helpers set for their callers: EXIT_CODE (wait_pid), SERVE_PORT (serve_dir), WINDOW_SHOWN
# (screenshot_window).
# shellcheck disable=SC2034
EXIT_CODE=0
# shellcheck disable=SC2034
SERVE_PORT=0
# shellcheck disable=SC2034
WINDOW_SHOWN=0
SMOKE_OUT="${SMOKE_OUT:-$PWD/smoke-out}"
mkdir -p "$SMOKE_OUT"
SERVER_PIDS=()

log() { echo "smoke: $*" >&2; }
fail() { echo "::error::smoke: $*" >&2; exit 1; }

host_os() {
  case "$(uname -s)" in
    Linux) echo linux ;;
    Darwin) echo macos ;;
    MINGW* | MSYS* | CYGWIN*) echo windows ;;
    *) echo unknown ;;
  esac
}

# A writable temporary root in this shell's own path form (in Git Bash: /d/a/_temp rather than the Windows form).
temp_root() {
  local root="${RUNNER_TEMP:-${TMPDIR:-/tmp}}"
  if [ "$(host_os)" = windows ]; then cygpath -u "$root"; else echo "$root"; fi
}

# A path the native (non-MSYS) programs understand.
native_path() {
  if [ "$(host_os)" = windows ]; then cygpath -w "$1"; else echo "$1"; fi
}

python_bin() {
  if command -v python3 >/dev/null 2>&1 && python3 -c '' >/dev/null 2>&1; then echo python3; else echo python; fi
}

# Windows: the ANSI code page ("Language for non-Unicode programs"), which Java reads its command line in; empty
# elsewhere.
windows_ansi_code_page() {
  [ "$(host_os)" = windows ] || return 0
  powershell -NoProfile -Command '(Get-ItemProperty HKLM:\SYSTEM\CurrentControlSet\Control\Nls\CodePage).ACP' | tr -d '\r'
}

# A folder name with letters beyond ASCII that Java can read on this machine: Cyrillic, or on a Windows whose ANSI
# code page has no Cyrillic (GitHub's runners use 1252) Latin letters with diacritics (ADR 0006).
readable_folder() {
  case "$(windows_ansi_code_page)" in
    "" | 1251 | 65001) echo "Ігри з пробілом" ;;
    1250 | 1252 | 1254 | 1257) echo "Spiele für alle" ;;
    *) echo "Games with spaces" ;;
  esac
}

# Waits until <file> has a line matching the extended regex <pattern>, or <seconds> pass.
wait_log() {
  local file="$1" pattern="$2" seconds="$3" waited=0
  until [ -f "$file" ] && grep -Eq "$pattern" "$file"; do
    [ "$waited" -ge "$seconds" ] && return 1
    sleep 1
    waited=$((waited + 1))
  done
}

# Waits until the prestarter's page has reported ready (its window is then on screen) and takes the screenshot
# <name>. Sets WINDOW_SHOWN to 1, or to 0 when <seconds> passed first: the caller fails after collecting the logs.
screenshot_window() { # <prestarter log> <name> <seconds>
  # shellcheck disable=SC2034 # read by the caller
  if wait_log "$1" "showing the window" "$3"; then WINDOW_SHOWN=1; sleep 1; else WINDOW_SHOWN=0; fi
  screenshot "$2"
}

# Waits until <file> exists (and is not empty) or <seconds> pass.
wait_file() {
  local file="$1" seconds="$2" waited=0
  while [ ! -s "$file" ]; do
    [ "$waited" -ge "$seconds" ] && return 1
    sleep 1
    waited=$((waited + 1))
  done
}

# Waits for <pid> up to <seconds>; sets EXIT_CODE. Returns 1 on timeout (the process is killed).
wait_pid() {
  local pid="$1" seconds="$2" waited=0
  while kill -0 "$pid" 2>/dev/null; do
    if [ "$waited" -ge "$seconds" ]; then
      kill "$pid" 2>/dev/null || true
      EXIT_CODE=124
      return 1
    fi
    sleep 1
    waited=$((waited + 1))
  done
  # shellcheck disable=SC2034 # read by the caller
  if wait "$pid"; then EXIT_CODE=0; else EXIT_CODE=$?; fi
}

# Waits for the prestarter <pid> up to <seconds> like wait_pid (sets EXIT_CODE; 1 on timeout), but returns 2 as soon
# as its log shows an error on screen: the window would wait for the player, so the process is stopped at once.
wait_prestarter() { # <pid> <prestarter log> <seconds>
  local pid="$1" file="$2" seconds="$3" waited=0
  while kill -0 "$pid" 2>/dev/null; do
    if [ -f "$file" ] && grep -q 'ERROR \[app::worker\]' "$file"; then
      sleep 2
      screenshot "error-$(date +%s)"
      kill "$pid" 2>/dev/null || true
      # shellcheck disable=SC2034 # read by the caller
      EXIT_CODE=125
      return 2
    fi
    if [ "$waited" -ge "$seconds" ]; then
      kill "$pid" 2>/dev/null || true
      # shellcheck disable=SC2034 # read by the caller
      EXIT_CODE=124
      return 1
    fi
    sleep 1
    waited=$((waited + 1))
  done
  # shellcheck disable=SC2034 # read by the caller
  if wait "$pid"; then EXIT_CODE=0; else EXIT_CODE=$?; fi
}

# The error a prestarter log shows on screen, if any (for the failure message).
shown_error() { # <prestarter log>
  grep -m 1 'ERROR \[app::worker\]' "$1" 2>/dev/null | sed 's/^.*ERROR \[app::worker\] //' || true
}

# Full-screen screenshot into $SMOKE_OUT/<name>.png (best effort: a missing tool only logs).
screenshot() {
  local name="$1" file="$SMOKE_OUT/$1.png"
  case "$(host_os)" in
    linux)
      if [ -n "${WAYLAND_SCREENSHOT:-}" ]; then
        (cd "$SMOKE_OUT" && WAYLAND_DISPLAY="$WAYLAND_SCREENSHOT" weston-screenshooter >/dev/null 2>&1 && mv wayland-screenshot*.png "$name.png") || log "no Wayland screenshot"
      elif command -v import >/dev/null 2>&1 && [ -n "${DISPLAY:-}" ]; then
        import -window root "$file" 2>/dev/null || log "screenshot $name failed"
      else
        log "no screenshot tool for $name"
      fi
      ;;
    macos) screencapture -x "$file" || log "screenshot $name failed" ;;
    windows)
      powershell -NoProfile -Command "Add-Type -AssemblyName System.Windows.Forms,System.Drawing; \
        \$b=[System.Windows.Forms.SystemInformation]::VirtualScreen; \$bmp=New-Object System.Drawing.Bitmap \$b.Width,\$b.Height; \
        \$g=[System.Drawing.Graphics]::FromImage(\$bmp); \$g.CopyFromScreen(\$b.Left,\$b.Top,0,0,\$bmp.Size); \
        \$bmp.Save('$(native_path "$file")')" || log "screenshot $name failed"
      ;;
  esac
  [ -s "$file" ] && log "screenshot $file"
}

# Serves <dir> on 127.0.0.1 and sets SERVE_PORT (not a subshell, so stop_servers can stop it). serve.py, not
# `python -m http.server`: that one looks up the host name, and macOS then covers the screen (and the screenshots)
# with a "find devices on local networks" dialog.
serve_dir() {
  local dir="$1" port
  port="$("$(python_bin)" -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()')"
  "$(python_bin)" "$(native_path "$(dirname "${BASH_SOURCE[0]}")/serve.py")" "$port" "$(native_path "$dir")" \
    >"$SMOKE_OUT/http-$port.log" 2>&1 &
  SERVER_PIDS+=("$!")
  local tries=0
  until curl -fsS -o /dev/null "http://127.0.0.1:$port/" 2>/dev/null; do
    tries=$((tries + 1))
    [ "$tries" -gt 50 ] && fail "the jar server did not start"
    sleep 0.2
  done
  # shellcheck disable=SC2034 # read by the caller
  SERVE_PORT="$port"
}

stop_servers() {
  local pid
  for pid in "${SERVER_PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
}

# Reads a JSON value: json_get <file> <python expression on `d`>.
json_get() {
  "$(python_bin)" -c "import json,sys; d=json.load(open(sys.argv[1], encoding='utf-8')); print($2)" "$1"
}

# Copies the prestarter's logs and state out of <store> under <label>.
collect_store() {
  local store="$1" label="$2"
  mkdir -p "$SMOKE_OUT/$label"
  cp -f "$store"/logs/*.log "$SMOKE_OUT/$label/" 2>/dev/null || true
  cp -f "$store/state.json" "$SMOKE_OUT/$label/" 2>/dev/null || true
}

# The variables an AppImage's AppRun sets (tests/fixtures/appimage/apprun-env.txt): none may reach the launcher.
APPIMAGE_VARS="APPDIR APPIMAGE ARGV0 OWD GDK_BACKEND GTK_THEME GTK_PATH GTK_IM_MODULE_FILE GTK_DATA_PREFIX GTK_EXE_PREFIX GDK_PIXBUF_MODULE_FILE GIO_EXTRA_MODULES GSETTINGS_SCHEMA_DIR PYTHONHOME PYTHONDONTWRITEBYTECODE PYTHONPATH PERLLIB QT_PLUGIN_PATH GI_TYPELIB_PATH GST_PLUGIN_SYSTEM_PATH GST_PLUGIN_SYSTEM_PATH_1_0"

assert_clean_env() { # <fx json> <mount or extraction dir>
  local record="$1" appdir="$2" leaked
  leaked="$("$(python_bin)" - "$record" "$appdir" "$APPIMAGE_VARS" <<'PY'
import json, sys
record, appdir, names = sys.argv[1], sys.argv[2], sys.argv[3].split()
env = json.load(open(record, encoding="utf-8"))["env"]
bad = [n for n in names if n in env]
bad += [f"{k}={v}" for k, v in env.items() if appdir and appdir in v]
print(" ".join(bad))
PY
)"
  [ -z "$leaked" ] || fail "AppImage variables reached the launcher: $leaked"
  log "the launcher environment is free of the AppImage variables"
}
