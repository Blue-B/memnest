<h1 align="center">palimpsest</h1>

<p align="center">
  <strong>Layered persistent memory for AI coding agents — local, encrypted, free.</strong>
  <br/>
  <em>One Rust engine + an MCP bridge + a git-backed audit layer. No cloud, $0 per call.</em>
</p>

<p align="center">
  <a href="./LICENSE"><img src="https://img.shields.io/badge/license-MIT-green.svg" alt="license" /></a>
</p>

---

**palimpsest** is a local-first memory system for AI agents. Everything lives in
one SQLite store at `~/.palimpsest/memory.db`, and any tool you use — pi, Claude
Desktop, Cursor, Cline, curl — reads and writes the **same** memory, searchable
forever, for $0.

This is a monorepo containing the three layers of the system:

| Directory | Package | Lang | Role |
| --------- | ------- | ---- | ---- |
| [`core/`](./core) | `palimpsest` | Rust | **The engine.** HTTP API + stdio MCP server, hybrid BM25+vector search, encrypted secret vault, built-in dashboard. *Required.* |
| [`pi-extension/`](./pi-extension) | `pi-palimpsest` | TS/npm | **The connector.** Exposes palimpsest memory as MCP tools (plus AutoLog) inside [pi](https://github.com/badlogic/pi-mono) and other clients. *Use if you run pi.* |
| [`journal/`](./journal) | `palimpsest-journal` | TS/npm | **The audit layer.** Mirrors the memory DB to a git-backed markdown repo so you can `git diff` / `revert` / PR-review what the AI learned. *Optional.* |

## Quick start

```bash
# 1. build & run the engine
cd core
cargo build --release
./target/release/palimpsest            # HTTP on 127.0.0.1:3111 + dashboard

# 2. (optional) connect pi
pi install npm:pi-palimpsest
memory_remember text="Project X uses port 8317"
memory_search   query="port 8317"

# 3. (optional) mirror memory to git
npm install -g palimpsest-journal
pjournal init ~/memory-journal && pjournal sync --push
```

Open the dashboard at **http://127.0.0.1:3111/**.

## How the pieces fit

```
        ┌─────────────────────────────────────────┐
        │  pi · Claude Desktop · Cursor · curl …    │
        └───────────────┬───────────────┬──────────┘
                        │ MCP/HTTP       │ stdio MCP
              pi-extension (npm)         │
                        │                │
                        ▼                ▼
                ┌───────────────────────────────┐
                │  core (Rust)  127.0.0.1:3111   │
                │  search · facts · secrets · UI │
                └───────────────┬───────────────┘
                                │  ~/.palimpsest/memory.db
                                ▼
                       journal (npm) → git markdown mirror
```

## Building / testing each layer

- **core** — `cd core && cargo build --release && cargo test`
- **pi-extension** — `cd pi-extension && npm install && npm run build && npm run smoke`
- **journal** — `cd journal && npm install && npm run smoke`

Each subdirectory keeps its own `README`, `LICENSE`, and `CHANGELOG`, and the two
npm packages publish independently from their own folders.

## License

MIT © Blue-B
