#!/usr/bin/env bash
set -euo pipefail

binary=${1:-target/debug/memnest}
if [[ ! -x "$binary" ]]; then
  echo "missing memnest binary: $binary" >&2
  exit 1
fi
binary=$(cd "$(dirname "$binary")" && pwd)/$(basename "$binary")

scratch=$(mktemp -d "${TMPDIR:-/tmp}/memnest-transcript-e2e.XXXXXX")
transcripts="$scratch/transcripts"
data="$scratch/data"
mkdir -p "$transcripts" "$data"
token=preserve-identical-retry-X7F404

python3 - "$transcripts/codex.jsonl" "$token" <<'PY'
import json, sys
path, token = sys.argv[1:]
rows = [
    {"type": "session_meta", "payload": {"id": "scratch-session", "cwd": "/tmp/scratch-project", "thread_source": "user"}},
    {"type": "event_msg", "id": "reused-host-id", "payload": {"type": "user_message", "message": token}},
    {"type": "event_msg", "id": "reused-host-id", "payload": {"type": "user_message", "message": token}},
]
with open(path, "w", encoding="utf-8") as handle:
    for row in rows:
        handle.write(json.dumps(row) + "\n")
PY

port=$(python3 - <<'PY'
import socket
sock = socket.socket()
sock.bind(("127.0.0.1", 0))
print(sock.getsockname()[1])
sock.close()
PY
)

"$binary" --data-dir "$data" --port "$port" >"$scratch/service.log" 2>&1 &
pid=$!
cleanup() {
  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
}
trap cleanup EXIT

ready=0
for _ in $(seq 1 120); do
  if curl -fsS "http://127.0.0.1:$port/health" >/dev/null 2>&1; then
    ready=1
    break
  fi
  if ! kill -0 "$pid" 2>/dev/null; then
    break
  fi
  sleep 1
done
if [[ "$ready" != 1 ]]; then
  tail -80 "$scratch/service.log" >&2
  exit 1
fi

run_watch() {
  "$binary" --data-dir "$data" watch \
    --url "http://127.0.0.1:$port" \
    --path "$transcripts" --once --backfill
}

assert_two() {
  curl -fsS -X POST "http://127.0.0.1:$port/search" \
    -H 'content-type: application/json' \
    -d "{\"query\":\"$token\",\"project\":\"\",\"cwd\":\"/tmp/scratch-project\",\"n_results\":10}" \
    >"$scratch/search.json"
  python3 - "$scratch/search.json" "$token" <<'PY'
import json, sys
path, token = sys.argv[1:]
documents = [item["document"] for item in json.load(open(path, encoding="utf-8")).get("results", [])]
expected = "User said: " + token
count = sum(document == expected for document in documents)
print(f"TRANSCRIPT_E2E_MATCHING={count}")
assert count == 2, documents
PY
}

run_watch
assert_two
printf '{"version":2,"files":{}}\n' >"$data/watch-state.json"
run_watch
assert_two

echo "TRANSCRIPT_E2E_DISTINCT=2"
echo "TRANSCRIPT_E2E_RETRY_TOTAL=2"
echo "TRANSCRIPT_E2E_SCRATCH=$scratch"
