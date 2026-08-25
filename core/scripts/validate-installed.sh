#!/usr/bin/env bash
set -euo pipefail

MODE="${MODE:-user}"
HOST="${MEMNEST_HOST:-127.0.0.1}"
PORT="${MEMNEST_PORT:-3111}"
SERVICE="${SERVICE:-memnest.service}"
DATA_DIR="${MEMNEST_DATA_DIR:-}"
BIN="${BIN:-}"

usage() {
  cat <<'EOF'
Usage: scripts/validate-installed.sh [--user|--system]

Validates an already installed Linux Memnest service by checking service
state, health, doctor diagnostics, restart recovery, and health again.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --user) MODE="user" ;;
    --system) MODE="system" ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown argument: $1" >&2; usage; exit 2 ;;
  esac
  shift
done

wait_for_health() {
  for _ in $(seq 1 30); do
    if curl -fsS "http://${HOST}:${PORT}/health" >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  echo "health check failed at http://${HOST}:${PORT}/health" >&2
  return 1
}

if ! command -v curl >/dev/null 2>&1; then
  echo "curl is required for installed validation" >&2
  exit 1
fi

if [ "$MODE" = "system" ]; then
  DATA_DIR="${DATA_DIR:-/var/lib/memnest}"
  BIN="${BIN:-/usr/local/bin/memnest}"
  systemctl is-enabled "$SERVICE"
  systemctl is-active "$SERVICE"
  wait_for_health
  "$BIN" --data-dir "$DATA_DIR" --doctor
  sudo systemctl restart "$SERVICE"
  systemctl is-active "$SERVICE"
else
  DATA_DIR="${DATA_DIR:-$HOME/.memnest}"
  BIN="${BIN:-$HOME/.local/bin/memnest}"
  systemctl --user is-enabled "$SERVICE"
  systemctl --user is-active "$SERVICE"
  wait_for_health
  "$BIN" --data-dir "$DATA_DIR" --doctor
  systemctl --user restart "$SERVICE"
  systemctl --user is-active "$SERVICE"
fi

wait_for_health
echo "validate_installed_linux_ok"
