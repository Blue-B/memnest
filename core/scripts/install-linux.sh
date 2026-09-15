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
  MEMNEST_HOST  preserve installed value; default 127.0.0.1 for new installs
  MEMNEST_PORT  preserve installed value; default 3111 for new installs
Explicit values update supported endpoint assignments; other unit settings are retained.
Custom commands, drop-ins or external environment files require a manual upgrade.
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
# shellcheck source=linux-service-config.sh
source "$ROOT/scripts/linux-service-config.sh"
resolve_linux_service_config

if ! command -v systemctl >/dev/null 2>&1; then
  echo "systemd is required for this installer" >&2
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

STAGING="$(mktemp -d)"
trap 'rm -f "$STAGING/memnest.service" "$STAGING/memnest-watch.service"; rmdir "$STAGING"' EXIT
PRIV=()
[ "$MODE" != system ] || PRIV=(sudo)

render_service() {
  local template="$1" dest="$2" tmp backup mode=0644
  tmp="$STAGING/$(basename "$dest")"
  # Upgrade the binary, not the user's unit. Only endpoint lines may change.
  [ ! -e "$dest" ] || template="$dest"
  sed \
    -e "s/^Environment=MEMNEST_HOST=.*/Environment=MEMNEST_HOST=${HOST}/" \
    -e "s/^Environment=MEMNEST_PORT=.*/Environment=MEMNEST_PORT=${PORT}/" \
    "$template" >"$tmp"
  if [ -e "$dest" ]; then
    cmp -s "$tmp" "$dest" && return 0
    mode="$(stat -c '%a' "$dest")"
    backup="$("${PRIV[@]}" mktemp "$dest.backup.XXXXXX")"
    "${PRIV[@]}" install -m 0600 "$dest" "$backup"
    printf 'Service backup: %s\nRestore: ' "$backup"
    printf '%q ' "${PRIV[@]}" install -m "$mode" "$backup" "$dest"
    printf '\n'
  fi
  "${PRIV[@]}" install -m "$mode" "$tmp" "$dest"
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
  sudo install -d /usr/local/bin /var/lib/memnest /usr/local/share/memnest/scripts
  if ! [ "$BIN_SRC" -ef /usr/local/bin/memnest ]; then
    sudo install -m 0755 "$BIN_SRC" /usr/local/bin/memnest
  fi
  sudo install -m 0755 "$ROOT/scripts/setup-clients.py" "$ROOT/scripts/uninstall-linux.sh" /usr/local/share/memnest/scripts/
  render_service "$ROOT/packaging/systemd/memnest.service" "$SERVICE_DIR/memnest.service"
  sudo systemctl daemon-reload
  sudo systemctl enable memnest.service
  sudo systemctl restart memnest.service
  sudo systemctl status memnest.service --no-pager -l
  echo "System installs do not run a root transcript watcher; run memnest watch as the desktop user if capture is wanted."
else
  install -d "$HOME/.local/bin" "$HOME/.local/share/memnest/scripts" "$SERVICE_DIR" "$HOME/.memnest"
  if ! [ "$BIN_SRC" -ef "$HOME/.local/bin/memnest" ]; then
    install -m 0755 "$BIN_SRC" "$HOME/.local/bin/memnest"
  fi
  install -m 0755 "$ROOT/scripts/setup-clients.py" "$ROOT/scripts/uninstall-linux.sh" "$HOME/.local/share/memnest/scripts/"
  render_service "$ROOT/packaging/systemd/memnest-user.service" "$SERVICE_DIR/memnest.service"
  render_service "$ROOT/packaging/systemd/memnest-watch-user.service" "$SERVICE_DIR/memnest-watch.service"
  systemctl --user daemon-reload
  systemctl --user enable memnest.service memnest-watch.service
  systemctl --user restart memnest.service memnest-watch.service
  systemctl --user status memnest.service memnest-watch.service --no-pager -l
fi

wait_for_health
echo "Memnest MCP: http://${HOST}:${PORT}/mcp"
