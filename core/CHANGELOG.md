# Changelog

All notable changes to the `memnest` Rust engine are recorded here.

## [0.3.0] - 2026-09-15

### Added

- Paged `memory_get` over HTTP and MCP: Unicode offsets, a shared document-text budget, continuation offsets, and up to five captures before and after a transcript record in the same project, session, source, and cwd.
- Selected source provenance with a separate 2048-character budget per record, at most 16 source IDs, and an explicit `provenance_truncated` flag. Stored text and metadata are not modified by reading.
- Linux setup with client detection, private exact backups, conflict-aware restore, prerequisite checks, watcher startup, and a scratch remember/search round trip. Automatic recall remains opt-in.
- Read-only `status --diagnose` checks bounded health and MCP capability contracts. Health now reports embedding load state and index queue/rebuild state without initializing the model.
- A pinned MemoryBench adapter, small Korean retrieval fixture, expanded fixed retrieval evaluation, and reproducible independent-client demo. These are not answer-accuracy or provider-comparison benchmarks.

### Changed

- A lexical result no longer removes all vector-only candidates when result capacity remains. Common English question words are not treated as keyword evidence. The embedding model and learned ranking were not changed.
- Permanently deleted identified transcript events leave content-free tombstones so watcher replay does not recreate them after garbage collection or index rebuild.

### Fixed

- Missing or inactive `supersedes` targets now return a specific 404 or 409 instead of a redacted internal-error 500, without hiding the current record.
- Server logs stay on stderr so stdio MCP stdout remains valid JSON.

### Compatibility

- Search still returns at most 600 characters per result and has no aggregate search text cap, but candidate admission can include a strong semantic result that older versions discarded.
- MCP `memory_get` now returns JSON text, including continuation and provenance, instead of the former compact header and document. Consumers parsing the old display text need updating.
- Transcript neighbors follow capture order, not source chronology or final decisions. Existing stores do not need deletion or recapture.
- macOS launchd and packaging code remains source-only until native installation, restart, and removal are validated.

See [the v0.3.0 summary](../docs/next-release.ko.md) for user-facing changes since v0.2.1.

## [0.2.1] - 2026-09-01

### Fixed

- Removed a user-systemd ordering cycle that could prevent dependent services from starting during login.
- Installers no longer advertise the removed Dashboard, a bare 404 URL, or an unpublished Windows archive; Linux output names the live MCP endpoint.
- The legacy contrib installer now reuses the actual `memnest` binary path and installs the published pi extension instead of rebuilding it from source.

## [0.2.0] - 2026-08-31

### Added

- `memnest status` reports the conversation watcher's heartbeat and the last transcript capture, warning when the heartbeat is stale or missing. The watch loop records both in its state file, so the check works for systemd, launchd, and manually started watchers alike.

### Changed

- `/search` accepts a `durable_only` flag and the `/context` pack sets it: automatic context only admits deliberate or consolidated memories. Raw transcript AutoLog remains available to explicit searches for questions about earlier conversations.

## [0.1.0] - 2026-08-28

First public release.

### Added

- Local multilingual retrieval using e5 embeddings, Tantivy BM25, HNSW vector search, Reciprocal Rank Fusion, and MMR diversity ranking.
- SQLite as the source of truth, with transactional index jobs, startup repair, atomic index replacement, corrupt-sidecar detection, and an exclusive writer lock.
- Five memory operations for storing, searching, updating, deleting, and inspecting workspace-scoped memories. The built-in `playbook` collection supplies cross-project recall.
- Streamable HTTP MCP at `/mcp`, the JSON HTTP API, and a stdio MCP mode backed by the same operations.
- `memnest hook` for prompt-time context and `memnest watch` for redacted transcript capture from pi, Claude Code, and Codex.
- Structured `record`, `fact`, `rule`, and `procedure` memory kinds, including confidence, source IDs, verification time, and atomic `supersedes` corrections.
- Backup, restore, validation, health, statistics, support-bundle, and embedding warmup commands.
- Linux service installation for user and system units, plus WSL and Windows service-registration scripts.
- Linux release archives for x86_64 and aarch64, each published with a SHA-256 checksum.

### Security

- Credential-shaped text is redacted before searchable storage.
- Secrets use a separate AES-256-GCM vault and model-facing vault tools are disabled unless `MEMNEST_EXPOSE_SECRET_TOOLS=1` is set.
- Automatic context is marked as untrusted reference data and stored markup is escaped before prompt injection.
- Workspace identities use path-derived hashes, ambiguous legacy aliases fail closed, and project filtering happens during candidate generation.
- New vault ciphertext is bound to its row identity, master keys are created with private permissions, and backup restoration validates staged data before replacement.

### Reliability

- Confirmed writes are durable before the API acknowledges them, and interrupted index updates replay at startup.
- Corrections hide superseded values from every visible search path without retaining new raw transcript chunks.
- A second process cannot write the same data directory concurrently.
- Hook failures never block the host prompt.

### Compatibility

- Running the Linux binary requires no Rust toolchain. The embedding model downloads on the first write or search, or explicitly through `memnest --warmup-embedding`.
- Windows and WSL scripts register a binary built or supplied by the user. This release does not publish a Windows binary archive.
