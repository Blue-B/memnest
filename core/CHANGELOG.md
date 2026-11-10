# Changelog

[English](CHANGELOG.md) | [한국어](CHANGELOG.ko.md)

All notable changes to the `memnest` Rust engine are recorded here.

## [Unreleased]

### Added

- Native Windows install (64-bit x86, Windows 10/11) without administrator rights. `irm https://raw.githubusercontent.com/Blue-B/memnest/main/core/scripts/install.ps1 | iex` downloads the release zip, checks its SHA-256 and runs `install-windows.ps1`, which installs under `%LOCALAPPDATA%\memnest`, keeps data in `%USERPROFILE%\.memnest`, and registers two logon scheduled tasks: the server and `memnest watch`. See `docs/operations.md`.
- Releases publish `memnest-<version>-x86_64-pc-windows-msvc.zip` with a `.sha256`. It needs the Microsoft Visual C++ Redistributable (x64) and is not code-signed.

### Changed

- `install-windows.ps1` no longer installs an elevated WinSW service with data in `%ProgramData%\Memnest`; a service account cannot read the user's transcripts. `uninstall-windows.ps1` warns when such an install is still present, and `docs/operations.md` has the steps to move its data.
- Release archives contain only the scripts and service templates their own system installs with. Linux and macOS archives no longer carry PowerShell, test, and development scripts.

### Fixed

- Windows: `memnest evidence` used a different workspace id than hooks and MCP for the same directory, because the canonical path starts with `\\?\`.
- Windows: the default transcript folders of `memnest watch` are written with backslashes, so a later `--path` to the same folder identifies the same files.
- `setup-clients.py` on Windows reads client files as UTF-8 instead of the system code page, recognises `memnest.exe` as its own hook so a second run adds nothing, and writes hook and restore commands that Git Bash and cmd both run.

## [0.5.1] - 2026-11-09

### Security

- Update rustls to 0.23.46 (and rustls-webpki to 0.103.16) for four rustls advisories: TLS 1.3 handshake messages accepted across encryption changes, session `Debug` output exposing secrets, an unbounded allocation in Brotli certificate decompression, and a ServerHello that could poison the key-exchange cache. memnest uses rustls for its HTTPS connections, such as downloading the embedding model.

## [0.5.0] - 2026-11-08

### Security

- Without a token, refuse browser cross-site and DNS-rebinding requests: the `Host` and `Origin` must be an IP address, `localhost`, or a name listed in `MEMNEST_ALLOWED_HOSTS` (comma-separated).
- cwd-scoped search is read-only, so a reader cannot drop a workspace's legacy recall. cwd length, search query length, and MCP batch size are capped.
- `memnest hook` and `memnest watch` never send traffic through an ambient proxy or follow a redirect.
- Backups and restores are owner-only (0700 directory, 0600 `memory.db`).
- Only the newest 1,000 unauthenticated audit rows are kept, and they stay out of `/audit`.
- Imported vectors of the wrong dimension are re-embedded, and imported text is redacted.
- Redaction also covers prefixed environment names, JSON-quoted keys, Bearer tokens, URL passwords, and `github_pat_` tokens.
- The read-only MCP gateway has an error boundary, a 1 MiB body cap, rejects non-object bodies, and times out upstream calls.

### Added

- Task cards for every client. A card is a handoff note for multi-day work; see `docs/task-cards.md`.
  - The MCP `task_card` tool and `POST /task-card` save, list, open, finish, and close cards.
  - `memnest hook` handles `SessionStart` and `UserPromptSubmit` for Claude Code and Codex, through `POST /task-card/hook`. It lists cards on a session's first prompt and records each user message verbatim into the open card. After compaction it brings back the rule and the user's latest words.
  - With no cards, the first prompt says when to save one, including when the user resumes earlier work, and an empty `list` asks for a card instead of saying "treat this as a new task". A compaction with no card open reminds once per session.
  - A card open but not saved for 8 user messages gets one reminder to close it if the conversation moved on, or save it.
  - `memnest hook --no-recall` runs task cards without automatic recall.
  - Setup installs that hook for Claude Code (`~/.claude/settings.json`) and Codex (`~/.codex/hooks.json`) by default, and lets Codex call `task_card` without an approval prompt. `--autocontext` still opts in to recall.

### Fixed

- On macOS 15, reinstalling or upgrading failed with "Bootstrap failed: 5: Input/output error" because `launchctl bootout` returns before the job is removed. The installer and uninstaller now wait for the job to go, and the installer retries `bootstrap`.

### Compatibility and limits

- MCP lists six tools by default, with `task_card` after the five memory tools.
- Open cards per session live in core memory and are forgotten on restart; the next prompt lists cards again.
- Codex runs new hooks only after you trust them once with `/hooks`.

## [0.4.3] - 2026-10-31

### Added

- Apple Silicon macOS archive (`aarch64-apple-darwin`), installed as launchd agents by the same `install.sh`. Core CI installs, reinstalls, restarts and removes it on a macOS 14 runner. It is not signed or notarized, and Intel Macs still need a source build.

### Fixed

- Default setup no longer requires Python 3.11. On Python 3.10 (Ubuntu 22.04's default) it configures Claude Code and Cursor and leaves an existing Codex `config.toml` unchanged, because it cannot validate that file without `tomllib`; setup prints the line to add by hand.

## [0.4.2] - 2026-10-13

### Fixed

- The x86_64 Linux archive runs on glibc 2.35 or newer (Ubuntu 22.04, Debian 12 and later). v0.4.0 was built on Ubuntu 24.04 and failed to start on older systems with `GLIBC_2.38 not found`. It now builds on Ubuntu 22.04, and the prebuilt onnxruntime's C23 `strtol` family references are forwarded to the classic functions.
- The release job fails if a binary requires a newer glibc than its documented floor.

### Limits

- The aarch64 archive still requires glibc 2.39 (Ubuntu 24.04 or later). Its prebuilt onnxruntime needs GCC 14's libstdc++ at runtime and crashed at startup with Ubuntu 22.04 libraries.
- v0.4.1 was tagged but never published because its aarch64 build failed.

## [0.4.0] - 2026-10-07

### Added

- Optional named-principal bearer policy with exact project read/write grants, content-free access audit, protected-store reopen checks, and per-project retention. ID reads, history, context, mutations, restoration, and vault/statistics/audit paths share authorization boundaries.
- Explicit code evidence capture/compare commands and bounded file fingerprints. File changes are not solution invalidation; unchanged files are not verification.
- Caller-reported approach outcomes and immutable replacement through `supersedes`.
- Opt-in `evidence_only` source selection using a configured loopback Ollama model, constrained source excerpts, and exact quotation checks. Default search remains generation-free.
- `POST /purge` for already-trashed records and administrator `POST /retention` for immediate lifecycle checks.

### Changed

- Plaintext archive copies are off by default; `MEMNEST_ARCHIVE=1` is explicit opt-in. Existing archives, backups, external transcripts, and physical disk copies are not erased.
- Secret lists omit free-form notes because they can contain sensitive values. Explicit secret retrieval remains administrator-only in policy mode.

### Limits

- Single-host team controls, not SSO, SaaS tenant administration, HA, or compliance certification. Local file owners remain trusted.
- Model-selected excerpts remain unverified. The small local comparison did not establish superior retrieval accuracy; see the documented dataset, model stack, timings, and continuation.

## [0.3.1] - 2026-09-28

### Fixed

- Explicit `watch --backfill --once --path /original/absolute/path` replays selected history without rewinding the live watcher. `--dry-run` previews eligible chunks without storing them. Same-source replay preserves existing deletion and deduplication rules; eligible does not mean missing.
- Mixed Korean/Latin searches no longer fill named matches with candidates that lack every explicit Latin identifier. Semantic fallback remains when no named candidate exists; relevant name-free records can still be missed.

### Added

- A bounded `following_id` hint on at most one retrieved user capture, so clients can read a later assistant source with `memory_get` even when its text omits the searched name. Search result order and count remain unchanged. Capture order is not a verified reply relationship or a guarantee of a final answer.
- Status explicitly states that past-history completeness has not been checked.

### Compatibility

- No database reset, automatic history replay, new dependency, or additional generative model call is required.
- Linux x86_64 and aarch64 archives remain the release targets. macOS remains source-only; Windows builds are covered by CI but no Windows archive is published.

See [history import](../docs/history-import.md), [search relevance](../docs/search-relevance.md), and [following captures](../docs/following-capture.md) for limits and validation.

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
