#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

fail() {
  echo "product audit failed: $1" >&2
  exit 1
}

require_file() {
  [ -f "$1" ] || fail "missing file: $1"
}

require_pattern() {
  local file="$1"
  local pattern="$2"
  local label="$3"
  grep -Fq "$pattern" "$file" || fail "$label"
}

require_absent_pattern() {
  local file="$1"
  local pattern="$2"
  local label="$3"
  if grep -Fq "$pattern" "$file"; then
    fail "$label"
  fi
}

require_file static/memory-atlas.png
require_file scripts/install-linux.sh
require_file scripts/install-windows.ps1
require_file scripts/install-wsl.ps1
require_file scripts/uninstall-linux.sh
require_file scripts/uninstall-windows.ps1
require_file scripts/uninstall-wsl.ps1
require_file scripts/validate-installed.sh
require_file scripts/validate-installed-windows.ps1
require_file scripts/preflight-linux.sh
require_file scripts/preflight-windows.ps1
require_file docs/DEPLOYMENT.md
require_file docs/PRODUCT_READINESS.md
require_file docs/PRODUCT_AUDIT.md
require_file docs/RELEASE_SIGNOFF.md
require_file docs/SECURITY.md
require_file docs/TROUBLESHOOTING.md
require_file docs/THIRD_PARTY_NOTICES.md

require_pattern src/server/api.rs "highlight_query_html" "viewer search must highlight query terms"
require_pattern src/server/api.rs "require_visible_match" "viewer search must filter to visible matches"
require_pattern src/server/api.rs "data-i18n" "dashboard must expose i18n hooks"
require_pattern src/server/api.rs "data-memory-count" "dashboard dynamic memory counts must be localizable"
require_pattern src/server/api.rs "data-result-count" "dashboard dynamic search result counts must be localizable"
require_pattern src/server/api.rs "data-scope-count" "dashboard collection scope options must be localizable"
require_pattern src/server/api.rs "memory-atlas.png" "dashboard must use packaged visual asset"
require_absent_pattern src/server/api.rs "cdn.tailwindcss.com" "dashboard source still references Tailwind CDN"
require_absent_pattern src/server/api.rs "fonts.googleapis.com" "dashboard source still references Google Fonts"

require_pattern scripts/install-linux.sh "static" "Linux installer must copy dashboard static assets"
require_pattern scripts/install-windows.ps1 "static" "Windows installer must copy dashboard static assets"
require_pattern scripts/validate-installed.sh "/assets/memory-atlas.png" "Linux validator must verify dashboard assets"
require_pattern scripts/validate-installed-windows.ps1 "/assets/memory-atlas.png" "Windows validator must verify dashboard assets"
require_pattern scripts/smoke-local.sh "cdn\\.tailwindcss\\.com" "smoke test must reject external dashboard CDN dependencies"
require_pattern scripts/check-licenses.py "DENIED_MARKERS" "release gate must include license screening"
require_pattern .github/workflows/release.yml "WINDOWS_CODESIGN_PFX_BASE64" "Windows releases must require signing credentials"
require_pattern .github/workflows/release.yml "static" "release archives must include dashboard static assets"
require_pattern docs/PRODUCT_READINESS.md "Required before paid distribution" "readiness doc must list external paid-release gates"
require_pattern docs/PRODUCT_AUDIT.md "Not complete until externally verified" "audit doc must distinguish local evidence from external gates"

echo "product_audit_ok"
