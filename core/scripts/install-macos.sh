#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_SRC="${BIN_SRC:-}"
PORT="${MEMNEST_PORT:-3111}"
DATA_DIR="${MEMNEST_DATA_DIR:-$HOME/.memnest}"
BIN_DIR="${MEMNEST_BIN_DIR:-$HOME/.local/bin}"
AGENTS="$HOME/Library/LaunchAgents"
LOGS="$HOME/Library/Logs/Memnest"
BACKUPS="$DATA_DIR/setup-backups"
SHARE="$HOME/.local/share/memnest/scripts"

validate_port() {
  case "$PORT" in ''|*[!0-9]*) echo "MEMNEST_PORT must be an integer from 1 to 65535" >&2; exit 2 ;; esac
  [ "${#PORT}" -le 5 ] && [ "$((10#$PORT))" -ge 1 ] && [ "$((10#$PORT))" -le 65535 ] || {
    echo "MEMNEST_PORT must be an integer from 1 to 65535" >&2; exit 2;
  }
}

usage() { echo "Usage: scripts/install-macos.sh [--bin /path/to/memnest]"; }
while [ "$#" -gt 0 ]; do
  case "$1" in
    --bin) BIN_SRC="${2:-}"; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown argument: $1" >&2; usage; exit 2 ;;
  esac
  shift
done
validate_port
[ "$(uname -s)" = Darwin ] || { echo "install-macos.sh requires macOS" >&2; exit 1; }
if [ -z "$BIN_SRC" ]; then
  for candidate in ./memnest ./target/release/memnest "$ROOT/memnest" "$ROOT/target/release/memnest"; do
    if [ -x "$candidate" ]; then BIN_SRC="$candidate"; break; fi
  done
fi
[ -n "$BIN_SRC" ] && [ -x "$BIN_SRC" ] || { echo "memnest binary not found; pass --bin" >&2; exit 1; }
command -v launchctl >/dev/null || { echo "launchctl is required" >&2; exit 1; }
command -v plutil >/dev/null || { echo "plutil is required" >&2; exit 1; }
command -v python3 >/dev/null || { echo "install-macos.sh requires python3" >&2; exit 1; }

mkdir -p "$BIN_DIR" "$AGENTS" "$LOGS" "$DATA_DIR" "$BACKUPS" "$SHARE"
install -m 0755 "$ROOT/scripts/setup-clients.py" "$ROOT/scripts/uninstall-macos.sh" "$ROOT/scripts/validate-installed-macos.sh" "$SHARE/"
if ! [ "$BIN_SRC" -ef "$BIN_DIR/memnest" ]; then
  install -m 0755 "$BIN_SRC" "$BIN_DIR/memnest"
fi
timestamp="$(date -u +%Y%m%dT%H%M%SZ)"
render_plist() {
  src="$1"; dest="$2"
  tmp="$(mktemp "${TMPDIR:-/tmp}/memnest-plist.XXXXXX")"
  python3 - "$src" "$tmp" "$BIN_DIR/memnest" "$DATA_DIR" "$LOGS" "$PORT" <<'PY'
import pathlib, sys
src, dest, binary, data, logs, port = sys.argv[1:]
text = pathlib.Path(src).read_text()
for key, value in {"__MEMNEST_BIN__": binary, "__MEMNEST_DATA__": data,
                   "__MEMNEST_LOGS__": logs, "__MEMNEST_PORT__": port}.items():
    text = text.replace(key, value.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;"))
pathlib.Path(dest).write_text(text)
PY
  plutil -lint "$tmp" >/dev/null
  if [ -f "$dest" ] && ! cmp -s "$tmp" "$dest"; then
    cp -p "$dest" "$BACKUPS/$(basename "$dest").$timestamp.bak"
  fi
  install -m 0644 "$tmp" "$dest"
  rm -f "$tmp"
}

uid="$(id -u)"
for label in io.memnest.watch io.memnest.service; do
  launchctl bootout "gui/$uid/$label" 2>/dev/null || true
done
render_plist "$ROOT/packaging/launchd/io.memnest.service.plist" "$AGENTS/io.memnest.service.plist"
render_plist "$ROOT/packaging/launchd/io.memnest.watch.plist" "$AGENTS/io.memnest.watch.plist"
launchctl bootstrap "gui/$uid" "$AGENTS/io.memnest.service.plist"
launchctl bootstrap "gui/$uid" "$AGENTS/io.memnest.watch.plist"
launchctl enable "gui/$uid/io.memnest.service"
launchctl enable "gui/$uid/io.memnest.watch"
launchctl kickstart -k "gui/$uid/io.memnest.service"
launchctl kickstart -k "gui/$uid/io.memnest.watch"

python3 - "$PORT" <<'PY'
import sys, time, urllib.request
url = "http://127.0.0.1:%s/health" % sys.argv[1]
for attempt in range(30):
    try:
        with urllib.request.urlopen(url, timeout=2) as response:
            if 200 <= response.status < 300:
                print("Health check passed: " + url)
                break
    except Exception:
        if attempt == 29:
            raise SystemExit("service did not answer health check at " + url)
        time.sleep(1)
PY
echo "Installed launchd agents io.memnest.service and io.memnest.watch."
