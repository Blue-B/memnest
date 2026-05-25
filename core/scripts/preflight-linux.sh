#!/usr/bin/env bash
set -euo pipefail

MODE="${MODE:-user}"
BIN="${BIN:-}"
HOST="${PALIMPSEST_HOST:-127.0.0.1}"
PORT="${PALIMPSEST_PORT:-3111}"
DATA_DIR="${PALIMPSEST_DATA_DIR:-$HOME/.palimpsest}"

usage() {
  cat <<'EOF'
Usage: scripts/preflight-linux.sh [--user|--system] [--bin /path/to/palimpsest]

Checks whether this Linux machine is ready to install Palimpsest. It does not
change system state.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --user) MODE="user" ;;
    --system) MODE="system" ;;
    --bin) BIN="${2:-}"; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown argument: $1" >&2; usage; exit 2 ;;
  esac
  shift
done

failures=0

check() {
  local name="$1"
  shift
  if "$@" >/dev/null 2>&1; then
    printf 'ok: %s\n' "$name"
  else
    printf 'fail: %s\n' "$name" >&2
    failures=$((failures + 1))
  fi
}

check "systemctl is available" command -v systemctl
check "curl is available" command -v curl

if [ "$MODE" = "user" ]; then
  check "systemd user manager is available" systemctl --user show-environment
else
  check "system service manager is available" systemctl show-environment
  check "sudo is available for system install" command -v sudo
fi

if [ -n "$BIN" ]; then
  check "palimpsest binary is executable" test -x "$BIN"
elif [ -x "./palimpsest" ]; then
  check "release binary is executable" test -x "./palimpsest"
elif [ -x "./target/release/palimpsest" ]; then
  check "source build binary is executable" test -x "./target/release/palimpsest"
else
  printf 'fail: palimpsest binary not found; pass --bin or extract/build first\n' >&2
  failures=$((failures + 1))
fi

check_writable_target() {
  local path="$1"
  if [ -d "$path" ]; then
    test -w "$path"
    return
  fi

  local parent
  parent="$(dirname "$path")"
  while [ ! -d "$parent" ] && [ "$parent" != "/" ]; do
    parent="$(dirname "$parent")"
  done
  test -w "$parent"
}

check "data directory or nearest parent is writable" check_writable_target "$DATA_DIR"
check "dashboard static assets are present" test -f "$ROOT/static/memory-atlas.png"

is_local_host() {
  case "$1" in
    127.0.0.1|localhost|::1) return 0 ;;
    *) return 1 ;;
  esac
}

if is_local_host "$HOST"; then
  printf 'ok: host %s is a supported local bind\n' "$HOST"
else
  printf 'fail: packaged installs only support local binds; got %s\n' "$HOST" >&2
  failures=$((failures + 1))
fi

if command -v ss >/dev/null 2>&1; then
  if ss -ltn "( sport = :${PORT} )" | grep -q ":${PORT}"; then
    printf 'warn: port %s is already listening on %s\n' "$PORT" "$HOST" >&2
  else
    printf 'ok: port %s is not currently listening\n' "$PORT"
  fi
else
  printf 'warn: ss not found; skipping port availability check\n' >&2
fi

if [ "$failures" -gt 0 ]; then
  printf 'preflight failed with %s issue(s)\n' "$failures" >&2
  exit 1
fi

printf 'preflight_linux_ok\n'
