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
# The scripts stay source-checked, but v0.3.0 must not publish unverified
# macOS archives from the tag-triggered workflow.
release_workflow="$ROOT/../.github/workflows/core-release.yml"
if [ ! -f "$release_workflow" ]; then
  echo "missing core release workflow: $release_workflow" >&2
  exit 1
fi
if grep -q 'apple-darwin' "$release_workflow"; then
  echo "macOS release target found before native lifecycle validation" >&2
  exit 1
fi
echo verify_macos_packaging_ok
