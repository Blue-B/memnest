#!/usr/bin/env bash
# Record a transport-level Memnest demo without launching an agent or an LLM.
set -euo pipefail

BASE_URL="${MEMNEST_URL:-http://127.0.0.1:3111}"
BASE_URL="${BASE_URL%/}"
OUT="${1:-memnest-demo-evidence}"
TEXT='Checkout webhooks: use the event ID as the idempotency key; retries must not charge twice.'
QUERY='How should checkout webhook retries avoid duplicate charges?'
TRANSCRIPT_TEXT='Transcript handoff: checkout retry tests must reuse the original event ID.'
MEMNEST_BIN="${MEMNEST_BIN:-memnest}"
PROJECT="memnest-demo-$(date -u +%Y%m%dT%H%M%S)-$$"

for command in curl python3 "$MEMNEST_BIN"; do
  command -v "$command" >/dev/null || { echo "missing required command: $command" >&2; exit 2; }
done
mkdir -p "$OUT/codex-transcript"
WATCH_STATE="$(mktemp -d -t memnest-demo-watch-XXXXXX)"
trap 'rm -rf "$WATCH_STATE"' EXIT

headers=(-H 'Content-Type: application/json')
if [[ -n "${MEMNEST_TOKEN:-}" ]]; then
  headers+=(-H "Authorization: Bearer ${MEMNEST_TOKEN}")
fi

cat >"$OUT/client-a-mcp-request.json" <<JSON
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"memory_remember","arguments":{"text":"$TEXT","project":"$PROJECT","memory_kind":"rule","importance":"decision"}}}
JSON
cat >"$OUT/client-b-http-request.json" <<JSON
{"query":"$QUERY","project":"$PROJECT","n_results":3,"adapter":"demo-independent-http"}
JSON
cat >"$OUT/codex-transcript/session.jsonl" <<JSONL
{"type":"session_meta","payload":{"id":"scripted-codex-session","cwd":"/tmp/memnest-demo-transcript","thread_source":"user"}}
{"type":"event_msg","id":"scripted-turn-1","payload":{"type":"user_message","message":"$TRANSCRIPT_TEXT"}}
JSONL
cat >"$OUT/transcript-search-request.json" <<JSON
{"query":"What marker must checkout retry tests reuse?","cwd":"/tmp/memnest-demo-transcript","n_results":3,"adapter":"demo-transcript-verifier"}
JSON

printf 'Checking %s ...\n' "$BASE_URL"
curl --fail-with-body --silent --show-error --max-time 15 \
  "${headers[@]}" "$BASE_URL/health" >"$OUT/health.json"

printf 'Client A (curl process using MCP over Streamable HTTP): remember\n'
curl --fail-with-body --silent --show-error --max-time 300 \
  "${headers[@]}" -H 'Accept: application/json, text/event-stream' \
  --data-binary @"$OUT/client-a-mcp-request.json" "$BASE_URL/mcp" \
  >"$OUT/client-a-mcp-response.json"

printf 'Client B (new curl process using the JSON HTTP API): search\n'
curl --fail-with-body --silent --show-error --max-time 300 \
  "${headers[@]}" --data-binary @"$OUT/client-b-http-request.json" \
  "$BASE_URL/search" >"$OUT/client-b-http-response.json"

printf 'Transcript fixture (real Codex JSONL shape): memnest watch, then search\n'
{
  printf 'command=memnest watch --path <fixture> --once --backfill\n'
  "$MEMNEST_BIN" --data-dir "$WATCH_STATE" watch --url "$BASE_URL" \
    --path "$OUT/codex-transcript" --once --backfill
  printf 'exit_status=0\n'
} >"$OUT/watch-output.txt"
curl --fail-with-body --silent --show-error --max-time 300 \
  "${headers[@]}" --data-binary @"$OUT/transcript-search-request.json" \
  "$BASE_URL/search" >"$OUT/transcript-search-response.json"

python3 - "$OUT" "$BASE_URL" "$PROJECT" <<'PY'
import datetime, hashlib, json, pathlib, sys

out = pathlib.Path(sys.argv[1])
base_url = sys.argv[2]
project = sys.argv[3]
text = "Checkout webhooks: use the event ID as the idempotency key; retries must not charge twice."
transcript_text = "Transcript handoff: checkout retry tests must reuse the original event ID."

def load(name):
    with (out / name).open(encoding="utf-8") as f:
        return json.load(f)

health = load("health.json")
remember = load("client-a-mcp-response.json")
search = load("client-b-http-response.json")
transcript_search = load("transcript-search-response.json")
try:
    tool_texts = [part["text"] for part in remember["result"]["content"] if part.get("type") == "text"]
    tool_result = next(json.loads(value) for value in tool_texts if value.lstrip().startswith("{"))
    memory_id = tool_result["id"]
    assert tool_result["status"] == "succeeded"
    matches = [row for row in search["results"] if row["id"] == memory_id and text in row["document"]]
    transcript_matches = [row for row in transcript_search["results"] if row["document"] == "User said: " + transcript_text]
    assert matches
    assert transcript_matches
except (AssertionError, KeyError, StopIteration, TypeError, json.JSONDecodeError) as error:
    raise SystemExit(f"FAIL: responses do not prove matching write and retrieval: {error}")

pretty_remember = json.dumps(remember, indent=2, ensure_ascii=False)
pretty_search = json.dumps(search, indent=2, ensure_ascii=False)
pretty_transcript_search = json.dumps(transcript_search, indent=2, ensure_ascii=False)
transcript = f"""MEMNEST INDEPENDENT-CLIENT DEMO

Scope: transport-level proof only; no agent or LLM was launched.
Service: {base_url}

CLIENT A — MCP over Streamable HTTP
memory_remember({text!r}, project='memnest-demo')

RAW RESPONSE
{pretty_remember}

CLIENT B — JSON HTTP API, a separate curl process with no Client A state
POST /search query='How should checkout webhook retries avoid duplicate charges?'

RAW RESPONSE
{pretty_search}

VERIFIED
Client A wrote ID: {memory_id}
Client B retrieved the same ID and exact text: yes

SUPPORTED TRANSCRIPT-FORMAT CHECK
A scripted fixture uses Codex's session_meta + event_msg JSONL shape.
memnest watch parsed it; a subsequent HTTP search returned:
{pretty_transcript_search}
Parsed transcript document found: yes ({transcript_matches[0]['id']})

This fixture checks the shipped parser and watcher without running Codex.
This does not show that an autonomous agent will decide to save, search, or apply a memory.
"""
(out / "transcript.txt").write_text(transcript, encoding="utf-8")

# Asciicast v2 is JSON Lines. Timing is editorially paced to ~45 seconds; all
# displayed IDs and response fields come from the raw files written above.
events = [
    (0.2, "$ ./docs/run-independent-client-demo.sh demo-evidence\r\n"),
    (2.5, "TRANSPORT PROOF — no agent or LLM is running\r\n\r\n"),
    (6.0, "Client A: MCP over Streamable HTTP → memory_remember\r\n"),
    (10.0, f"decision: {text}\r\n"),
    (15.0, f"write status: succeeded\r\nmemory ID: {memory_id}\r\n\r\n"),
    (21.0, "Client B: separate curl process, JSON HTTP API → POST /search\r\n"),
    (26.0, "query: How should checkout webhook retries avoid duplicate charges?\r\n"),
    (32.0, f"retrieved ID: {matches[0]['id']}\r\n"),
    (35.0, f"retrieved text: {matches[0]['document']}\r\n\r\n"),
    (38.0, "Codex-format JSONL fixture → memnest watch → HTTP search: PASS\r\n"),
    (41.0, "PASS: IDs/text match; the shipped transcript parser also retrieved its fixture.\r\n"),
    (44.5, "Not proved: autonomous agent save/recall/application behavior.\r\n"),
]
header = {"version": 2, "width": 100, "height": 24, "timestamp": int(datetime.datetime.now().timestamp()),
          "env": {"SHELL": "/bin/bash", "TERM": "xterm-256color"}, "title": "Memnest independent-client transport demo"}
with (out / "demo.cast").open("w", encoding="utf-8") as f:
    f.write(json.dumps(header, separators=(",", ":")) + "\n")
    for timestamp, content in events:
        f.write(json.dumps([timestamp, "o", content], separators=(",", ":"), ensure_ascii=False) + "\n")

files = ["health.json", "client-a-mcp-request.json", "client-a-mcp-response.json",
         "client-b-http-request.json", "client-b-http-response.json", "codex-transcript/session.jsonl",
         "watch-output.txt", "transcript-search-request.json", "transcript-search-response.json",
         "transcript.txt", "demo.cast"]
manifest = {
    "recorded_at": datetime.datetime.now(datetime.timezone.utc).isoformat(),
    "service_version": health.get("version", "not reported"),
    "service_url": base_url,
    "project": project,
    "memory_id": memory_id,
    "transcript_memory_id": transcript_matches[0]["id"],
    "verification": {"same_id": True, "exact_text": True, "codex_transcript_fixture_retrieved": True},
    "scope": "transport/parser-level write and retrieval; no autonomous agent behavior tested",
    "files": {name: hashlib.sha256((out / name).read_bytes()).hexdigest() for name in files},
    "recorder": str(pathlib.Path("docs/run-independent-client-demo.sh")),
}
(out / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
print(f"PASS: Client B retrieved Client A memory {memory_id}")
print(f"Raw evidence, transcript, manifest, and ~45-second cast: {out}")
PY
