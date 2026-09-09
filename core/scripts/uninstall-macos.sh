#!/usr/bin/env bash
set -euo pipefail
REMOVE_DATA=0
usage() { echo "Usage: scripts/uninstall-macos.sh [--remove-data]"; }
while [ "$#" -gt 0 ]; do
  case "$1" in --remove-data) REMOVE_DATA=1 ;; -h|--help) usage; exit 0 ;; *) echo "unknown argument: $1" >&2; exit 2 ;; esac
  shift
done
[ "$(uname -s)" = Darwin ] || { echo "uninstall-macos.sh requires macOS" >&2; exit 1; }
uid="$(id -u)"
for label in io.memnest.watch io.memnest.service; do launchctl bootout "gui/$uid/$label" 2>/dev/null || true; done
rm -f "$HOME/Library/LaunchAgents/io.memnest.service.plist" "$HOME/Library/LaunchAgents/io.memnest.watch.plist"
rm -f "${MEMNEST_BIN_DIR:-$HOME/.local/bin}/memnest"
rm -rf "$HOME/.local/share/memnest"
if [ "$REMOVE_DATA" = 1 ]; then
  rm -rf "${MEMNEST_DATA_DIR:-$HOME/.memnest}" "$HOME/Library/Logs/Memnest"
fi
echo "Memnest launch agents and binary removed. Client config backups are retained; restore them with setup-clients.py --restore <manifest>."
