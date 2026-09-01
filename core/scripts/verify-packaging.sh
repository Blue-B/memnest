#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
USER_UNIT="$ROOT/packaging/systemd/memnest-user.service"

fail() {
  printf 'packaging contract failed: %s\n' "$1" >&2
  exit 1
}

for after in $(sed -n 's/^After=//p' "$USER_UNIT"); do
  for wanted in $(sed -n 's/^WantedBy=//p' "$USER_UNIT"); do
    [ "$after" != "$wanted" ] || fail "user service cannot be ordered after its own install target: $after"
  done
done

if grep -E -i -n 'dashboard|Windows release archive|Memnest API and MCP:|Memnest is available at http' \
  "$ROOT/packaging/windows/memnest-service.xml" \
  "$ROOT/scripts/install.sh" \
  "$ROOT/scripts/install-linux.sh" \
  "$ROOT/scripts/install-windows.ps1" \
  "$ROOT/scripts/install-wsl.ps1"; then
  fail "install paths still advertise a removed or unpublished surface"
fi

REPO_ROOT="$(cd "$ROOT/.." && pwd)"
CONTRIB_INSTALL="$REPO_ROOT/pi-extension/contrib/install.sh"
CONTRIB_UNIT="$REPO_ROOT/pi-extension/contrib/memnest.service"
if [ -f "$CONTRIB_INSTALL" ] && [ -f "$CONTRIB_UNIT" ] &&
  grep -E -i -n 'After=default.target|dashboard|not published to npm yet' \
    "$CONTRIB_INSTALL" "$CONTRIB_UNIT"; then
  fail "legacy contrib installers still advertise a broken or removed surface"
fi

printf 'verify_packaging_ok\n'
