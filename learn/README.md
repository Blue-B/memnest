# memnest-learn

The **learning + working-memory + injection layer** on top of the
[memnest](../core) engine.

memnest (Rust) is a fast, private, cross-tool memory **engine** — hybrid search,
MMR, dedup, encrypted vault, no LLM at runtime. `memnest-learn` adds the
**intelligence layer** that competitors like `mem0` (LLM extract-and-update),
`pi-hermes-memory` (failure/correction learning, skills) and `jayzeng/pi-memory`
(KV-cache-stable injection, scratchpad/daily working memory) provide — **without
putting any LLM into the engine**. Every LLM call here borrows the *host agent's
own model*, so there is no extra API key, cost, or service.

```
[host agent LLM]──borrowed──┐
                            ▼
   pi hooks ─► memnest-learn (this package, TS)
                │  capture · consolidate · working-memory · KV-stable injection
                ▼ HTTP
   memnest engine (Rust): store + hybrid search + MMR + vault
```

## What it adds

| Layer | What | Borrowed idea | Storage |
|------|------|---------------|---------|
| **L1 Capture** | every N turns, the host LLM extracts durable memories → categorised (`failure`/`correction`/`insight`/`preference`/`convention`/`tool_quirk`) → `POST /add`. Corrections are saved immediately. | mem0 / pi-hermes | memnest |
| **L2 Working memory** | `scratchpad` checklist + daily logs + a **handoff written before context compaction** so in-progress state survives | jayzeng/pi-memory | local md + memnest `/summary` |
| **L3 Consolidate** | cluster near-duplicate memories (trigram similarity) → LLM merges each cluster → update survivor, **non-destructively retire the rest** (`_superseded` bucket) | mem0 | memnest `/update` |
| **L4 Inject** | build a budget-bounded pack from memnest `/context` + working memory, served as a **byte-stable snapshot** so local prefix caches (llama.cpp/vLLM/MLX) aren't busted every turn | jayzeng/pi-memory | — |
| **L5 Skills** | save/recall reusable procedures ("how") | pi-hermes | memnest `_skills` project |

The only engine change required is a `category` field on memory metadata
(already shipped in core); everything else reuses existing memnest endpoints.

## Install (pi)

```bash
pi install npm:memnest-learn      # or: pi -e /path/to/memnest/learn/src/index.ts
# requires a running memnest engine (default http://127.0.0.1:3111)
```

## Config (env)

| Var | Default | Description |
|-----|---------|-------------|
| `MEMNEST_URL` | `http://127.0.0.1:3111` | memnest engine base URL |
| `MEMNEST_LEARN_DIR` | `~/.pi/agent/memnest-learn` | working-memory files |
| `MEMNEST_CAPTURE_TURNS` | `10` | run background capture every N user turns |
| `MEMNEST_PROJECT` | `default` | project bucket for captured memories |

## Tools

`scratchpad` (add/done/undo/remove/list/clear) · `skill` (create/find) ·
`memory_consolidate` (dry-run by default).

## Design notes

- **Engine stays LLM-free.** All LLM use is in this client layer via the host
  model — the pattern `pi-hermes` proved viable. memnest itself needs no API key.
- **Non-destructive.** Consolidation retires duplicates into a `_superseded`
  project (reversible + auditable) rather than hard-deleting.
- **Cross-tool.** Because the store/search live in memnest (HTTP/MCP), memories
  captured here are visible to any memnest client, not just pi.

## Testing / status

```bash
bun test test/      # 14 tests, pure core (no pi / no network / no LLM)
bunx tsc --noEmit   # full typecheck incl. the pi-runtime wiring
```

Three levels of verification:

1. **Pure core** (capture, extraction parsing, consolidation/clustering,
   KV-cache snapshot, working-memory, the memnest HTTP client) — 15 unit tests
   with injected mocks (`bun test`).
2. **Live data path** — `test/integration.live.ts` runs capture -> categorised
   `/add` -> search -> `/context` injection -> `consolidateByEmbedding` merge +
   `_superseded` -> correction fast-path against a REAL throwaway memnest engine
   (deterministic stub LLM). 12/12 verified.
3. **pi contract** — `src/index.ts` typechecks against the REAL
   `@earendil-works/pi-coding-agent` / `@earendil-works/pi-ai` types (`tsc
   --noEmit` clean), so the hook signatures, the TypeBox tool schemas and the
   `complete()` call match the runtime API.

Still open: the full loop has not been run inside a live `pi` session with a
real model (capture/merge *quality* depends on the host model + prompt). The
data plumbing and the pi type contract are verified; the live-model smoke is
the remaining step.
