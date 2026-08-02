# memnest

<!-- markdownlint-disable MD013 -->

Local memory service for AI coding agents.

One Rust binary that stores what your agent learned, answers retrieval queries about it, and shows you whether the recall actually helped.

[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](./LICENSE)
![Rust](https://img.shields.io/badge/core-Rust-orange.svg)
![Protocol](https://img.shields.io/badge/interface-MCP%20%2B%20HTTP-blue.svg)

![memnest operations dashboard](docs/dashboard.png)

## Why this exists

An agent session ends and the project decisions, the user's preferences, and every correction you made go with it. Next session you explain the same constraints again.

The usual fixes each give something up. Hosted memory services want your project history on their servers. A bare vector database stores embeddings but cannot tell you which memory was actually used, whether a write succeeded, or why retrieval returned the wrong thing. Rolling your own means you own an indexing pipeline instead of shipping features.

memnest keeps the store in a directory on your machine, serves it over HTTP and MCP so any agent can reach it, and records every recall so retrieval quality is something you can inspect and correct instead of guess at.

## Architecture

```text
   pi extension            MCP clients             Any other host
   20 tools, /memnest      Claude Code,            HTTP + JSONL
   autocontext             Cursor, and others      adapter contract
          |                       |                       |
          +-----------+-----------+-----------+-----------+
                      |                       |
                 HTTP :3111               stdio MCP
                      |                       |
          +-----------v-----------------------v-----------+
          |             memnest core (Rust)               |
          |                                               |
          |   write path      search path      operations |
          |   redact          BM25 + vector    recall log  |
          |   embed           RRF fusion       job status   |
          |   dedup + alias   re-rank + MMR    feedback     |
          +-----------------------+-----------------------+
                                  |
          +-----------------------v-----------------------+
          |          ~/.memnest, one data directory       |
          |   SQLite      tantivy      HNSW      archive  |
          |   records     BM25 index   vectors   JSONL    |
          +-----------------------------------------------+
```

A write is redacted for credential-shaped strings, embedded, checked against existing memories, and acknowledged only after it is stored. A search fuses BM25 and vector candidates, re-ranks them by importance, type, recency, and recorded feedback, then diversifies the result with MMR. Every search returns a `recall_id`, and marking that recall helpful or harmful changes how those memories rank next time.

## What you get

| Area | What is implemented |
| --- | --- |
| Retrieval | Hybrid BM25 and HNSW vector search, project filters, compact excerpts, nearest-neighbor queries, and a `recall_id` on every search |
| Feedback loop | Helpful and harmful outcomes persist per memory and feed back into the ranking score, so memories that proved useful surface first |
| Structured memory | Optional record, fact, rule, and procedure kinds with confidence, provenance, verification, and replacement metadata |
| Context assembly | Character-bounded context packs built from matching memories, notes, and facts, counted in Unicode characters so non-Latin text is not truncated early |
| Observability | 90 days of recall events and processing jobs with latency, adapter identity, and outcomes, shown in an operations console |
| Recovery | Deletes move to a hidden trash collection, restore reindexes them, and hard-deleted records are archived to monthly JSONL first |
| Secret storage | A separate vault encrypts credential values with AES-256-GCM using a local master key |
| Integration | One binary serves HTTP and stdio MCP. pi is first-class, and a public adapter contract covers other hosts |

Memory, note, fact, and session text is stored locally but is not encrypted at rest. The secret vault is the only storage path designed for sensitive values. See [Security](#security).

## Quick start

Build and run the engine from this checkout. There are no published release tags or npm packages yet.

```bash
git clone https://github.com/Blue-B/memnest.git
cd memnest/core
cargo build --release
./target/release/memnest --data-dir ~/.memnest
```

The service listens on `http://127.0.0.1:3111` and serves the dashboard on the same port. New installs use `~/.memnest`. An existing `~/.factory/memories` store keeps being used until you migrate, so an upgrade never hides data behind a new default path.

You do not have to remember the port:

```bash
memnest status      # health, dashboard link, data directory
memnest dashboard   # just the clickable link
```

Save a memory, search for it, then tell memnest whether the result was any good:

```bash
curl -s http://127.0.0.1:3111/add \
  -H 'content-type: application/json' \
  -d '{"text":"Deploy uses port 8320","project":"acme","metadata":{"importance":"knowledge","memory_kind":"fact"}}'

curl -s http://127.0.0.1:3111/search \
  -H 'content-type: application/json' \
  -d '{"query":"deploy port","project":"acme","n_results":3}'

curl -s http://127.0.0.1:3111/feedback \
  -H 'content-type: application/json' \
  -d '{"recall_id":"recall_...","outcome":"helpful"}'
```

The first write takes longer because fastembed downloads the embedding model. `/add` returns a memory id and a job id, and reports `succeeded` or `deduplicated` only after the record is stored and indexed.

## Connect your agent

### pi

Start the HTTP service, then install the extension from this checkout:

```bash
cd /path/to/memnest/pi-extension
npm install
pi install .
```

It connects to `http://127.0.0.1:3111` by default and registers 20 tools. Run `/memnest` inside pi for status and the dashboard link. Set `MEMNEST_URL` for a different address and `MEMNEST_TOKEN` when bearer authentication is on. Details are in [`pi-extension/README.md`](./pi-extension/README.md).

### Any MCP client

```bash
./target/release/memnest --mcp --data-dir ~/.memnest
```

Register that command in the client, using absolute paths when it does not inherit your shell environment:

```json
{
  "mcpServers": {
    "memnest": {
      "command": "/absolute/path/to/memnest",
      "args": ["--mcp", "--data-dir", "/home/you/.memnest"]
    }
  }
}
```

Each MCP client starts its own process, so do not point two writers at one data directory. For pi, scripts, and the dashboard together, run a single long-lived HTTP service instead.

### Anything else

Translate your host's events into HTTP calls using the dependency-free JSONL adapter in [`adapters/`](./adapters). Adapters send `adapter` and `adapter_version` on every write and search, so their traffic and failures stay visible in the operations console.

## How memnest compares

memnest is a memory engine, not an agent runtime. It does not run your agent, manage prompts, or replace compaction. It remembers, retrieves, and reports.

| Category | What it does | Where memnest differs |
| --- | --- | --- |
| Session-continuity extensions, such as [pi-observational-memory](https://github.com/elpapi42/pi-observational-memory) | Keep one long session coherent across compaction by capturing observations and reflections, recovered by id | memnest is a cross-session searchable store with hybrid retrieval. The two solve different problems and can run together: one protects today's session, the other remembers last quarter's decision |
| Hosted memory services | Managed memory behind an account and an API | The store is a local directory you can back up, inspect, and delete. No account, no upload |
| Agent platforms with built-in memory | Bundle memory with their own runtime, desktop app, and sync | memnest stays a dependency of the agent you already use, and ships as one binary with no runtime of its own |
| Vector databases | Store and search embeddings | memnest adds redaction, deduplication with stable ids, lifecycle and recovery, and a feedback loop that changes ranking |

## Repository layout

| Directory | Package | Role |
| --- | --- | --- |
| [`core/`](./core) | `memnest` 0.2.0 | The engine. HTTP API, MCP server, indexes, lifecycle, observability, vault, dashboard |
| [`pi-extension/`](./pi-extension) | `pi-memnest` 0.6.0 | First-class pi integration: 20 tools, `/memnest`, autocontext, feedback, opt-in AutoLog |
| [`adapters/`](./adapters) | contract | Platform-neutral integration contract and a reference JSONL adapter |
| [`journal/`](./journal) | `memnest-journal` 0.1.0 | Optional Markdown and git audit mirror, not a database backup |
| [`learn/`](./learn) | `memnest-learn` 0.1.0 | Optional pi learning and working-memory layer that uses the host agent model |

Only `core/` is required.

## Documentation

- [Operations guide](docs/operations.md) covers service install on Linux, WSL, and Windows, backup and restore, retention and recovery, CLI reference, and development checks
- [0.2 before and after report](docs/upgrade-0.2-before-after.md) records the measured baseline, what changed, and the verification evidence
- [Adapter contract](adapters/README.md) is the integration surface for non-pi hosts

## Security

The HTTP server binds to `127.0.0.1`. A non-local bind is refused unless `MEMNEST_TOKEN` is set, and requests must then send `Authorization: Bearer <token>`. Do not expose port 3111 to the internet directly; put a reviewed reverse proxy and TLS in front of it if you need remote access.

Incoming memory text is scanned for credential-shaped strings and redacted, but that is a safety net, not a place to put secrets. Use the vault for those. On normal startup memnest creates `<data-dir>/master.key` and uses it for AES-256-GCM secret values; confirm that file exists before relying on vault encryption, because the crypto helper falls back to stored plaintext when no key is available.

Engine attributions are in [`core/THIRD_PARTY_NOTICES.md`](./core/THIRD_PARTY_NOTICES.md).

## Contributing

Run the checks in the [operations guide](docs/operations.md#development-checks) for the component you touched. Issues and pull requests go to the [memnest repository](https://github.com/Blue-B/memnest/issues).

## License

MIT © Blue-B
