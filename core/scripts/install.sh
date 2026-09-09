#!/usr/bin/env bash
set -euo pipefail

REPO="${REPO:-https://github.com/Blue-B/memnest}"
VERSION="${VERSION:-latest}"
MODE="${MODE:-user}"

usage() {
  cat <<'EOF'
Usage:
  scripts/install.sh [--user|--system]

Downloads a release archive and runs the idempotent Linux systemd or macOS
launchd setup, including detected client configuration and a scratch round trip.
For WSL and Windows native installs, use:

  scripts/install-wsl.ps1
  scripts/install-windows.ps1
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
  --user) MODE="user" ;;
  --system) MODE="system" ;;
  -h | --help)
    usage
    exit 0
    ;;
  *)
    echo "unknown argument: $1" >&2
    usage
    exit 2
    ;;
  esac
  shift
done

OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"

case "$ARCH" in
x86_64) TARGET_ARCH="x86_64" ;;
aarch64 | arm64) TARGET_ARCH="aarch64" ;;
*)
  echo "unsupported architecture: $ARCH" >&2
  exit 1
  ;;
esac

case "$OS" in
linux) TARGET="${TARGET_ARCH}-unknown-linux-gnu" ;;
darwin)
  [ "$MODE" = user ] || { echo "macOS installs support only --user" >&2; exit 1; }
  TARGET="${TARGET_ARCH}-apple-darwin"
  ;;
*)
  echo "unsupported OS: $OS" >&2
  exit 1
  ;;
esac

if [ "$VERSION" = "latest" ]; then
  API_URL="https://api.github.com/repos/Blue-B/memnest/releases/latest"
  VERSION="$(curl -fsSL "$API_URL" | sed -n 's/.*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' | head -1)"
  if [ -z "$VERSION" ]; then
    echo "failed to determine latest version" >&2
    echo "if no release is published yet, build from source instead:" >&2
    echo "  git clone ${REPO} && cd memnest/core && cargo build --release" >&2
    exit 1
  fi
fi

ARCHIVE="memnest-${VERSION}-${TARGET}.tar.gz"
URL="${REPO}/releases/download/${VERSION}/${ARCHIVE}"
CHECKSUM_URL="${URL}.sha256"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

echo "Installing Memnest ${VERSION} for ${TARGET}"
curl -fsSL "$URL" -o "$TMP_DIR/$ARCHIVE"
curl -fsSL "$CHECKSUM_URL" -o "$TMP_DIR/$ARCHIVE.sha256"

expected="$(awk '{print tolower($1)}' "$TMP_DIR/$ARCHIVE.sha256")"
if command -v sha256sum >/dev/null 2>&1; then
  actual="$(sha256sum "$TMP_DIR/$ARCHIVE" | awk '{print tolower($1)}')"
else
  actual="$(shasum -a 256 "$TMP_DIR/$ARCHIVE" | awk '{print tolower($1)}')"
fi
if [ "$expected" != "$actual" ]; then
  echo "checksum mismatch for $ARCHIVE" >&2
  echo "expected: $expected" >&2
  echo "actual:   $actual" >&2
  exit 1
fi

tar -xzf "$TMP_DIR/$ARCHIVE" -C "$TMP_DIR"

(
  cd "$TMP_DIR"
  scripts/setup.sh "--${MODE}" --bin "$TMP_DIR/memnest"
)
