#!/usr/bin/env bash
# Stage what one release archive ships: the binary, the install, uninstall,
# validation, setup and support scripts that target uses, its service
# templates, and the license files. Development and test scripts stay out.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
[ "$#" -eq 3 ] || { echo "Usage: scripts/package-release.sh <target> <binary> <stage-dir>" >&2; exit 2; }
TARGET="$1"
BINARY="$2"
STAGE="$3"

case "$TARGET" in
  *-linux-*)
    scripts=(setup.sh setup-clients.py install-linux.sh linux-service-config.sh uninstall-linux.sh
      preflight-linux.sh validate-installed.sh support-bundle.sh)
    packaging=systemd
    ;;
  *-apple-darwin)
    scripts=(setup.sh setup-clients.py install-macos.sh uninstall-macos.sh validate-installed-macos.sh)
    packaging=launchd
    ;;
  *-windows-*)
    scripts=(install-windows.ps1 uninstall-windows.ps1 preflight-windows.ps1
      validate-installed-windows.ps1 support-bundle-windows.ps1 setup-clients.py)
    packaging=""
    ;;
  *) echo "unknown target: $TARGET" >&2; exit 2 ;;
esac

mkdir -p "$STAGE/scripts"
cp "$BINARY" "$STAGE/"
for script in "${scripts[@]}"; do
  cp "$ROOT/scripts/$script" "$STAGE/scripts/"
done
if [ -n "$packaging" ]; then
  mkdir -p "$STAGE/packaging"
  cp -r "$ROOT/packaging/$packaging" "$STAGE/packaging/"
fi
cp "$ROOT/LICENSE" "$ROOT/THIRD_PARTY_NOTICES.md" "$ROOT/../README.md" "$STAGE/"
