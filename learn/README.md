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

### Closed loop (the feedback half)

L1–L5 store and retrieve; these make the store *get better every task*:

| Loop | What | Storage |
|------|------|---------|
| **Outcome reinforcement** | a recurrence signal ("still broken", "또 죽었어") raises the matching `failure`/`correction` memory one importance rung and stamps `[recurred ×N]`; a success signal validates the one that helped. Matching is cosine `/neighbors` over the last few turns. | memnest `/update` |
| **Skill self-improvement** | after a capture, each procedural learning refines the nearest existing skill (append a step) or drafts a new one — skills improve *during use*, not just at save time. | memnest `_skills` |
| **User model** | `preference` memories fold into a `_user_model` bucket of sharpening facets (refine-or-add, conflicts prefer the newer), injected first in the memory block. | memnest `_user_model` |
| **Assistant capture** | an `agent_end` hook also extracts from what the *assistant* said, so model-discovered failures/insights the user never restated are still learned. | memnest |
| **Budget** | a sliding-window cap throttles the automatic borrowed-model calls so background learning can't compete with the user's real work (manual tools are never gated). | — |

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
| `MEMNEST_PROJECT` | basename(cwd) | project bucket for captured memories (falls back to the working-directory name, then `default`; `_skills`/`_user_model` stay global) |
| `MEMNEST_LLM_MAX_CALLS` | `24` | max automatic borrowed-model calls per window |
| `MEMNEST_LLM_WINDOW_MS` | `300000` | budget window (5 min) for the call cap |

## Tools

`scratchpad` (add/done/undo/remove/list/clear) · `skill` (create/find/update) ·
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
bun test test/      # 34 tests, pure core (no pi / no network / no LLM)
bunx tsc --noEmit   # full typecheck incl. the pi-runtime wiring
```

Four levels of verification:

1. **Pure core** (capture, extraction parsing, consolidation, reinforcement
   signal+ladder, skill/user-model logic, LlmBudget, assistant-text extraction,
   KV-cache snapshot, working-memory, the HTTP client) — 34 unit tests with
   injected mocks (`bun test`).
2. **Live data path** — `test/integration.live.ts` + `test/loop.integration.live.ts`
   run capture -> categorised `/add` -> `/context` injection -> consolidation +
   `_superseded` -> reinforcement (`[recurred ×N]`) -> skill draft/refine ->
   user-model refine against a REAL throwaway engine (stub LLM). 11/11 + 12/12.
3. **Real-model quality** — `test/quality.live.ts` drives the ACTUAL
   extension prompts through a real model (`GEMINI_API_KEY`, clean
   OpenAI-compatible path, not `pi -p`) to judge extraction / skill / user-model
   quality. Confirmed: clean categorised extraction (incl. a quirk stated only
   by the assistant), skill self-improvement refines instead of duplicating,
   user model stays unpolluted, reinforcement hits the right failure.
4. **pi contract** — `src/index.ts` typechecks against the REAL
   `@earendil-works/pi-coding-agent` / `@earendil-works/pi-ai` types, so hook
   signatures, TypeBox tool schemas and the `complete()` call match the runtime.

Still open: a multi-day soak inside a live `pi` session (this extension is not
yet installed in any pi config); and a `/review` of the full diff.
