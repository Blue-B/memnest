<p align="center">
  <img src="./docs/logo.png" alt="pi-memnest" width="160" />
</p>

<h1 align="center">pi-memnest</h1>

<p align="center">
  <strong>One persistent memory layer for every AI tool you use — local, encrypted, free.</strong>
  <br/>
  <em>18 pi tools • risk-triggered autocontext • AES-GCM secret vault • BM25 + vector hybrid search • no cloud.</em>
</p>

<p align="center">
  <a href="https://www.npmjs.com/package/pi-memnest"><img src="https://img.shields.io/npm/v/pi-memnest.svg?style=flat&color=blue" alt="npm version" /></a>
  <a href="https://github.com/Blue-B/memnest/blob/main/LICENSE"><img src="https://img.shields.io/npm/l/pi-memnest.svg?style=flat&color=green" alt="license" /></a>
  <a href="https://github.com/sponsors/Blue-B"><img src="https://img.shields.io/badge/sponsor-❤-ea4aaa.svg?style=flat" alt="sponsor" /></a>
</p>

---

**pi-memnest** bridges [pi](https://github.com/badlogic/pi-mono) to a locally
running [memnest](https://github.com/Blue-B/memnest) memory server. Memories
you write from Claude Desktop, Cursor, Cline, pi, or curl all land in the
**same SQLite database** at `~/.memnest/memory.db` — and stay searchable
across every session of every tool, forever, for $0.

```bash
# 30-second demo
pi install npm:pi-memnest                  # in pi
memory_remember text="Project X uses port 8317 for CLIProxy"
memory_search   query="CLIProxy port"          # → returns the chunk above
memory_context  query="Project X deployment"   # notes + facts + memories
memory_update   id=manual_... text="Project X uses port 8320"
secret_set      key=github_pat value=ghp_...   # AES-256-GCM encrypted on disk
```

## Install

Memnest must be running (default endpoint `http://127.0.0.1:3111`, override
with `MEMNEST_URL`). Then:

```bash
pi install npm:pi-memnest        # from npm
pi install /path/to/pi-memnest   # or from a local checkout (auto-builds)
```

## Tools (18)

All backed by the memnest HTTP API:

| Tool | Purpose |
| ---- | ------- |
| `memory_remember`   | save a memory chunk (auto-routes preference/decision to `playbook`) |
| `memory_update`     | correct an existing memory by id and refresh indexes |
| `memory_search`     | hybrid BM25 + vector search |
| `memory_context`    | prompt-ready notes + facts + retrieved memories |
| `memory_stats`      | server statistics |
| `memory_sessions`   | recent session summaries |
| `memory_facts_list` | structured fact triples |
| `note_set` / `note_get` / `notes_list` / `note_delete` | core KV memory blocks |
| `secret_set` / `secret_get` / `secret_list` / `secret_delete` | AES-GCM encrypted credentials |
| `collections_list`  | enumerate project buckets |
| `memory_health`     | server liveness probe |
| `memnest_autocontext_status` | inspect automatic memory retrieval and test a live query |

Other MCP clients (Claude Desktop, Cursor, Cline, …) don't need this
extension — they register `memnest --mcp` directly, see the
[root README](../README.md#connect-your-client).

## Autocontext — tiny memory cards when they matter

Not a large startup memory dump: the default `balanced` mode only retrieves
memory on high-risk prompts (previous attempts, credentials, impossibility
claims, money or project strategy).

- `MEMNEST_AUTOCONTEXT_MODE`: `balanced` (default) · `aggressive` (adds first-turn/topic-shift lane) · `off`.
- Tunables: `MEMNEST_AUTOCONTEXT_N`, `_TOP`, `_MAX_INJECTIONS`, `_MIN_SCORE`, `_EXCLUDE`.
- Inspect counters or preview a retrieval with `memnest_autocontext_status`.

## AutoLog — passive memory capture

Event hooks automatically send user inputs and the assistant's final messages
to memnest as you work (skips thinking content, short noise, and memnest's own
tool calls). Fire-and-forget: never blocks the agent loop; pending writes are
drained on session end. Disable with `MEMNEST_AUTOLOG=0`.

## Development

```bash
npm install     # runs `prepare` -> `npm run build`
npm run build   # esbuild src/index.ts -> dist/index.mjs
```

pi loads the pre-built ESM bundle `dist/index.mjs` (not `src/index.ts`): the
bundle inlines everything except the host-provided
`@earendil-works/pi-coding-agent`, which avoids jiti/typebox resolution
failures under the Bun-compiled pi binary.

## Links

- [Root README](https://github.com/Blue-B/memnest#readme) — engine install, client registration, deployment, security, troubleshooting.
- [CHANGELOG.md](./CHANGELOG.md) — release notes.
- [memnest-journal](https://github.com/Blue-B/memnest) — mirror the memory DB to a git-backed markdown repo (diff, revert, PR-review).

Issues and PRs welcome at [github.com/Blue-B/memnest](https://github.com/Blue-B/memnest/issues).
If this saves you cloud-memory bills, consider [sponsoring](https://github.com/sponsors/Blue-B).

## License

MIT © Blue-B
