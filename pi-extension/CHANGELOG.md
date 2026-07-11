# Changelog

All notable changes to `pi-memnest`.

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
