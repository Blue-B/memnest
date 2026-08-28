#!/usr/bin/env bash
set -euo pipefail

REPO="${REPO:-https://github.com/Blue-B/memnest}"
VERSION="${VERSION:-latest}"
MODE="${MODE:-user}"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

usage() {
  cat <<'EOF'
Usage:
  scripts/install.sh [--user|--system]

Downloads a release archive, installs the memnest binary, and registers
the Linux systemd service. For WSL and Windows native installs, use:

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
actual="$(sha256sum "$TMP_DIR/$ARCHIVE" | awk '{print tolower($1)}')"
if [ "$expected" != "$actual" ]; then
  echo "checksum mismatch for $ARCHIVE" >&2
  echo "expected: $expected" >&2
  echo "actual:   $actual" >&2
  exit 1
fi

tar -xzf "$TMP_DIR/$ARCHIVE" -C "$TMP_DIR"

mkdir -p "$INSTALL_DIR"
install -m 0755 "$TMP_DIR/memnest" "$INSTALL_DIR/memnest"

if [ "$OS" = "linux" ]; then
  (
    cd "$TMP_DIR"
    BIN_SRC="$INSTALL_DIR/memnest" scripts/install-linux.sh "--${MODE}" --bin "$INSTALL_DIR/memnest"
  )
else
  echo "Binary installed to $INSTALL_DIR/memnest"
  echo "Start it with: $INSTALL_DIR/memnest --host 127.0.0.1 --port 3111"
fi

echo "Memnest API and MCP: http://127.0.0.1:3111/"
