<div align="center">

# palimpsest

**Layered persistent memory for AI coding agents — local, encrypted, free.**

One Rust engine, an MCP bridge, and a git-backed audit layer. No cloud, no per-call cost.

[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](./LICENSE)
![Rust](https://img.shields.io/badge/core-Rust-orange.svg)
![Protocol](https://img.shields.io/badge/interface-MCP%20%2B%20HTTP-blue.svg)

[English](./README.md) · [한국어](./README.ko.md)

<br/>

<img src="docs/dashboard.png" alt="palimpsest dashboard" width="820" />

</div>

---

## Overview

palimpsest is a local-first memory system for AI agents. Everything lives in a
single SQLite store at `~/.palimpsest/memory.db`, and any client you use — Claude
Code, Claude Desktop, Cursor, Cline, Codex CLI, pi, or `curl` — reads and writes the
same memory. No account, no cloud, and your data never leaves your machine.

It is client-agnostic by design: the engine exposes a **stdio MCP** server and an
**HTTP API** over one store, so it is not tied to any single editor or agent.

## Features

- **Hybrid search** — BM25 full-text (Tantivy) fused with vector similarity (HNSW) over native [fastembed](https://github.com/Anush008/fastembed-rs) embeddings.
- **Knowledge graph and lifecycle** — relationships between memories, plus importance-weighted decay and consolidation of old entries.
- **Encrypted secret vault** — credentials are stored with AES-256-GCM (Argon2-derived key); incoming text is scanned and secrets (`sk-…`, private keys, `api_key=…`) are redacted before storage.
- **Built-in dashboard** — unified search, per-collection volume, and recent entries, in Korean and English.
- **Two interfaces, one store** — an HTTP API and a stdio MCP server read the same database.
- **Hardened defaults** — binds to `127.0.0.1` only, refuses non-local binds without a token, and sets CSP, `nosniff`, and `no-store` headers.

## Supported clients

Because the engine speaks stdio MCP, it works with any MCP-compatible client. They
all share the one `~/.palimpsest/memory.db`, so a memory written in one client is
searchable in every other.

| Client | How to connect |
| ------ | -------------- |
| Claude Desktop, Cursor, Cline, Continue, Zed, opencode | Register the `palimpsest --mcp` command — see [`pi-extension/INSTALL-CLIENTS.md`](./pi-extension/INSTALL-CLIENTS.md). |
| Claude Code, Codex CLI, Kilo Code, Windsurf, … | Any other MCP-capable client uses the same `palimpsest --mcp` registration. |
| pi | One-command install: `pi install npm:pi-palimpsest` (adds tools plus AutoLog). |
| Scripts / anything | Call the HTTP API directly at `http://127.0.0.1:3111`. |

The MCP registration is identical everywhere:

```json
{ "command": "palimpsest", "args": ["--mcp"] }
```

## Repository layout

This is a monorepo for the three layers of the system.

| Directory | Package | Language | Role |
| --------- | ------- | -------- | ---- |
| [`core/`](./core) | `palimpsest` | Rust | The engine: HTTP API + stdio MCP server, hybrid search, secret vault, dashboard. Required. |
| [`pi-extension/`](./pi-extension) | `pi-palimpsest` | TypeScript | A convenience bridge for [pi](https://github.com/badlogic/pi-mono) (tools + AutoLog). Other clients connect to the core directly over MCP, so this is optional. |
| [`journal/`](./journal) | `palimpsest-journal` | TypeScript | Audit layer that mirrors the database to a git-backed markdown repo, so you can diff, revert, and review what the agent learned. Optional. |

## Quick start

```bash
# 1. Build and run the engine
cd core
cargo build --release
./target/release/palimpsest          # HTTP on 127.0.0.1:3111 + dashboard

# 2. Connect a client (any MCP client uses the same command)
#    Register:  command "palimpsest", args ["--mcp"]
#    For pi:
pi install npm:pi-palimpsest
memory_remember text="Project X uses port 8317"
memory_search   query="port 8317"

# 3. (optional) Mirror memory to git
npm install -g palimpsest-journal
pjournal init ~/memory-journal && pjournal sync --push
```

The dashboard is served at `http://127.0.0.1:3111/`.

## Running the engine

```bash
palimpsest                      # HTTP server + dashboard (127.0.0.1:3111)
palimpsest --mcp                # stdio MCP mode
palimpsest --doctor             # environment and store health check
palimpsest --warmup-embedding   # preload the embedding model
```

Common flags: `--port`, `--host`, `--data-dir`, `--backup-dir`, `--restore-dir`,
`--import-jsonl`. Run `palimpsest --help` for the full list.

## Usage examples

Record and recall through any connected client (here, pi's tools):

```text
memory_remember text="Deploy uses blue-green on port 8080"
memory_search   query="deploy port"
```

Or call the HTTP API directly:

```bash
# add a memory
curl -s http://127.0.0.1:3111/add \
  -H 'content-type: application/json' \
  -d '{"text":"Deploy uses blue-green on port 8080","project":"acme"}'

# hybrid search (BM25 + vector)
curl -s http://127.0.0.1:3111/search \
  -H 'content-type: application/json' \
  -d '{"query":"deploy port","n_results":5}'

# store statistics
curl -s http://127.0.0.1:3111/stats
```

Review and revert what the agent learned, through the journal:

```bash
pjournal sync --push                  # export DB -> commit -> push
git -C ~/memory-journal log --oneline
git -C ~/memory-journal revert <bad-commit>
```

## Architecture

```
   Claude Code · Claude Desktop · Cursor · Cline · Codex CLI · pi · curl …
                 |                                   |
                 | stdio MCP  (palimpsest --mcp)     | HTTP
                 v                                   v
        +-------------------------------------------------+
        |   core (Rust)            127.0.0.1:3111         |
        |   search · facts · secrets · dashboard          |
        +-----------------------+-------------------------+
                                |  ~/.palimpsest/memory.db
                                v
                       journal (npm) -> git markdown mirror
```

## Building and testing

| Layer | Commands |
| ----- | -------- |
| core | `cd core && cargo build --release && cargo test` |
| pi-extension | `cd pi-extension && npm install && npm run build && npm run smoke` |
| journal | `cd journal && npm install && npm run smoke` |

Each subdirectory keeps its own `README`, `LICENSE`, and `CHANGELOG`. The two npm
packages are published independently from their own folders.

## Security

- The default bind is `127.0.0.1`. Non-local binds are refused unless `PALIMPSEST_TOKEN` is set, in which case a `Bearer` token is required.
- Secrets are encrypted with AES-256-GCM; the master key never leaves the local disk and is never exported.
- See [`core/docs/SECURITY.md`](./core/docs/SECURITY.md) for the threat model.

## License

MIT © Blue-B
