# Changelog

All notable changes to `pi-memnest`.

## Unreleased

### Removed

- Removed the `memory_feedback` tool. Nothing ever recorded an outcome, so the ranking signal behind it was always zero. The default model surface drops from six memory tools to five.
- Search results no longer carry a `recall_id`. The core stopped recording query text, so there is no stored lookup left to point at.

### Changed

- `/memnest` no longer prints a dashboard link, because the core no longer serves that page. It reports search latency instead.

- Default model surface is now five memory tools. The four vault tools require `MEMNEST_EXPOSE_SECRET_TOOLS=1`.
- Omitted projects send pi's absolute `cwd`, letting the core isolate same-basename workspaces and include `playbook` safely.
- Autocontext labels all results as untrusted reference data, marks AutoLog as conversation evidence, and escapes stored markup before injection.
- Autocontext now searches every substantive prompt in its original language and lets the semantic score gate decide whether to inject. The English/Korean keyword rules, risk lane, topic-overlap lane, and their three environment variables are removed.

## [0.7.0] - 2026-08-20

### Breaking

- **Total exposed tools: 20 to 10.** The model surface is now exactly six memory
  tools (`memory_remember`, `memory_search`, `memory_get`, `memory_update`,
  `memory_delete`, `memory_feedback`) and four vault tools (`secret_set`,
  `secret_get`, `secret_list`, `secret_delete`). Everything else was removed,
  including `memory_context`, `note_set`, `note_get`, `note_delete`, and the
  remaining status and admin tools. A prompt or script that called a removed
  tool by name now fails. `/memnest` covers status, and the Autocontext hook
  covers automatic recall, without either occupying a tool slot.
- Vault tools fail closed. They surface an error rather than a plaintext value
  when the core cannot use its key.

### Changed

- Memory operations share one HTTP/MCP path, with scoped automatic recall, soft
  delete, targeted feedback, and strict auth token normalization.
- Raise the default Autocontext score thresholds from `0.12` to `0.25` so weak,
  unrelated retrievals are not injected into prompts. Both values remain
  configurable with environment variables.
- Conversation capture is `memnest watch` only. The extension installs no
  AutoLog event hooks, so pi turns are stored once.
- The published package now ships only `dist/`, the README, the changelog, the
  license, and third-party notices. `src/`, `test/`, `contrib/`, and `docs/`
  stay in the repository.
- Dropped the optional `@earendil-works/pi-coding-agent` peer dependency. No
  source file imports it, and declaring it polluted `package-lock.json` with
  absolute paths from whoever ran `npm install` last.

### Fixed

- Updated the build-only `esbuild` dependency to `0.25.12`, which fixes
  GHSA-67mh-4wv8-2f99 in its development server. The published bundle does not
  include the development server.

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
