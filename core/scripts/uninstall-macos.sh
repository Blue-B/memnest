#!/usr/bin/env bash
set -euo pipefail
REMOVE_DATA=0
usage() { echo "Usage: scripts/uninstall-macos.sh [--remove-data]"; }
while [ "$#" -gt 0 ]; do
  case "$1" in --remove-data) REMOVE_DATA=1 ;; -h|--help) usage; exit 0 ;; *) echo "unknown argument: $1" >&2; exit 2 ;; esac
  shift
done
[ "$(uname -s)" = Darwin ] || { echo "uninstall-macos.sh requires macOS" >&2; exit 1; }
DATA_DIR="${MEMNEST_DATA_DIR:-$HOME/.memnest}"
if [ "$REMOVE_DATA" = 1 ]; then
  canonical_data="$(python3 - "$DATA_DIR" <<'PY'
import os, sys
print(os.path.realpath(os.path.expanduser(sys.argv[1])))
PY
)"
  case "$canonical_data" in
    ""|/|"$HOME"|/home|/Users|/tmp|/var|/usr|/usr/local)
      echo "refusing to recursively remove unsafe MEMNEST_DATA_DIR: $canonical_data" >&2
      exit 2
      ;;
  esac
  case "$(basename "$canonical_data" | tr '[:upper:]' '[:lower:]')" in
    *memnest*) ;;
    *) echo "refusing to remove a data directory not named for Memnest: $canonical_data" >&2; exit 2 ;;
  esac
fi
uid="$(id -u)"
for label in io.memnest.watch io.memnest.service; do launchctl bootout "gui/$uid/$label" 2>/dev/null || true; done
rm -f "$HOME/Library/LaunchAgents/io.memnest.service.plist" "$HOME/Library/LaunchAgents/io.memnest.watch.plist"
rm -f "${MEMNEST_BIN_DIR:-$HOME/.local/bin}/memnest"
rm -rf -- "$HOME/.local/share/memnest"
if [ "$REMOVE_DATA" = 1 ]; then
  rm -rf -- "$canonical_data" "$HOME/Library/Logs/Memnest"
  echo "Memnest launch agents, binary, data, logs, and client-config backups removed. Client entries were left unchanged."
else
  echo "Memnest launch agents and binary removed. Data and timestamped client-config backups were retained."
  echo "Each backup directory contains setup-clients.py and its exact restore manifest."
fi
