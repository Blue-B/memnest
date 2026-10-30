#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
for script in install-macos.sh uninstall-macos.sh validate-installed-macos.sh setup.sh; do bash -n "$ROOT/scripts/$script"; done
python3 -m py_compile "$ROOT/scripts/setup-clients.py"
for plist in "$ROOT"/packaging/launchd/*.plist; do
  if command -v plutil >/dev/null 2>&1; then plutil -lint "$plist" >/dev/null; fi
  grep -q '<key>Label</key>' "$plist"
  grep -q '<key>ProgramArguments</key>' "$plist"
  grep -q '__MEMNEST_BIN__' "$plist"
  grep -q '__MEMNEST_DATA__' "$plist"
done
# A published macOS archive must stay gated by the native lifecycle job.
ci_workflow="$ROOT/../.github/workflows/core-ci.yml"
if ! grep -q 'uninstall-macos.sh --remove-data' "$ci_workflow"; then
  echo "core-ci.yml no longer runs the macOS install/removal lifecycle" >&2
  exit 1
fi
echo verify_macos_packaging_ok
