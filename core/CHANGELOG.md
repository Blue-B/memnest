# Changelog

All notable changes to `memnest`, the Rust engine.

## Unreleased

### Fixed

- Escaped collection names on every dashboard render path. A crafted collection
  name could previously be injected into the page body, the `<title>`, and
  generated links.
- Excluded the internal `_trash` and `_superseded` buckets from dashboard
  totals, collection listings, recent saves, search scope, and collection
  detail. A soft-deleted memory could previously still be read there.

### Removed (breaking for HTTP consumers)

- Dropped the `/facts`, `/notes`, `/notes/{key}`, `/servers`, `/summary`,
  `/sessions`, `/collection/{name}/meta` mutation, `/reproject`, and `/compact`
  endpoints, along with the `--import-facts-json` flag. Their SQLite tables are
  left in place and untouched, so existing rows remain readable and are
  preserved for a future migration.
- Removed the unsupported `learn/` package, in-memory graph runtime,
  session-fork mutation endpoint, and learning-only neighbors endpoint. Existing
  memory rows, lineage metadata, and the legacy `graph_edges` table remain
  untouched for compatibility.

### Changed

- Canonicalized the model surface to six memory tools and four vault tools, with shared HTTP/MCP memory operations, scoped automatic recall, soft delete, targeted feedback, strict auth token normalization, and fail-closed secrets.
- `/context` now requires an explicit project, matching `/search`. It no longer
  falls back to a cross-project scan.

## [0.2.0] - 2026-08-17

First version to expose MCP over HTTP and to record memory without a per-host
extension. No release has been tagged yet, so every change below is still
reached by building from a checkout.

Release scope: archives are published for Linux x86_64 and aarch64. No Windows
archive is published, because nothing consumes one. `scripts/install.sh` refuses
a non-Linux host, and `scripts/install-windows.ps1` registers a binary you have
already built rather than downloading one.

### Added

- **MCP over Streamable HTTP.** `POST /mcp` answers `initialize`, `tools/list`,
  and `tools/call` with a single JSON response, so one running service covers
  the HTTP API, the dashboard, and MCP clients on the same port instead of each
  client spawning its own writer against the data directory. Notifications get
  202 with no body, `GET /mcp` is 405, and the route inherits the existing
  bearer authentication and security headers. The stdio transport is unchanged.
- **`memnest hook`.** Reads a host's prompt hook payload on stdin, asks the
  running service for a context pack, and answers in the shape that host
  expects: Claude Code's nested envelope when the payload looks like one of its
  events, plain text otherwise, or whatever `--format` pins. It never blocks a
  prompt, so an unreachable or slow service means no output, exit 0, and a line
  on stderr.
- **`memnest watch`.** Follows Claude Code, pi, and Codex transcripts, recognised
  by line shape rather than path. It stores redacted user and assistant text
  directly, excludes prompts and execution machinery, and splits long turns
  into ordered searchable chunks without truncation. Progress is a byte offset
  per file in `watch-state.json`, advanced only after storage succeeds or an
  idempotent retry is confirmed. `--backfill` imports existing history instead
  of following from the end.
- `recall_events` and `processing_jobs` tables with 90 day retention, holding
  redacted queries and status metadata only, never memory bodies or secrets.
- `/operations` and `/feedback` endpoints, a `recall_id` on every search, and
  adapter identity on writes and searches.
- Structured `record`, `fact`, `rule`, and `procedure` memory kinds with
  confidence, source ids, supersedes, and verified_at. Legacy metadata stays
  readable through serde defaults.

### Changed

- **`/add` returns `succeeded` or `deduplicated` instead of `queued`**, because
  the write is now complete before the response is sent. API consumers that
  matched on `queued` need updating.
- The dashboard renders English by default. Roughly 60 console, collection, and
  search strings had been hardcoded Korean, so the existing language toggle only
  ever translated the navigation bar. Both languages now render fully.
- The dashboard was rebuilt as an operations console covering recalls, latency,
  job state, failures, feedback, collection skew, and storage health.
- Helpful and harmful counts feed the ranking score through a saturating bonus
  bounded to the same scale as the importance and type bonuses.
- An unknown MCP method returns a JSON-RPC `-32601` error rather than an error
  object nested inside `result`, and `initialize` echoes a supported
  `protocolVersion` when the client asks for one.
- The default data directory moves to `~/.memnest` but keeps using an existing
  `~/.factory/memories` store, so an upgrade never hides existing data or the
  vault master key behind a new path.
- `scripts/install.sh` explains how to build from source when no release has
  been published yet.

### Fixed

- Semantic dedup no longer acknowledges an id that is never persisted. Aliases
  map the acknowledged id to the canonical memory, and `get_chunk` resolves
  them.
- A crash can no longer lose an already confirmed write, and jobs left queued or
  running by a restart are marked failed instead of appearing active forever.
- Context budgets count Unicode characters instead of UTF-8 bytes, which had
  been truncating Korean and other non-Latin text early.
- Feedback is idempotent and transactional: repeating an outcome does not double
  count, and changing it reverses the previous counter.
- `memnest status` probes the configured host rather than a fixed `127.0.0.1`,
  and brackets IPv6 hosts when printing the dashboard URL.

## [0.1.0] - earlier

Initial monorepo version: HTTP API, stdio MCP server, hybrid BM25 and vector
search, SQLite storage, and the secret vault.
