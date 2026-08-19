# Changelog

All notable changes to `pi-memnest`.

## Unreleased

- Canonicalized the model surface to six memory tools and four vault tools, with shared HTTP/MCP memory operations, scoped automatic recall, soft delete, targeted feedback, strict auth token normalization, and fail-closed secrets.

## Unreleased

- Raise the default Autocontext score thresholds from `0.12` to `0.25` so weak, unrelated retrievals are not injected into prompts. Both values remain configurable with environment variables.

## [0.6.0] - 2026-08-01

### Added

- `memory_feedback` records helpful, harmful, or ignored outcomes for a `recall_id` returned by search.
- `/memnest` reports local service health, memory count, data directory, and the dashboard URL.
- Structured `record`, `fact`, `rule`, and `procedure` memory kinds with optional confidence, provenance, replacement, and verification fields.
- Adapter identity on writes and searches so pi activity is visible in the operations console.
- `MEMNEST_TOKEN` bearer authentication for normal tool calls and Autocontext.

### Changed

- Total exposed tools: 19 to 20.
- Preferences and decisions default to rule memories, while knowledge defaults to fact memories.
- Search output includes `recall_id` without returning the full HTTP envelope.

## [0.5.2] — 2026-07-11

### Fixed — retrieval quality guardrails for the 0.5.1 token cuts

- `memory_search`: per-document clip 350 → 600 chars (76% of curated memories exceeded 350,
  and the actionable tail was being silently cut). Clipped results now end with an explicit
  `…[+N chars — memory_get <id>]` marker instead of reading as complete.
- New `memory_get(id)` tool + `GET /chunk/{id}` endpoint — full redacted document (8,000-char
  bound), the escape hatch for every excerpt. `/search` responses now carry `doc_len` so
  clients can detect clipping.
- `memory_search` appends up to 5 one-line stubs after the top results, making rank n+1
  visible so the model can re-query instead of silently losing recall at n=3.
- `memory_context`: memories (query-relevant) render before notes/facts, so static sections
  can no longer evict them under the 2,000-char budget.
- Semantic dedup scans all k neighbours for a same-project duplicate (was: first only).
- learn: no more mid-session snapshot rebuilds (markDirty on capture/reinforce/correction) —
  each rebuild changed the system prompt bytes and invalidated the whole prompt cache.
- Autolog default OFF (opt-in via MEMNEST_AUTOLOG=1); session_compact summaries fixed
  (chunk_type "session_summary" 422'd silently — now "consolidated") and routed to the
  project bucket instead of the excluded root bucket; MAX_INJECTIONS default 6 → 4.
- One-time migration: 94 curated chunks stranded in root (44 decisions) moved to
  playbook/shell (log: ~/memnest-root-migration-20260711.json).

## [0.5.1] — 2026-07-11

### Changed — token-efficiency overhaul

- `memory_search`: default `n_results` 10 → 3; compact text output (project/score/id + 350-char
  doc) instead of raw JSON passthrough; cross-project searches send `exclude_reserved` so the
  server drops the `root`/`default`/`global`/`_superseded` autolog buckets at the candidate
  level (pass `project="root"` explicitly to read transcript autologs).
- `memory_context`: defaults shrink to `n_results=3, max_notes=4, max_facts=4, max_chars=2000`;
  returns only the rendered prompt (not the full JSON envelope); infers the project from cwd
  when omitted, falling back to `all` (never the reserved `default` bucket).
- Autocontext: separate `MEMNEST_AUTOCONTEXT_RISK_MIN_SCORE` (default 0.12) for risk-trigger
  turns so a strict general `MIN_SCORE` no longer silences risk recall; default exclude list
  now includes `global`.
- Core server: `/search` default `n_results` 10 → 3, `/context` defaults 6/12/8/6000 →
  3/4/4/2000, new `SearchRequest.exclude_reserved` flag, MCP tool schemas updated to match.
- memnest-learn session snapshot slims: profile max 3 facets, playbook rules 4×260 chars,
  project-scoped context pack (1800 chars) — measured ~1220 → ~550 tokens per session.

## [0.5.0] — 2026-07-03

### Added

- `memory_update` — correct existing memories by id and refresh indexes.
- `memory_context` — prompt-ready context pack combining notes, facts, and retrieved memories.
- `note_set`, `note_get`, and `note_delete` — pi-side access to core key-value memory blocks.
- **Autocontext** — tiny, risk-triggered memory cards for pi. The default
  `balanced` profile avoids large startup dumps and only retrieves memory for
  prompts involving previous attempts, credentials, impossibility claims, or
  money/project strategy. `MEMNEST_AUTOCONTEXT_MODE=aggressive` restores the
  first-turn/topic-shift lane.
- **AutoLog** — automatic conversation capture. Hooks pi session events and
  fire-and-forgets user inputs and assistant final messages to memnest, so
  memory is built passively without explicit `memory_remember` calls.
  - Maps `cwd` → project at `session_start` for correct collection routing.
  - Skips `thinking` content and very short/noise messages; truncates large
    tool-result bodies before sending.
  - Never blocks or throws into the agent loop; drains in-flight writes on
    `agent_end` / `session_shutdown` (important for `pi -p` print mode).
  - Does not log memnest's own tool calls (avoids feedback chatter).
  - Can be disabled via environment variable for tool-only mode.
- `memnest_autocontext_status` — inspect Autocontext counters and preview a retrieval.

### Changed

- Total exposed tools: 12 → 18.

## [0.4.0] — 2026-05-17

### Added

- `memory_health` tool — checks memnest server liveness (GET /health).
- `collections_list` tool — enumerates all project buckets with counts and
  metadata (GET /collections).
- `secret_delete` tool — removes a stored credential by key (DELETE /secrets/:key).
- `LICENSE` (MIT) and `CHANGELOG.md`.

### Changed

- Total exposed tools: 9 → 12.

## [0.3.0] — 2026-05-16

### Added

- Pre-built ESM bundle at `dist/index.mjs` (esbuild, target=node20,
  typebox inlined, `@earendil-works/pi-coding-agent` kept external).
- `prepare` script for automatic build on `npm install`.
- `package.json` `pi.extensions` → `./dist/index.mjs`.

### Fixed

- `definition.execute is not a function` regression caused by jiti
  intermittently dropping `async method() {}` shorthand bodies in
  Bun-compiled pi binary environments. Bundling the extension as plain ESM
  bypasses the jiti TS transform entirely.

### Changed

- `engines.node` raised to `>=20`.
- `@sinclair/typebox` moved from `peerDependencies` to `devDependencies`
  (inlined into the bundle).

## [0.2.0] — earlier

Initial public version (pi tool wrapper around memnest HTTP API).
