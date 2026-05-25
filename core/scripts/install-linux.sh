#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
MODE="${MODE:-user}"
HOST="${PALIMPSEST_HOST:-127.0.0.1}"
PORT="${PALIMPSEST_PORT:-3111}"
BIN_SRC="${BIN_SRC:-}"

usage() {
  cat <<'EOF'
Usage: scripts/install-linux.sh [--user|--system] [--bin /path/to/palimpsest]

Installs Palimpsest as a systemd service on Linux.

Environment:
  PALIMPSEST_HOST  default 127.0.0.1
  PALIMPSEST_PORT  default 3111
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
  echo "install-linux.sh only supports local service binds. Use 127.0.0.1 for packaged installs; configure remote access manually with PALIMPSEST_TOKEN and a reviewed network policy." >&2
  exit 1
fi

if [ -z "$BIN_SRC" ]; then
  if [ -x "./palimpsest" ]; then
    BIN_SRC="./palimpsest"
  elif [ -x "./target/release/palimpsest" ]; then
    BIN_SRC="./target/release/palimpsest"
  elif [ -x "$ROOT/palimpsest" ]; then
    BIN_SRC="$ROOT/palimpsest"
  elif [ -x "$ROOT/target/release/palimpsest" ]; then
    BIN_SRC="$ROOT/target/release/palimpsest"
  elif command -v palimpsest >/dev/null 2>&1; then
    BIN_SRC="$(command -v palimpsest)"
  else
    echo "palimpsest binary not found. Extract a release archive, build first, or pass --bin /path/to/palimpsest" >&2
    exit 1
  fi
fi

patch_service_env() {
  local service_file="$1"
  local runner="${2:-}"
  if [ -n "$runner" ]; then
    $runner sed -i \
      -e "s/^Environment=PALIMPSEST_HOST=.*/Environment=PALIMPSEST_HOST=${HOST}/" \
      -e "s/^Environment=PALIMPSEST_PORT=.*/Environment=PALIMPSEST_PORT=${PORT}/" \
      "$service_file"
  else
    sed -i \
      -e "s/^Environment=PALIMPSEST_HOST=.*/Environment=PALIMPSEST_HOST=${HOST}/" \
      -e "s/^Environment=PALIMPSEST_PORT=.*/Environment=PALIMPSEST_PORT=${PORT}/" \
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
  sudo install -m 0755 "$BIN_SRC" /usr/local/bin/palimpsest
  sudo mkdir -p /var/lib/palimpsest /usr/local/share/palimpsest/static
  sudo cp -R "$ROOT/static/." /usr/local/share/palimpsest/static/
  sudo find /usr/local/share/palimpsest/static -type f -exec chmod 0644 {} +
  sudo install -m 0644 "$ROOT/packaging/systemd/palimpsest.service" /etc/systemd/system/palimpsest.service
  patch_service_env /etc/systemd/system/palimpsest.service sudo
  sudo systemctl daemon-reload
  sudo systemctl enable --now palimpsest.service
  sudo systemctl status palimpsest.service --no-pager -l
else
  install -d "$HOME/.local/bin" "$HOME/.config/systemd/user" "$HOME/.palimpsest" "$HOME/.local/share/palimpsest/static"
  install -m 0755 "$BIN_SRC" "$HOME/.local/bin/palimpsest"
  cp -R "$ROOT/static/." "$HOME/.local/share/palimpsest/static/"
  find "$HOME/.local/share/palimpsest/static" -type f -exec chmod 0644 {} +
  install -m 0644 "$ROOT/packaging/systemd/palimpsest-user.service" "$HOME/.config/systemd/user/palimpsest.service"
  patch_service_env "$HOME/.config/systemd/user/palimpsest.service"
  systemctl --user daemon-reload
  systemctl --user enable --now palimpsest.service
  systemctl --user status palimpsest.service --no-pager -l
fi

wait_for_health
echo "Palimpsest is available at http://${HOST}:${PORT}/"
