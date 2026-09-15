# Two independent clients, one saved decision

<!-- markdownlint-disable MD013 -->

This demo checks one narrow claim: two independent clients connected to one Memnest service can persist and retrieve the same record. It makes two separate curl invocations:

1. Client A calls `memory_remember` through **MCP over Streamable HTTP**.
2. Client B receives no process or chat state from A and calls `/search` through the **JSON HTTP API**.
3. A local verifier requires the returned memory IDs and full text to match.
4. The script also feeds a synthetic transcript in Codex's supported JSONL shape through `memnest watch`, then verifies retrieval. This checks the real parser without claiming Codex produced the fixture.

These are [supported Memnest client surfaces](../adapters/README.md#supported-surfaces). The path tests persistence across independent transport clients and one shipped transcript parser. It does not launch Claude Code, Codex, pi, or an LLM, and it does not test whether an autonomous agent decides to save, recall, trust, or apply memory.

A separate [real-client attempt](client-validation.md) passed the pi search path. Claude Code and Codex stopped on account access and usage errors before tool use, so that attempt is not presented as a cross-agent demo.

## Run it

Requirements are a running Memnest service, the `memnest` executable, Bash, curl, and Python 3. The recorder uses no npm or Python packages. By default the service is expected at `http://127.0.0.1:3111`; set `MEMNEST_BIN` if the executable is not on `PATH`.

```bash
./docs/run-independent-client-demo.sh /tmp/memnest-demo-evidence
cat /tmp/memnest-demo-evidence/transcript.txt
```

Set `MEMNEST_URL` for another local address. If the service has bearer authentication, set `MEMNEST_TOKEN`; the token is sent as a header and is not written to the evidence.

The script writes one fictional decision to a unique `memnest-demo-<timestamp>-<pid>` project, so repeated runs against one service do not deduplicate each other. Project scope separates search results, not database files. For a clean run, start a disposable service with its own data directory:

```bash
DATA_DIR="$(mktemp -d)"
memnest --data-dir "$DATA_DIR" --port 3111
# In a second terminal, run the recorder command above.
# Stop the service, then remove "$DATA_DIR" when finished.
```

The first write or search may download the local embedding model. That cold start is not included in the terminal cast's editorial timing and can take longer than the requests' five-minute limit. See [operations](operations.md#cli-reference) for `--warmup-embedding`.

## Evidence produced

The destination contains:

| File | Meaning |
| --- | --- |
| `client-a-mcp-request.json` | Exact JSON-RPC tool request sent by Client A. |
| `client-a-mcp-response.json` | Complete MCP response body, including the write ID. |
| `client-b-http-request.json` | Exact JSON request sent by Client B. |
| `client-b-http-response.json` | Complete HTTP response body, including ranked result IDs and text. |
| `health.json` | Service health response for the run. |
| `codex-transcript/session.jsonl` | Synthetic fixture in Codex's supported `session_meta` + `event_msg` JSONL shape. |
| `watch-output.txt` | Output from `memnest watch` parsing and sending that fixture. |
| `transcript-search-*.json` | Request and raw response proving the parsed fixture can be retrieved. |
| `manifest.json` | Service version when reported, verified IDs, scope statement, and SHA-256 hashes. |
| `transcript.txt` | Human-readable terminal transcript generated from the raw responses. |
| `demo.cast` | Asciicast v2 JSONL, paced to approximately 45 seconds from the raw responses. |

The script exits nonzero unless Client A reports `status: succeeded` and Client B returns that exact ID and text. HTTP success alone is not accepted as tool success.

The repository's [checked-in evidence directory](demo-evidence/) is one run against a disposable service. To inspect the artifact without any player, read `transcript.txt` or the JSONL cast. If asciinema is already installed, play it with:

```bash
asciinema play docs/demo-evidence/demo.cast
```

The cast is not a screen recording: the recorder builds it from the run's raw responses and assigns explanatory pauses. Its manifest hashes let a reviewer verify that the raw files used in that run have not changed.

## Earlier visual result

![Earlier real requests and responses from two MCP clients](demo-result.png)

This image and its [raw response file](demo-output.json) are an earlier v0.2.1 MCP-to-MCP run. Pillow was used to arrange selected fields for readability; it is not an agent UI screenshot and no image model generated the result. The [Korean image](demo-result.ko.png) translates labels while retaining the original tool data.

Neither recording is a comparative accuracy or latency benchmark. IDs, scores, and elapsed time vary between runs.

## Try it with agents

Connect two agent hosts to the same HTTP service using the [adapter guide](../adapters/README.md#mcp-clients). The following prompts make the success condition explicit.

In Client A:

```text
Call memory_remember exactly once with project="memnest-demo-agent" and this text:
"Checkout webhook retries must reuse the original event ID."
Return the stored record ID.
```

Start a fresh session in Client B:

```text
Call memory_search in project="memnest-demo-agent" for:
"How do checkout retries avoid duplicate processing?"
Report the matching record ID and quote the stored text. Do not answer from general knowledge.
```

Count the handoff as successful only when the Client B tool result contains Client A's record ID and exact text. Inspect the actual tool calls rather than accepting the model's answer alone.

This is a separate, interactive test. A successful scripted transport demo cannot substitute for evidence that a particular agent autonomously chose the tools. The saved payment rule is fictional and is not a complete payment implementation.

## Remove the example

Call `memory_delete` with every memory ID returned by repeated demo writes. Delete is soft: records move to trash, with retention described in [operations](operations.md#retention-and-recovery).
