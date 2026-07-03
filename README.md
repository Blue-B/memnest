<div align="center">

# memnest

**Layered persistent memory for AI coding agents — local, encrypted, free.**

One Rust engine, an MCP bridge, and a git-backed audit layer. No cloud, no per-call cost.

[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](./LICENSE)
![Rust](https://img.shields.io/badge/core-Rust-orange.svg)
![Protocol](https://img.shields.io/badge/interface-MCP%20%2B%20HTTP-blue.svg)

<br/>

<img src="docs/dashboard.png" alt="memnest dashboard" width="820" />

</div>

---

## Overview

memnest is a local-first memory system for AI agents. Everything lives in a
single SQLite store at `~/.memnest/memory.db`, and any client you use — Claude
Code, Claude Desktop, Cursor, Cline, Codex CLI, pi, or `curl` — reads and writes the
same memory. No account, no cloud, and your data never leaves your machine.

It is client-agnostic by design: the engine exposes a **stdio MCP** server and an
**HTTP API** over one store, so it is not tied to any single editor or agent.

## Features

- **Hybrid search** — BM25 full-text (Tantivy) fused with vector similarity (HNSW) over native [fastembed](https://github.com/Anush008/fastembed-rs) embeddings.
- **Context packs** — one call returns always-on notes, matching facts, and retrieved memories as a prompt-ready `<memnest_context>` block.
- **Correctable memory** — update an existing memory by id and refresh text/vector indexes, so stale facts are fixed instead of duplicated.
- **Core notes** — small key-value memory blocks for durable persona, user profile, active project, or operating rules.
- **Knowledge graph and lifecycle** — relationships between memories, plus importance-weighted decay and consolidation of old entries.
- **Encrypted secret vault** — credentials are stored with AES-256-GCM (Argon2-derived key); incoming text is scanned and secrets (`sk-…`, private keys, `api_key=…`) are redacted before storage.
- **Built-in dashboard** — unified search, per-collection volume, and recent entries, in Korean and English.
- **Two interfaces, one store** — an HTTP API and a stdio MCP server read the same database.
- **Hardened defaults** — binds to `127.0.0.1` only, refuses non-local binds without a token, and sets CSP, `nosniff`, and `no-store` headers.

## Repository layout

This is a monorepo for the layers of the system.

| Directory | Package | Language | Role |
| --------- | ------- | -------- | ---- |
| [`core/`](./core) | `memnest` | Rust | The engine: HTTP API + stdio MCP server, hybrid search, secret vault, dashboard. Required. |
| [`pi-extension/`](./pi-extension) | `pi-memnest` | TypeScript | A convenience bridge for [pi](https://github.com/badlogic/pi-mono) (tools + AutoLog + Autocontext). Other clients connect to the core directly over MCP, so this is optional. |
| [`journal/`](./journal) | `memnest-journal` | TypeScript | Audit layer that mirrors the database to a git-backed markdown repo, so you can diff, revert, and review what the agent learned. Optional. |
| [`learn/`](./learn) | `memnest-learn` | TypeScript | Learning + working-memory layer: failure/correction learning and KV-cache-stable injection, borrowing the host agent's own model (no extra API key). Optional. |

## Quick start

```bash
# 1. Build the engine (or grab a release binary)
git clone https://github.com/Blue-B/memnest
cd memnest/core
cargo build --release
cp target/release/memnest ~/.local/bin/

# 2. Run it
memnest                     # HTTP + dashboard on http://127.0.0.1:3111

# 3. Connect a client — the MCP registration is identical everywhere:
#    { "command": "memnest", "args": ["--mcp"] }
#    For pi:
pi install npm:pi-memnest

# 4. (optional) Mirror memory to git
npm install -g memnest-journal
pjournal init ~/memory-journal && pjournal sync --push
```

Engine flags:

```bash
memnest                      # HTTP server + dashboard (127.0.0.1:3111)
memnest --mcp                # stdio MCP mode
memnest --doctor             # environment and store health check
memnest --warmup-embedding   # preload the embedding model
```

Common flags: `--port`, `--host`, `--data-dir`, `--backup-dir`, `--restore-dir`,
`--import-jsonl`. Run `memnest --help` for the full list.

## Connect your client

Every MCP client registers the same command — `memnest --mcp` — and they all
share the one `~/.memnest/memory.db`, so a memory written in one client is
searchable in every other. You do **not** need a running service for `--mcp`
mode; each client spawns its own short-lived stdio server.

```json
{
  "mcpServers": {
    "memnest": { "command": "memnest", "args": ["--mcp"] }
  }
}
```

| Client | Config location |
| ------ | --------------- |
| Claude Desktop | macOS `~/Library/Application Support/Claude/claude_desktop_config.json` · Windows `%APPDATA%\Claude\claude_desktop_config.json` (WSL binary: `"command": "wsl.exe", "args": ["-e", "memnest", "--mcp"]`) |
| Cursor | `~/.cursor/mcp.json` (global) or `<project>/.cursor/mcp.json` |
| Cline | `.../globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json` |
| Continue.dev | `~/.continue/config.json` → `experimental.modelContextProtocolServers` |
| Zed | `~/.config/zed/settings.json` → `context_servers` |
| Claude Code, Codex CLI, Windsurf, opencode, … | Any MCP-capable client: register command `memnest`, args `["--mcp"]`. |
| pi | `pi install npm:pi-memnest` — HTTP bridge with memory tools, AutoLog, and risk-triggered Autocontext. |
| Scripts / anything | Call the HTTP API directly at `http://127.0.0.1:3111`. |

Optional `args` additions: `--data-dir <path>` to override `~/.memnest`;
`--warmup-embedding` for a slower start but faster first query.

If a client shows 0 tools, `memnest` is probably not in its PATH — use an
absolute path like `/home/you/.local/bin/memnest`. If two clients fight over
the same store's write lock, run the shared service below and keep `--mcp`
only in the primary client.

## Run as a service

For pi + curl + the dashboard to work alongside MCP clients, run the engine as
a long-lived local service. Installers live in [`core/scripts/`](./core/scripts).

**Linux (systemd user service, data in `~/.memnest`):**

```bash
cd core && cargo build --release
scripts/preflight-linux.sh --user --bin target/release/memnest
scripts/install-linux.sh   --user --bin target/release/memnest
# server:  scripts/install-linux.sh --system  (data in /var/lib/memnest)
# verify:  curl -fsS http://127.0.0.1:3111/health
# remove:  scripts/uninstall-linux.sh --user
```

**WSL (service inside the distro + Windows logon task that wakes it):**

```powershell
.\scripts\install-wsl.ps1 -Distro Ubuntu-24.04 -RepoPath /home/<user>/memnest
# verify: wsl -d Ubuntu-24.04 -- systemctl --user status memnest.service
# remove: .\scripts\uninstall-wsl.ps1 -Distro Ubuntu-24.04 -RepoPath /home/<user>/memnest
```

**Windows native (`memnest.exe` wrapped by WinSW, data in `%ProgramData%\Memnest\data`):**

```powershell
.\scripts\preflight-windows.ps1
.\scripts\install-windows.ps1          # options: -Port 3211, -BinPath, -WinSWPath + -WinSWSha256
```

**Updating** is installer-managed: back up, then install the new release over
the existing service — data stays in the configured data directory.

```bash
systemctl --user stop memnest.service
memnest --data-dir ~/.memnest --backup-dir ~/memnest-backup      # backup
scripts/install-linux.sh --user --bin <new-binary>               # upgrade
# rollback: memnest --data-dir ~/.memnest --restore-dir ~/memnest-backup --force
```

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

# update a stale memory and refresh indexes
curl -s http://127.0.0.1:3111/update \
  -H 'content-type: application/json' \
  -d '{"id":"manual_...","text":"Deploy now uses port 8320","importance":"decision"}'

# prompt-ready context pack: notes + facts + retrieved memories
curl -s http://127.0.0.1:3111/context \
  -H 'content-type: application/json' \
  -d '{"query":"deploy port","project":"acme"}'

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
                 | stdio MCP  (memnest --mcp)     | HTTP
                 v                                   v
        +-------------------------------------------------+
        |   core (Rust)            127.0.0.1:3111         |
        |   search · facts · secrets · dashboard          |
        +-----------------------+-------------------------+
                                |  ~/.memnest/memory.db
                                v
                       journal (npm) -> git markdown mirror
```

## Building and testing

| Layer | Commands |
| ----- | -------- |
| core | `cd core && cargo build --release && cargo test` |
| pi-extension | `cd pi-extension && npm install && npm run build && npm run smoke` |
| journal | `cd journal && npm install && npm run smoke` |
| learn | `cd learn && npm install && npm run build && npm test` |

## Troubleshooting

- **Dashboard does not open** — check the service: `systemctl --user status memnest.service`, then `curl -fsS http://127.0.0.1:3111/health`.
- **Port 3111 in use** — start with `--port <other>` or free the port.
- **WSL service dead after reboot** — `Start-ScheduledTask -TaskName "Memnest WSL"`, then check `systemctl --user status memnest.service` inside the distro.
- **First search/save fails offline** — the embedding model downloads on first use; run `memnest --warmup-embedding` once while online.
- **Remote bind refused** — non-localhost binds require `MEMNEST_TOKEN`; clients must then send `Authorization: Bearer <token>`.

## Security

- The default bind is `127.0.0.1`. Non-local binds are refused unless `MEMNEST_TOKEN` is set, in which case a `Bearer` token is required. Never expose the port to the public internet without a reviewed reverse proxy and TLS in front.
- Secrets are encrypted with AES-256-GCM (Argon2-derived key); the master key never leaves the local disk. Incoming text is scanned and credential-shaped strings are redacted before storage.
- Data directories: `~/.memnest` (Linux user service), `/var/lib/memnest` (system service), `%ProgramData%\Memnest\data` (Windows service). Stop the service before backing up or restoring.
- Third-party license attributions for the engine: [`core/THIRD_PARTY_NOTICES.md`](./core/THIRD_PARTY_NOTICES.md).

## License

MIT © Blue-B
