#!/usr/bin/env bash
set -euo pipefail

MODE="${MODE:-user}"
REMOVE_DATA="${REMOVE_DATA:-0}"

usage() {
  cat <<'EOF'
Usage: scripts/uninstall-linux.sh [--user|--system] [--remove-data]

Stops and removes the Palimpsest systemd service. Data is kept unless
--remove-data is passed.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --user) MODE="user" ;;
    --system) MODE="system" ;;
    --remove-data) REMOVE_DATA="1" ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown argument: $1" >&2; usage; exit 2 ;;
  esac
  shift
done

if [ "$MODE" = "system" ]; then
  sudo systemctl disable --now palimpsest.service 2>/dev/null || true
  sudo rm -f /etc/systemd/system/palimpsest.service
  sudo systemctl daemon-reload
  sudo rm -f /usr/local/bin/palimpsest
  sudo rm -rf /usr/local/share/palimpsest
  if [ "$REMOVE_DATA" = "1" ]; then
    sudo rm -rf /var/lib/palimpsest
  fi
else
  systemctl --user disable --now palimpsest.service 2>/dev/null || true
  rm -f "$HOME/.config/systemd/user/palimpsest.service"
  systemctl --user daemon-reload
  rm -f "$HOME/.local/bin/palimpsest"
  rm -rf "$HOME/.local/share/palimpsest"
  if [ "$REMOVE_DATA" = "1" ]; then
    rm -rf "$HOME/.palimpsest"
  fi
fi

echo "Palimpsest uninstalled."
