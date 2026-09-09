#!/usr/bin/env bash
set -euo pipefail
PORT="${MEMNEST_PORT:-3111}"
DATA_DIR="${MEMNEST_DATA_DIR:-$HOME/.memnest}"
BIN="${BIN:-$HOME/.local/bin/memnest}"
[ "$(uname -s)" = Darwin ] || { echo "validate-installed-macos.sh requires macOS" >&2; exit 1; }
uid="$(id -u)"
launchctl print "gui/$uid/io.memnest.service" >/dev/null
launchctl print "gui/$uid/io.memnest.watch" >/dev/null
"$BIN" --data-dir "$DATA_DIR" --doctor
launchctl kickstart -k "gui/$uid/io.memnest.service"
python3 - "$PORT" <<'PY'
import sys, time, urllib.request
url = "http://127.0.0.1:%s/health" % sys.argv[1]
for attempt in range(30):
    try:
        with urllib.request.urlopen(url, timeout=2) as response:
            if 200 <= response.status < 300:
                print("validate_installed_macos_ok")
                break
    except Exception:
        if attempt == 29: raise SystemExit("health check failed: " + url)
        time.sleep(1)
PY
