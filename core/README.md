<h1 align="center">memnest</h1>

<p align="center">
  <strong>Layered persistent memory for AI coding agents — a single local Rust binary.</strong>
  <br/>
  <em>HTTP API + stdio MCP • BM25 + vector hybrid search • AES-GCM secret vault • built-in dashboard • no cloud.</em>
</p>

<p align="center">
  <a href="./LICENSE"><img src="https://img.shields.io/badge/license-MIT-green.svg" alt="license" /></a>
  <img src="https://img.shields.io/badge/rust-edition%202024-orange.svg" alt="rust" />
</p>

---

`memnest` is the core memory server: a single Rust binary that stores every
memory chunk, fact, note, session summary, and (encrypted) credential in one
local SQLite database at `~/.memnest/memory.db`. It speaks two protocols
over the same store, so any tool you use writes to the same memory:

- **HTTP API** (default `http://127.0.0.1:3111`) — used by editors, scripts,
  curl, and the [pi-memnest](https://github.com/Blue-B/memnest) extension.
- **stdio MCP** (`memnest --mcp`) — register directly in Claude Desktop,
  Cursor, Cline, Continue, Zed (memory, context, notes, facts, graph, lifecycle, secrets, servers).

It runs entirely on your machine. No account, no cloud, $0 per call.

## Features

- **Hybrid search** — BM25 full-text ([tantivy](https://github.com/quickwit-oss/tantivy)) fused with
  vector similarity (HNSW, pure-Rust) over native [fastembed](https://github.com/Anush008/fastembed-rs) embeddings.
- **Context packs** — `/context` returns always-on notes, matching facts, and retrieved memories as a prompt-ready `<memnest_context>` block.
- **Correctable memory** — `/update` rewrites an existing memory by id and refreshes text/vector indexes to avoid stale duplicates.
- **Core notes** — `/notes` exposes small key-value memory blocks for persona, user profile, active projects, or operating rules.
- **Knowledge graph** — relationships between memories ([petgraph](https://github.com/petgraph/petgraph)).
- **Lifecycle** — importance-weighted decay and consolidation of old memories.
- **Encrypted secret vault** — SSH/server credentials stored AES-256-GCM, key
  derived with Argon2; the master key (`~/.memnest/master.key`) never leaves disk.
- **Secret redaction** — incoming text is scanned for `sk-…`, private keys,
  `api_key=…` patterns and redacted before storage.
- **Built-in dashboard** — open `http://127.0.0.1:3111/` in a browser: unified
  search, per-collection volume, recent entries (Korean / English i18n).
- **Hardened by default** — binds to `127.0.0.1` only; refuses non-local binds
  unless `MEMNEST_TOKEN` is set; sends CSP / `nosniff` / `no-store` headers.

## Install

```bash
# Linux / WSL (user-level)
cargo build --release
scripts/install-linux.sh --user --bin target/release/memnest

# Windows (PowerShell) and WSL helpers also live in scripts/
#   scripts/install-windows.ps1
#   scripts/install-wsl.ps1 -Distro Ubuntu-24.04 -RepoPath /home/<your-wsl-username>/memnest
```

See [docs/DEPLOYMENT.md](docs/DEPLOYMENT.md) for service setup (systemd / Windows
scheduled task), [docs/UPDATE.md](docs/UPDATE.md) and
[docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md).

## Run

```bash
memnest                 # HTTP server on 127.0.0.1:3111 + dashboard
memnest --mcp           # stdio MCP mode
memnest --doctor        # environment / store health check
memnest --warmup-embedding   # preload the embedding model
```

Common flags: `--port`, `--host`, `--data-dir`, `--backup-dir`, `--restore-dir`,
`--import-jsonl`, `--import-facts-json`. Run `memnest --help` for the full list.

## HTTP API (selected)

| Method | Path | Purpose |
| ------ | ---- | ------- |
| GET  | `/health` | liveness probe |
| POST | `/search` | hybrid BM25 + vector search |
| POST | `/context` | prompt-ready notes + facts + retrieved memories |
| POST | `/add` | add a memory chunk |
| POST | `/update` | update a memory chunk and refresh indexes |
| GET  | `/collections` | list project buckets with counts |
| GET  | `/facts` · POST `/facts` | structured fact triples |
| GET  | `/notes` · POST `/notes` · GET/DELETE `/notes/:key` | key-value notes |
| GET  | `/servers` | stored server/credential entries |
| GET  | `/sessions` | recent session summaries |
| GET  | `/stats` | store statistics |
| GET  | `/` | dashboard (HTML) |

## Security

- Default bind is `127.0.0.1`; `enforce_bind_safety` refuses any non-local host
  unless `MEMNEST_TOKEN` is set (then a `Bearer` token is required).
- Secrets are AES-256-GCM encrypted; the master key stays outside any export.
- See [docs/SECURITY.md](docs/SECURITY.md) for the threat model.

## Build & test

```bash
cargo build --release
cargo test            # unit tests
scripts/product-audit.sh    # product readiness gates
python3 scripts/check-licenses.py   # dependency license check
```

## Related projects

- [**pi-memnest**](https://github.com/Blue-B/memnest) — bridge that exposes
  memnest memory as MCP tools inside [pi](https://github.com/badlogic/pi-mono) and other clients.
- [**memnest-journal**](https://github.com/Blue-B/memnest) — mirror the
  memory DB to a git-backed markdown repo so you can `git diff` / `revert` / PR-review what the AI learned.

## License

MIT © Blue-B
