#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN="${BIN:-$ROOT/target/release/palimpsest}"
PORT="${PORT:-$((39000 + RANDOM % 2000))}"
TMP_DIR="$(mktemp -d)"
PID=""

cleanup() {
  if [ -n "$PID" ]; then
    kill "$PID" 2>/dev/null || true
    wait "$PID" 2>/dev/null || true
  fi
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

start_server() {
  "$BIN" --data-dir "$TMP_DIR/server-data" --host 127.0.0.1 --port "$PORT" >"$TMP_DIR/server.out" 2>"$TMP_DIR/server.err" &
  PID="$!"
}

wait_for_health() {
  for _ in $(seq 1 60); do
    if curl --max-time 2 -fsS "http://127.0.0.1:${PORT}/health" >/dev/null 2>&1; then
      return 0
    fi
    sleep 1
  done
  return 1
}

if [ ! -x "$BIN" ]; then
  cargo build --release
fi

mkdir -p "$TMP_DIR/data/sub"
printf 'backup-check' > "$TMP_DIR/data/sub/file.txt"
"$BIN" --data-dir "$TMP_DIR/data" --backup-dir "$TMP_DIR/backup"
"$BIN" --data-dir "$TMP_DIR/restore" --restore-dir "$TMP_DIR/backup"
cmp "$TMP_DIR/data/sub/file.txt" "$TMP_DIR/restore/sub/file.txt"

if "$BIN" --host 0.0.0.0 --port 39999 --data-dir "$TMP_DIR/bind" >"$TMP_DIR/bind.out" 2>"$TMP_DIR/bind.err"; then
  echo "remote bind without PALIMPSEST_TOKEN unexpectedly succeeded" >&2
  exit 1
fi
grep -q "refusing to bind" "$TMP_DIR/bind.err"

start_server
wait_for_health
rss_kb="$(ps -o rss= -p "$PID" | tr -d '[:space:]')"
if [ -n "$rss_kb" ] && [ "$rss_kb" -gt 250000 ]; then
  echo "startup RSS too high before search: ${rss_kb}KB" >&2
  exit 1
fi
curl --max-time 5 -fsS "http://127.0.0.1:${PORT}/health" -o "$TMP_DIR/health.json"
grep -q '"status":"ok"' "$TMP_DIR/health.json"
curl --max-time 5 -fsS -D "$TMP_DIR/headers.txt" "http://127.0.0.1:${PORT}/health" -o /dev/null
tr -d '\r' < "$TMP_DIR/headers.txt" > "$TMP_DIR/headers.normalized.txt"
grep -qi '^x-content-type-options: nosniff$' "$TMP_DIR/headers.normalized.txt"
grep -qi '^content-security-policy:' "$TMP_DIR/headers.normalized.txt"
grep -qi '^referrer-policy: no-referrer$' "$TMP_DIR/headers.normalized.txt"
curl --max-time 5 -fsS "http://127.0.0.1:${PORT}/" -o "$TMP_DIR/dashboard.html"
grep -q "Palimpsest" "$TMP_DIR/dashboard.html"
if grep -Eq 'cdn\.tailwindcss\.com|fonts\.googleapis\.com|fonts\.gstatic\.com' "$TMP_DIR/dashboard.html"; then
  echo "dashboard must not depend on external CDN assets" >&2
  exit 1
fi
curl --max-time 5 -fsS "http://127.0.0.1:${PORT}/assets/memory-atlas.png" -o "$TMP_DIR/memory-atlas.png"

kill "$PID"
wait "$PID" 2>/dev/null || true
PID=""
start_server
wait_for_health
curl --max-time 5 -fsS "http://127.0.0.1:${PORT}/health" -o "$TMP_DIR/restart-health.json"
grep -q '"status":"ok"' "$TMP_DIR/restart-health.json"

echo "smoke_local_ok"
