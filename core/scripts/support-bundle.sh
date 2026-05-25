#!/usr/bin/env bash
set -euo pipefail

OUT="${OUT:-palimpsest-support-$(date +%Y%m%d-%H%M%S).txt}"
MODE="${MODE:-user}"
SERVICE="${SERVICE:-palimpsest.service}"
URL="${URL:-http://127.0.0.1:3111/health}"

usage() {
  cat <<'EOF'
Usage: scripts/support-bundle.sh [--user|--system] [--out path]

Collects non-secret service diagnostics for support. The bundle includes
service status, recent logs, health output, binary version, disk usage, and
runtime metadata. It does not copy memory database contents.
EOF
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --user) MODE="user" ;;
    --system) MODE="system" ;;
    --out) OUT="${2:-}"; shift ;;
    -h|--help) usage; exit 0 ;;
    *) echo "unknown argument: $1" >&2; usage; exit 2 ;;
  esac
  shift
done

section() {
  printf '\n## %s\n' "$1" >> "$OUT"
}

run_capture() {
  printf '\n$ %s\n' "$*" >> "$OUT"
  "$@" >> "$OUT" 2>&1 || true
}

: > "$OUT"
section "Palimpsest Support Bundle"
printf 'created_at=%s\n' "$(date -Iseconds)" >> "$OUT"
printf 'mode=%s\n' "$MODE" >> "$OUT"
printf 'health_url=%s\n' "$URL" >> "$OUT"

section "System"
run_capture uname -a
run_capture date -Iseconds
run_capture df -h .
run_capture free -h

section "Binary"
if command -v palimpsest >/dev/null 2>&1; then
  run_capture palimpsest --version
else
  printf 'palimpsest binary not found in PATH\n' >> "$OUT"
fi

section "Health"
if command -v curl >/dev/null 2>&1; then
  run_capture curl -fsS "$URL"
else
  printf 'curl not found\n' >> "$OUT"
fi

section "Service"
if [ "$MODE" = "system" ]; then
  run_capture systemctl status "$SERVICE" --no-pager -l
  run_capture journalctl -u "$SERVICE" -n 150 --no-pager
else
  run_capture systemctl --user status "$SERVICE" --no-pager -l
  run_capture journalctl --user -u "$SERVICE" -n 150 --no-pager
fi

printf 'support bundle written: %s\n' "$OUT"
