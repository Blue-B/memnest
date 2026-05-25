<p align="center">
  <img src="./docs/logo.png" alt="pi-palimpsest" width="160" />
</p>

<h1 align="center">pi-palimpsest</h1>

<p align="center">
  <strong>One persistent memory layer for every AI tool you use — local, encrypted, free.</strong>
  <br/>
  <em>12 MCP tools • AES-GCM secret vault • BM25 + vector hybrid search • no cloud.</em>
</p>

<p align="center">
  <a href="https://www.npmjs.com/package/pi-palimpsest"><img src="https://img.shields.io/npm/v/pi-palimpsest.svg?style=flat&color=blue" alt="npm version" /></a>
  <a href="https://www.npmjs.com/package/pi-palimpsest"><img src="https://img.shields.io/npm/dm/pi-palimpsest.svg?style=flat&color=blue" alt="downloads" /></a>
  <a href="https://github.com/Blue-B/pi-palimpsest/blob/main/LICENSE"><img src="https://img.shields.io/npm/l/pi-palimpsest.svg?style=flat&color=green" alt="license" /></a>
  <a href="https://github.com/Blue-B/pi-palimpsest/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/Blue-B/pi-palimpsest/ci.yml?branch=main&style=flat&label=CI" alt="CI" /></a>
  <a href="https://github.com/sponsors/Blue-B"><img src="https://img.shields.io/badge/sponsor-❤-ea4aaa.svg?style=flat" alt="sponsor" /></a>
</p>

---

**pi-palimpsest** bridges [pi](https://github.com/badlogic/pi-mono) (and any
other MCP client) to a locally running
[palimpsest](https://github.com/badlogic/palimpsest) memory server. Memories
you write from Claude Desktop, Cursor, Cline, pi, or curl all land in the
**same SQLite database** at `~/.palimpsest/memory.db` — and stay searchable
across every session of every tool, forever, for $0.

```bash
# 30-second demo
pi install npm:pi-palimpsest                  # in pi
memory_remember text="Project X uses port 8317 for CLIProxy"
memory_search   query="CLIProxy port"          # → returns the chunk above
secret_set      key=github_pat value=ghp_...   # AES-256-GCM encrypted on disk
```

## Tools registered on the pi side (12)

All backed by the palimpsest HTTP API at `http://127.0.0.1:3111`:

| Tool | Purpose |
| ---- | ------- |
| `memory_remember`   | save a memory chunk (auto-routes preference/decision to `playbook`) |
| `memory_search`     | hybrid BM25 + vector search |
| `memory_stats`      | server statistics |
| `memory_sessions`   | recent session summaries |
| `memory_facts_list` | structured fact triples |
| `notes_list`        | list KV notes |
| `secret_set` / `secret_get` / `secret_list` / `secret_delete` | AES-GCM encrypted credentials |
| `collections_list`  | enumerate project buckets |
| `memory_health`     | server liveness probe |

The core palimpsest server's stdio MCP mode (`palimpsest --mcp`) exposes
**17 tools** (adds `memory_graph_query`, `memory_lifecycle_run`, `note_get`,
`note_set`, `server_add/update/info`, `memory_facts` search). For those,
register `palimpsest --mcp` in Claude Desktop / Cursor / Cline / Continue /
Zed alongside pi — see [INSTALL-CLIENTS.md](./INSTALL-CLIENTS.md).

## AutoLog — passive memory capture

Beyond the 12 explicit tools, `pi-palimpsest` installs **AutoLog**: event hooks
that automatically send your conversation to palimpsest as you work, so memory
accrues without you calling `memory_remember` by hand.

- Captures user inputs and the assistant's final messages (skips `thinking`
  content, very short noise, and palimpsest's own tool calls).
- Routes each write to a project derived from the session `cwd`.
- Fire-and-forget: never blocks or throws into the agent loop; pending writes
  are drained on session end (so `pi -p "…"` print mode still records).
- Disable it via environment variable if you prefer tool-only logging.

## Why this exists vs other memory layers

| | pi-palimpsest + palimpsest | mem0 | agentmemory (rohitg00) | Letta | Zep |
|---|:--:|:--:|:--:|:--:|:--:|
| Local-first, no cloud account required           | ✅ | ⚠️ (hosted preferred) | ✅ | ✅ | ⚠️ (cloud) |
| Single Rust binary, no Docker required           | ✅ | ❌ | ❌ (Rust engine + Python ext)    | ❌ | ❌ |
| BM25 + vector hybrid search                      | ✅ | ✅ | ✅ | ✅ | ✅ |
| AES-GCM encrypted secret store                   | ✅ | ❌ | ❌ | ❌ | ❌ |
| MCP stdio out of the box (no adapter)            | ✅ | partial | partial | partial | partial |
| Cross-client shared memory (Claude + Cursor + pi over one DB) | ✅ | ❌ | ❌ | ❌ | ❌ |
| Memory is auditable as plain files (git diff/revert/PR) via [palimpsest-journal](https://github.com/Blue-B/palimpsest-journal) | ✅ | ❌ | ❌ | ❌ | ❌ |
| Knowledge graph + lifecycle decay                | ✅ | ✅ | ✅ | ✅ | ✅ |
| SSH server credential vault built in             | ✅ | ❌ | ❌ | ❌ | ❌ |
| Per-call cost                                    | $0 | $$ (token + cloud) | $0 | $0 | $$ |

## Prerequisites

Palimpsest must be running. Default endpoint: `http://127.0.0.1:3111`.
Override with `PALIMPSEST_URL` env var.

## Install

```bash
# from npm
pi install npm:pi-palimpsest

# or from a local checkout (auto-builds via `prepare` hook)
pi install /path/to/pi-palimpsest
```

Then enable in pi `settings.json` (or it will be auto-discovered if installed via `pi install`):

```json
{
  "packages": ["npm:pi-palimpsest"]
}
```

## How it works

The extension entry point loaded by pi is the **pre-built bundle** at
`./dist/index.mjs`, not `src/index.ts`. The bundle is produced by `esbuild` and
includes all dependencies that pi does not provide (notably `typebox`), with
only `@earendil-works/pi-coding-agent` left external (provided by the host).

This shape avoids two failure modes observed when shipping `src/index.ts` as
the entry point:

1. `jiti` (pi's TS loader) occasionally fails to preserve method-shorthand
   bodies when re-transpiling under the Bun-compiled pi binary, surfacing as
   `TypeError: definition.execute is not a function` at tool-call time.
2. Peer-resolution of `typebox` is unreliable across `pnpm` / `npm` /
   `bun` installs and across direct vs. transitive installs of the host.

Shipping a pre-built ESM bundle removes both. The bundle is platform-agnostic
ESM targeting Node 20+; it works under Node, Bun, and the Bun-compiled `pi`
binary on Linux, macOS, and Windows (including WSL).

## Development

```bash
npm install     # runs `prepare` -> `npm run build`
npm run build   # esbuild src/index.ts -> dist/index.mjs
```

The published tarball includes both `src/` and `dist/` so consumers can either
re-bundle or use the shipped bundle directly.

## Files

- `src/index.ts` — source (TypeScript, ESM)
- `dist/index.mjs` — built bundle (entry point, declared in `pi.extensions`)
- `package.json` `pi.extensions` — `./dist/index.mjs`

## Related projects

- [**palimpsest**](https://github.com/badlogic/palimpsest) — the Rust memory server itself (HTTP + stdio MCP, 17 tools).
- [**palimpsest-journal**](https://github.com/Blue-B/palimpsest-journal) — mirror your memory DB to a **git-backed markdown repo** so you can `git diff`, `git revert`, and PR-review what the AI learned.
- [**pi-mono**](https://github.com/badlogic/pi-mono) — the pi coding agent that hosts this extension.

## Documentation

- [INSTALL-CLIENTS.md](./INSTALL-CLIENTS.md) — register `palimpsest --mcp` in Claude Desktop, Cursor, Cline, Continue, Zed, opencode, pi.
- [SECURITY.md](./SECURITY.md) — threat model, audit checklist, secret rotation.
- [CHANGELOG.md](./CHANGELOG.md) — release notes.

## Contributing & support

Issues and PRs welcome at [github.com/Blue-B/pi-palimpsest](https://github.com/Blue-B/pi-palimpsest/issues).

If this saves you cloud-memory bills, consider [sponsoring](https://github.com/sponsors/Blue-B) to fund maintenance.

## License

MIT © Blue-B
