#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${MODE:-user}"
HOST="${MEMNEST_HOST:-127.0.0.1}"
PORT="${MEMNEST_PORT:-3111}"
BIN_SRC="${BIN_SRC:-}"

usage() {
  cat <<'EOF'
Usage: scripts/install-linux.sh [--user|--system] [--bin /path/to/memnest]

Installs Memnest as a systemd service on Linux.

Environment:
  MEMNEST_HOST  default 127.0.0.1
  MEMNEST_PORT  default 3111
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --user) MODE="user" ;;
    --system) MODE="system" ;;
    --bin) BIN_SRC="${2:-}"; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown argument: $1" >&2; usage; exit 2 ;;
  esac
  shift
done

if ! command -v systemctl >/dev/null 2>&1; then
  echo "systemd is required for this installer" >&2
  exit 1
fi

is_local_host() {
  case "$1" in
    127.0.0.1|localhost|::1) return 0 ;;
    *) return 1 ;;
  esac
}

if ! is_local_host "$HOST"; then
  echo "install-linux.sh only supports local service binds. Use 127.0.0.1 for packaged installs; configure remote access manually with MEMNEST_TOKEN and a reviewed network policy." >&2
  exit 1
fi

if [ -z "$BIN_SRC" ]; then
  if [ -x "./memnest" ]; then
    BIN_SRC="./memnest"
  elif [ -x "./target/release/memnest" ]; then
    BIN_SRC="./target/release/memnest"
  elif [ -x "$ROOT/memnest" ]; then
    BIN_SRC="$ROOT/memnest"
  elif [ -x "$ROOT/target/release/memnest" ]; then
    BIN_SRC="$ROOT/target/release/memnest"
  elif command -v memnest >/dev/null 2>&1; then
    BIN_SRC="$(command -v memnest)"
  else
    echo "memnest binary not found. Extract a release archive, build first, or pass --bin /path/to/memnest" >&2
    exit 1
  fi
fi

patch_service_env() {
  local service_file="$1"
  local runner="${2:-}"
  if [ -n "$runner" ]; then
    $runner sed -i \
      -e "s/^Environment=MEMNEST_HOST=.*/Environment=MEMNEST_HOST=${HOST}/" \
      -e "s/^Environment=MEMNEST_PORT=.*/Environment=MEMNEST_PORT=${PORT}/" \
      "$service_file"
  else
    sed -i \
      -e "s/^Environment=MEMNEST_HOST=.*/Environment=MEMNEST_HOST=${HOST}/" \
      -e "s/^Environment=MEMNEST_PORT=.*/Environment=MEMNEST_PORT=${PORT}/" \
      "$service_file"
  fi
}

wait_for_health() {
  if ! command -v curl >/dev/null 2>&1; then
    echo "curl not found; skipping health probe"
    return 0
  fi

  for _ in $(seq 1 30); do
    if curl -fsS "http://${HOST}:${PORT}/health" >/dev/null 2>&1; then
      echo "Health check passed: http://${HOST}:${PORT}/health"
      return 0
    fi
    sleep 1
  done

  echo "service did not answer health check at http://${HOST}:${PORT}/health" >&2
  return 1
}

if [ "$MODE" = "system" ]; then
  sudo install -m 0755 "$BIN_SRC" /usr/local/bin/memnest
  sudo mkdir -p /var/lib/memnest
  sudo install -m 0644 "$ROOT/packaging/systemd/memnest.service" /etc/systemd/system/memnest.service
  patch_service_env /etc/systemd/system/memnest.service sudo
  sudo systemctl daemon-reload
  sudo systemctl enable --now memnest.service
  sudo systemctl status memnest.service --no-pager -l
else
  install -d "$HOME/.local/bin" "$HOME/.config/systemd/user" "$HOME/.memnest"
  install -m 0755 "$BIN_SRC" "$HOME/.local/bin/memnest"
  install -m 0644 "$ROOT/packaging/systemd/memnest-user.service" "$HOME/.config/systemd/user/memnest.service"
  patch_service_env "$HOME/.config/systemd/user/memnest.service"
  systemctl --user daemon-reload
  systemctl --user enable --now memnest.service
  systemctl --user status memnest.service --no-pager -l
fi

wait_for_health
echo "Memnest MCP: http://${HOST}:${PORT}/mcp"
