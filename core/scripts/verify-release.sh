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
target/release/palimpsest --help | grep -q -- '--warmup-embedding'
scripts/smoke-local.sh

rm -rf /tmp/palimpsest-dist-check
mkdir -p /tmp/palimpsest-dist-check/dist
cp target/release/palimpsest /tmp/palimpsest-dist-check/dist/
cp -r scripts packaging docs static /tmp/palimpsest-dist-check/dist/
tar -czf /tmp/palimpsest-dist-check/palimpsest-test.tar.gz -C /tmp/palimpsest-dist-check/dist .
sha256sum /tmp/palimpsest-dist-check/palimpsest-test.tar.gz > /tmp/palimpsest-dist-check/palimpsest-test.tar.gz.sha256
scripts/verify-artifact.sh /tmp/palimpsest-dist-check/palimpsest-test.tar.gz /tmp/palimpsest-dist-check/palimpsest-test.tar.gz.sha256
tar -tzf /tmp/palimpsest-dist-check/palimpsest-test.tar.gz > /tmp/palimpsest-dist-check/manifest.txt
grep -q './scripts/install.sh' /tmp/palimpsest-dist-check/manifest.txt
grep -q './packaging/systemd/palimpsest-user.service' /tmp/palimpsest-dist-check/manifest.txt
grep -q './packaging/windows/palimpsest-service.xml' /tmp/palimpsest-dist-check/manifest.txt
grep -q './docs/PRODUCT_READINESS.md' /tmp/palimpsest-dist-check/manifest.txt
grep -q './docs/RELEASE_SIGNOFF.md' /tmp/palimpsest-dist-check/manifest.txt
grep -q './docs/PRODUCT_AUDIT.md' /tmp/palimpsest-dist-check/manifest.txt
grep -q './docs/SECURITY.md' /tmp/palimpsest-dist-check/manifest.txt
grep -q './docs/THIRD_PARTY_NOTICES.md' /tmp/palimpsest-dist-check/manifest.txt
grep -q './static/memory-atlas.png' /tmp/palimpsest-dist-check/manifest.txt
grep -q './scripts/validate-installed.sh' /tmp/palimpsest-dist-check/manifest.txt
grep -q './scripts/validate-installed-windows.ps1' /tmp/palimpsest-dist-check/manifest.txt
grep -q './scripts/preflight-linux.sh' /tmp/palimpsest-dist-check/manifest.txt
grep -q './scripts/preflight-windows.ps1' /tmp/palimpsest-dist-check/manifest.txt
grep -q './scripts/verify-artifact.sh' /tmp/palimpsest-dist-check/manifest.txt
grep -q './scripts/verify-artifact-windows.ps1' /tmp/palimpsest-dist-check/manifest.txt
grep -q './scripts/verify-workflows.sh' /tmp/palimpsest-dist-check/manifest.txt
grep -q './scripts/product-audit.sh' /tmp/palimpsest-dist-check/manifest.txt
grep -q './scripts/check-licenses.py' /tmp/palimpsest-dist-check/manifest.txt
grep -q './scripts/sign-windows.ps1' /tmp/palimpsest-dist-check/manifest.txt
grep -q './scripts/verify-windows-signatures.ps1' /tmp/palimpsest-dist-check/manifest.txt
grep -q './scripts/support-bundle.sh' /tmp/palimpsest-dist-check/manifest.txt
grep -q './scripts/support-bundle-windows.ps1' /tmp/palimpsest-dist-check/manifest.txt

echo "verify_release_ok"
