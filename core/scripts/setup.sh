#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_SRC="${BIN_SRC:-}"
PORT="${MEMNEST_PORT:-3111}"
MODE="${MODE:-user}"
CONFIGURE_CLIENTS=1
AUTOCONTEXT=0
VERIFY=1
validate_port() {
  case "$PORT" in ''|*[!0-9]*) echo "MEMNEST_PORT must be an integer from 1 to 65535" >&2; exit 2 ;; esac
  [ "${#PORT}" -le 5 ] && [ "$((10#$PORT))" -ge 1 ] && [ "$((10#$PORT))" -le 65535 ] || {
    echo "MEMNEST_PORT must be an integer from 1 to 65535" >&2; exit 2;
  }
}
usage() {
  cat <<'EOF'
Usage: scripts/setup.sh [--bin /path/to/memnest] [--user|--system] [--no-clients] [--autocontext] [--no-verify]

Installs and starts Memnest, merges configuration for detected clients, starts
conversation watch, and verifies a scratch remember/search round trip.
Automatic recall hooks are opt-in (--autocontext); existing hooks are preserved.
EOF
}
while [ "$#" -gt 0 ]; do
  case "$1" in
    --bin) BIN_SRC="${2:-}"; shift ;;
    --user) MODE=user ;;
    --system) MODE=system ;;
    --no-clients) CONFIGURE_CLIENTS=0 ;;
    --autocontext) AUTOCONTEXT=1 ;;
    --no-verify) VERIFY=0 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown argument: $1" >&2; usage; exit 2 ;;
  esac
  shift
done
OS="$(uname -s)"
if [ "$OS" = Linux ]; then
  # Resolve before installation so client configuration and probes use the same port.
  # shellcheck source=linux-service-config.sh
  source "$ROOT/scripts/linux-service-config.sh"
  resolve_linux_service_config
fi
validate_port
BASE_URL="http://${MEMNEST_HOST:-127.0.0.1}:$PORT"
if [ "$CONFIGURE_CLIENTS" = 1 ]; then
  python3 -c 'import tomllib' 2>/dev/null || {
    echo "default setup requires Python 3.11 or newer for safe client-config validation" >&2
    exit 1
  }
elif [ "$VERIFY" = 1 ] || [ "$OS" = Darwin ]; then
  command -v python3 >/dev/null 2>&1 || { echo "setup requires python3" >&2; exit 1; }
fi
case "$OS" in
  Linux)
    args=("--$MODE")
    [ -z "$BIN_SRC" ] || args+=(--bin "$BIN_SRC")
    "$ROOT/scripts/install-linux.sh" "${args[@]}"
    if [ "$MODE" = user ]; then
      BIN="$HOME/.local/bin/memnest"
      INSTALLED_SCRIPTS="$HOME/.local/share/memnest/scripts"
    else
      BIN=/usr/local/bin/memnest
      INSTALLED_SCRIPTS=/usr/local/share/memnest/scripts
    fi
    ;;
  Darwin)
    [ "$MODE" = user ] || { echo "macOS setup supports only --user" >&2; exit 1; }
    args=()
    [ -z "$BIN_SRC" ] || args+=(--bin "$BIN_SRC")
    "$ROOT/scripts/install-macos.sh" "${args[@]}"
    BIN="${MEMNEST_BIN_DIR:-$HOME/.local/bin}/memnest"
    INSTALLED_SCRIPTS="$HOME/.local/share/memnest/scripts"
    ;;
  *) echo "unsupported OS: $OS" >&2; exit 1 ;;
esac

if [ "$CONFIGURE_CLIENTS" = 1 ]; then
  client_args=(--bin "$BIN" --url "$BASE_URL")
  [ "$AUTOCONTEXT" = 0 ] || client_args+=(--autocontext)
  python3 "$ROOT/scripts/setup-clients.py" "${client_args[@]}"
fi
if [ "$VERIFY" = 1 ]; then
  python3 - "$BASE_URL" <<'PY'
import json, sys, time, urllib.request
base = sys.argv[1]
marker = "memnest-setup-roundtrip-%d" % time.time_ns()
def post(path, body):
    request = urllib.request.Request(base + path, json.dumps(body).encode(), {"content-type": "application/json"})
    with urllib.request.urlopen(request, timeout=300) as response:
        return json.load(response)
created = post("/add", {"text": marker, "project": "memnest-setup-check"})
found = post("/search", {"query": marker, "project": "memnest-setup-check", "n_results": 5})
if marker not in json.dumps(found):
    raise SystemExit("scratch remember/search verification failed")
identifier = created.get("id")
if identifier:
    post("/delete", {"ids": [identifier]})
print("setup_roundtrip_ok")
PY
fi
if [ "$OS" = Linux ]; then
  UNINSTALL="\"$INSTALLED_SCRIPTS/uninstall-linux.sh\" --$MODE"
else
  UNINSTALL="\"$INSTALLED_SCRIPTS/uninstall-macos.sh\""
fi
cat <<EOF
Setup complete. MCP endpoint: $BASE_URL/mcp
Rollback client changes: use the exact restore command printed above when setup changed a client file.
Uninstall: $UNINSTALL
Data is retained unless --remove-data is explicitly passed to the uninstaller.
EOF
