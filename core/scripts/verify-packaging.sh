#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
USER_UNIT="$ROOT/packaging/systemd/memnest-user.service"
WATCH_UNIT="$ROOT/packaging/systemd/memnest-watch-user.service"

fail() {
  printf 'packaging contract failed: %s\n' "$1" >&2
  exit 1
}

for after in $(sed -n 's/^After=//p' "$USER_UNIT"); do
  for wanted in $(sed -n 's/^WantedBy=//p' "$USER_UNIT"); do
    [ "$after" != "$wanted" ] || fail "user service cannot be ordered after its own install target: $after"
  done
done

grep -q 'After=memnest.service' "$WATCH_UNIT" || fail "watch service must start after the server"
grep -q 'Requires=memnest.service' "$WATCH_UNIT" || fail "watch service must require the server"
for script in setup.sh setup-clients.py linux-service-config.sh install-macos.sh uninstall-macos.sh validate-installed-macos.sh; do
  [ -f "$ROOT/scripts/$script" ] || fail "missing setup/macOS script: $script"
done

if grep -E -i -n 'dashboard|Memnest API and MCP:|Memnest is available at http' \
  "$ROOT/scripts/install.sh" \
  "$ROOT/scripts/install.ps1" \
  "$ROOT/scripts/install-linux.sh" \
  "$ROOT/scripts/install-windows.ps1" \
  "$ROOT/scripts/install-wsl.ps1"; then
  fail "install paths still advertise a removed or unpublished surface"
fi

# Each release archive ships only what its OS installs with, and everything
# those scripts reach for through $ROOT is inside the archive.
STAGE_ROOT="$(mktemp -d)"
trap 'rm -rf "$STAGE_ROOT"' EXIT
: > "$STAGE_ROOT/memnest"
for target in x86_64-unknown-linux-gnu aarch64-apple-darwin x86_64-pc-windows-msvc; do
  stage="$STAGE_ROOT/$target"
  bash "$ROOT/scripts/package-release.sh" "$target" "$STAGE_ROOT/memnest" "$stage"
  for file in LICENSE THIRD_PARTY_NOTICES.md README.md; do
    [ -f "$stage/$file" ] || fail "$target archive is missing $file"
  done
  # setup.sh serves Linux and macOS, so it names the other system's installer.
  case "$target" in
    *windows*) foreign='\.sh$' other='^$' ;;
    *linux*) foreign='\.ps1$' other='macos' ;;
    *) foreign='\.ps1$' other='linux' ;;
  esac
  if find "$stage" -type f | grep -E "$foreign|/(test-|check-licenses|generate-|verify-|sign-|transcript-e2e|package-release)"; then
    fail "$target archive ships a script that target does not install with"
  fi
  # shellcheck disable=SC2016
  for reference in $(grep -h -o -E '\$ROOT/(scripts|packaging)/[A-Za-z0-9_./-]+' "$stage"/scripts/*.sh 2>/dev/null | grep -v -E "$other" | sort -u); do
    [ -e "$stage/${reference#\$ROOT/}" ] || fail "$target archive lacks ${reference#\$ROOT/}"
  done
done
for script in $(grep -o -E '"[a-z-]+\.(ps1|py)"' "$ROOT/scripts/install-windows.ps1" | tr -d '"' | sort -u); do
  [ -f "$STAGE_ROOT/x86_64-pc-windows-msvc/scripts/$script" ] || fail "Windows archive lacks $script"
done
[ -f "$STAGE_ROOT/x86_64-pc-windows-msvc/scripts/install-windows.ps1" ] || fail "Windows archive lacks install-windows.ps1"

printf 'verify_packaging_ok\n'
