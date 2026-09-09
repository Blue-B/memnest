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
grep -q 'x86_64-apple-darwin' "$ROOT/../.github/workflows/core-release.yml"
grep -q 'aarch64-apple-darwin' "$ROOT/../.github/workflows/core-release.yml"
echo verify_macos_packaging_ok
