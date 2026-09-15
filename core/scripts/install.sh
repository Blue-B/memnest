#!/usr/bin/env bash
set -euo pipefail

REPO="${REPO:-https://github.com/Blue-B/memnest}"
VERSION="${VERSION:-latest}"
MODE="${MODE:-user}"
AUTOCONTEXT=0

usage() {
  cat <<'EOF'
Usage:
  scripts/install.sh [--user|--system] [--autocontext]

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
  --autocontext) AUTOCONTEXT=1 ;;
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
  args=("--${MODE}" --bin "$TMP_DIR/memnest")
  [ "$AUTOCONTEXT" = 0 ] || args+=(--autocontext)
  if [ -f scripts/setup.sh ]; then
    scripts/setup.sh "${args[@]}"
  else
    # v0.2.1 predates setup.sh. Do not run its overwriting installer on an upgrade.
    [ "$OS" = linux ] && [ "$AUTOCONTEXT" = 0 ] || {
      echo "Release $VERSION does not provide setup for these options; use a source build." >&2
      exit 1
    }
    unit_paths="$(systemd-analyze "--$MODE" unit-paths)" || {
      echo "Cannot inspect systemd unit paths; use a manual/source upgrade." >&2; exit 1;
    }
    [ -n "$unit_paths" ] || { echo "Empty systemd unit path; refusing legacy install." >&2; exit 1; }
    while IFS= read -r unit_dir; do
      [ ! -d "$unit_dir" ] || { [ -r "$unit_dir" ] && [ -x "$unit_dir" ]; } || {
        echo "Cannot inspect systemd path: $unit_dir" >&2; exit 1;
      }
      for entry in memnest.service memnest-watch.service memnest.service.d memnest-watch.service.d memnest-.service.d service.d; do
        if [ -e "$unit_dir/$entry" ] || [ -L "$unit_dir/$entry" ]; then
          echo "Legacy release installer cannot preserve existing service settings; use the source-build upgrade path." >&2
          exit 1
        fi
      done
    done <<< "$unit_paths"
    echo "Legacy release: installs the Linux server only. Configure clients and capture separately."
    scripts/install-linux.sh "--${MODE}" --bin "$TMP_DIR/memnest"
  fi
)
