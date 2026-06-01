#!/usr/bin/env bash
set -euo pipefail

ARTIFACT="${1:-}"
CHECKSUM="${2:-}"

usage() {
  cat <<'EOF'
Usage: scripts/verify-artifact.sh path/to/archive path/to/archive.sha256

Verifies a downloaded Memnest release archive against its SHA-256 file.
EOF
}

if [ -z "$ARTIFACT" ] || [ -z "$CHECKSUM" ]; then
  usage
  exit 2
fi

if [ ! -f "$ARTIFACT" ]; then
  echo "artifact not found: $ARTIFACT" >&2
  exit 1
fi

if [ ! -f "$CHECKSUM" ]; then
  echo "checksum file not found: $CHECKSUM" >&2
  exit 1
fi

expected="$(awk '{print tolower($1)}' "$CHECKSUM")"
actual="$(sha256sum "$ARTIFACT" | awk '{print tolower($1)}')"

if [ "$expected" != "$actual" ]; then
  echo "checksum mismatch for $ARTIFACT" >&2
  echo "expected: $expected" >&2
  echo "actual:   $actual" >&2
  exit 1
fi

echo "verify_artifact_ok"
