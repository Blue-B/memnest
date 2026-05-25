#!/usr/bin/env bash
set -euo pipefail

release=".github/workflows/release.yml"
ci=".github/workflows/ci.yml"

if [ ! -f "$release" ]; then
  echo "missing release workflow: $release" >&2
  exit 1
fi

if [ ! -f "$ci" ]; then
  echo "missing CI workflow: $ci" >&2
  exit 1
fi

require() {
  local file="$1"
  local pattern="$2"
  local label="$3"
  if ! grep -Fq "$pattern" "$file"; then
    echo "workflow check failed: $label" >&2
    echo "missing pattern: $pattern" >&2
    exit 1
  fi
}

require "$release" "WINDOWS_CODESIGN_PFX_BASE64" "Windows signing certificate secret is required"
require "$release" "WINDOWS_CODESIGN_PASSWORD" "Windows signing password secret is required"
require "$release" "throw \"WINDOWS_CODESIGN_PFX_BASE64 and WINDOWS_CODESIGN_PASSWORD are required" "unsigned Windows releases must fail"
require "$release" "WinSW-x64.exe" "Windows release must include WinSW"
require "$release" "WinSW-x64.exe.sha256" "Windows release must include WinSW checksum"
require "$release" "sign-windows.ps1" "Windows release must sign files"
require "$release" "verify-windows-signatures.ps1" "Windows release must verify signatures"
require "$release" "sha256sum" "Unix release checksums must be generated"
require "$release" "Get-FileHash" "Windows release checksum must be generated"
require "$release" "scripts" "Release artifact must include scripts"
require "$release" "packaging" "Release artifact must include packaging"
require "$release" "docs" "Release artifact must include docs"
require "$release" "static" "Release artifact must include dashboard static assets"

require "$ci" "cargo test --quiet" "CI must run Rust tests"
require "$ci" "cargo build --release" "CI must run release build"
require "$ci" "scripts/verify-release.sh" "CI must run product release gates"
require "$ci" "scripts/product-audit.sh" "CI must run product audit gates"
require "$ci" ".\\scripts\\verify-release.ps1" "CI must run Windows release parser gate"
require "$ci" "windows-latest" "CI must include Windows validation"

echo "verify_workflows_ok"
