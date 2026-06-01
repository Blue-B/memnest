#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

bash -n scripts/install.sh scripts/install-linux.sh scripts/uninstall-linux.sh scripts/smoke-local.sh scripts/validate-installed.sh scripts/support-bundle.sh scripts/preflight-linux.sh scripts/verify-artifact.sh scripts/verify-workflows.sh scripts/product-audit.sh
bash scripts/verify-workflows.sh
bash scripts/product-audit.sh
python3 scripts/check-licenses.py
cargo test --quiet
cargo build --release
target/release/memnest --help | grep -q -- '--warmup-embedding'
scripts/smoke-local.sh

rm -rf /tmp/memnest-dist-check
mkdir -p /tmp/memnest-dist-check/dist
cp target/release/memnest /tmp/memnest-dist-check/dist/
cp -r scripts packaging docs static /tmp/memnest-dist-check/dist/
tar -czf /tmp/memnest-dist-check/memnest-test.tar.gz -C /tmp/memnest-dist-check/dist .
sha256sum /tmp/memnest-dist-check/memnest-test.tar.gz > /tmp/memnest-dist-check/memnest-test.tar.gz.sha256
scripts/verify-artifact.sh /tmp/memnest-dist-check/memnest-test.tar.gz /tmp/memnest-dist-check/memnest-test.tar.gz.sha256
tar -tzf /tmp/memnest-dist-check/memnest-test.tar.gz > /tmp/memnest-dist-check/manifest.txt
grep -q './scripts/install.sh' /tmp/memnest-dist-check/manifest.txt
grep -q './packaging/systemd/memnest-user.service' /tmp/memnest-dist-check/manifest.txt
grep -q './packaging/windows/memnest-service.xml' /tmp/memnest-dist-check/manifest.txt
grep -q './docs/PRODUCT_READINESS.md' /tmp/memnest-dist-check/manifest.txt
grep -q './docs/RELEASE_SIGNOFF.md' /tmp/memnest-dist-check/manifest.txt
grep -q './docs/PRODUCT_AUDIT.md' /tmp/memnest-dist-check/manifest.txt
grep -q './docs/SECURITY.md' /tmp/memnest-dist-check/manifest.txt
grep -q './docs/THIRD_PARTY_NOTICES.md' /tmp/memnest-dist-check/manifest.txt
grep -q './static/memory-atlas.png' /tmp/memnest-dist-check/manifest.txt
grep -q './scripts/validate-installed.sh' /tmp/memnest-dist-check/manifest.txt
grep -q './scripts/validate-installed-windows.ps1' /tmp/memnest-dist-check/manifest.txt
grep -q './scripts/preflight-linux.sh' /tmp/memnest-dist-check/manifest.txt
grep -q './scripts/preflight-windows.ps1' /tmp/memnest-dist-check/manifest.txt
grep -q './scripts/verify-artifact.sh' /tmp/memnest-dist-check/manifest.txt
grep -q './scripts/verify-artifact-windows.ps1' /tmp/memnest-dist-check/manifest.txt
grep -q './scripts/verify-workflows.sh' /tmp/memnest-dist-check/manifest.txt
grep -q './scripts/product-audit.sh' /tmp/memnest-dist-check/manifest.txt
grep -q './scripts/check-licenses.py' /tmp/memnest-dist-check/manifest.txt
grep -q './scripts/sign-windows.ps1' /tmp/memnest-dist-check/manifest.txt
grep -q './scripts/verify-windows-signatures.ps1' /tmp/memnest-dist-check/manifest.txt
grep -q './scripts/support-bundle.sh' /tmp/memnest-dist-check/manifest.txt
grep -q './scripts/support-bundle-windows.ps1' /tmp/memnest-dist-check/manifest.txt

echo "verify_release_ok"
