# Changelog

[English](CHANGELOG.md) | [한국어](CHANGELOG.ko.md)

All notable changes to the `memnest` Rust engine are recorded here.

## [Unreleased]

### Security

- Cross-site requests: an `Origin` is accepted only when it names the host and port the request was sent to (`localhost`, `127.0.0.1` and `[::1]` count as one host). Before, any IP address or any local port passed. Requests marked `Sec-Fetch-Site: cross-site` are refused, and `POST /mcp` answers 415 unless the body is `application/json`.
- Task cards: the `task-cards` and `task-cards-done` collections are reserved for the task-card path. `memory_remember`, `memory_update`, `/add` and `/update` can no longer create, edit or move a card, so a stored text cannot pose as a card or as the user's own words. Deleting a card still works. Titles are cut to 120 characters on one line with `<` and `>` replaced, and card text shown to a model cannot close the reminder wrapper. `POST /task-card` accepts user messages from a host that records them itself (the pi extension); the MCP tool does not.
- Vault: `/secrets` routes and `secret_*` tools over `POST /mcp` now need a bearer token even with no `MEMNEST_TOKEN` and no access policy. The service writes a per-install token to `<data-dir>/api-token` (mode 0600) and requires it in that mode. Stdio MCP is unchanged.
- The process sets umask 077 and makes the data directory mode 0700 on every start. Packaged systemd units set `UMask=0077`, and installers create the directory 0700.
- The system-mode systemd unit adds `StateDirectoryMode=0700` and `ProtectSystem=strict`. `/var/lib` stays writable because the writer lock is `/var/lib/.memnest.writer.lock`. Existing unit files are not rewritten on upgrade.
- Redaction also covers JWTs, Stripe `sk_live_`/`sk_test_`/`rk_live_`/`rk_test_` keys, GitLab `glpat-`, `npm_`, Hugging Face `hf_`, Slack `xapp-` and `xoxe-`, and AWS `ASIA` temporary keys.
- `MEMNEST_TOKEN` is compared in constant time.
- A card in the trash cannot be rewritten with `memory_update` or `/update` and then restored, and a heading in a saved card body that imitates the "User's own words" section is cut off with everything below it.
- Token patterns use ASCII word boundaries, so a token followed directly by Hangul (`hf_...입니다`) is redacted. The Slack app-token pattern needs the `xapp-<digit>-` shape, so package names such as `xapp-sn-watcher` are left alone.
- Dependency updates for RustSec advisories: `rusqlite` 0.41 with `libsqlite3-sys` 0.39 (bundled SQLite CVE), `zstd` 0.13.4 with `zstd-safe` 7.4, `quinn-proto` 0.11.19, `webpki-roots` 1.0.11, `crossbeam-epoch` 0.9.21, `tokio` 1.54, `anyhow` 1.0.104, `memmap2` 0.9.12. `cargo audit` reports no vulnerabilities; `lru` (unsound) and `paste` (unmaintained) remain as transitive warnings with no fixed release in the dependency tree.
- The read-only MCP gateway compares its URL secret in constant time, refuses `memory_remember`, `memory_update`, `memory_delete`, `task_card` and every `secret_*` tool even when `MEMNEST_GATEWAY_TOOLS` lists them, and removes `evidence_only` before forwarding a call.
- `--backup-dir` prints a warning that the backup contains the vault key. The key is still copied, so a restore keeps working.
- `memnest hook` and `memnest watch` refuse to send `MEMNEST_TOKEN` over plain HTTP to an address that is not loopback.

### Added

- `POST /recent` lists saved memories (chunk type manual) for a person's own UI, such as the pi extension's `/memnest-recent` and `/memnest-review`; it is not an MCP tool. It takes `project` or `cwd` (resolved as `/search` does), `rules_only` (importance `preference` or kind `rule`), `oldest_first` and `limit` (1 to 50, default 20), and filters and sorts in SQL, so transcript rows are never loaded. Newest first goes by the last save; `oldest_first` goes by the later of creation and last review. It never returns sensitive memories, the trash, superseded versions or task cards, and a caller with project grants gets only the projects it may read. Text is redacted as in search.
- `POST /reviewed {id}` records that a person confirmed a saved memory still holds, as `reviewed_at` in its metadata. It changes nothing else (not the text, embedding, `updated_at`, indexes or ranking) and applies the write checks of `/update`. `/add` ignores a `reviewed_at` it is given, and editing a memory's text clears the date.

- Retrieval benchmarks without any language model: a harness under `benchmarks/retrieval` (LoCoMo in full, a fixed 100-question sample of LongMemEval-S), per-question result files, a miss analysis, and the page [docs/benchmarks.md](../docs/benchmarks.md). The numbers are retrieval recall, not answer accuracy, and are not comparable to LLM-judged scores.
- A search that used the workspace scope (no `project` given) now says when other projects hold clearly stronger matches. One line is added to the result: it names up to three projects, gives the number of stronger saved memories in each, and says to search again with `project="<name>"` or `project="all"`. The scope itself does not change and no text from the other projects is returned. The line appears in the MCP `memory_search` text, as an optional `scope_hint` field in `/search` and `/context`, and inside the context pack used by `memnest hook`. It is not added when the caller named a project, asked for newest-first results, or is limited to some projects by an access policy. It never names the trash, superseded records, task cards, sensitive records, or a project the caller may not read. On 47 real prompts it appeared for 3; all 3 named the project the prompt was about, and for 1 of them the counted memories were the exact answer. It did not appear for any of the 24 vague prompts. Method and limits: `docs/search-relevance.md` (Korean).
- Saving a new task card answers with the rules and preferences already stored for that work, with their ids, so the model can copy what the user stated and what still applies into the card. A card used to hold only what was said after it was created; in real use a rule stored five days before the card was missed until the user repeated it. The list comes from the project most of the related memories live in, newest first, then from the search results, at most 10 lines.
- Saving a memory lists existing memories that may cover the same subject. `memory_remember` and `POST /add`, for a save without `supersedes`, reply with `same_subject`: up to 3 saved memories of the same project (id, date of last save, a redacted one-line 160-character excerpt), nearest first, and `same_subject_note`, one sentence that says the list is reference data and how to replace one: call `memory_update` on the new id with `supersedes=<old id>` and do not save the text again; leave them if they are about a different event. Nothing is changed automatically. Two memories count when their stored embeddings are within 0.035, or within 0.07 with at least 45% of the smaller set of topic keywords shared. On 272 replaced memories from a real store this rule was right for 95% of the pairs it picked and found 29% of the replacements; replaying the last 1,000 saves, 37 got a list (30 with one entry, 7 with two), and of 20 lists read by hand 13 named the same subject and 7 a different event on the same system. Not for transcript capture, task cards or a deduplicated save; the trash, superseded versions, task cards and sensitive memories are never listed, and read access is checked per row. The save compares against at most the 5,000 newest saved memories of the project using stored embeddings: 91 ms median at 5,000 (36 ms at 1,500). A mark on search results for memories that have a newer one on the same subject was measured and not added: on current memories it was right for 18 of 30. [Details](../docs/search-relevance.md) (Korean)
- The MCP `memory_update` tool accepts `supersedes=<id>`, as `POST /update` already did in its metadata patch: the memory being updated replaces that older one, which leaves search and stays readable by id.
- The `POST /context` reply carries `on_topic_only: true`, so a client can tell that its memories passed the on-topic filter.
- Native Windows install (64-bit x86, Windows 10/11) without administrator rights. `irm https://raw.githubusercontent.com/Blue-B/memnest/main/core/scripts/install.ps1 | iex` downloads the release zip, checks its SHA-256 and runs `install-windows.ps1`, which installs under `%LOCALAPPDATA%\memnest`, keeps data in `%USERPROFILE%\.memnest`, and registers two logon scheduled tasks: the server and `memnest watch`. See `docs/operations.md`.
- Releases publish `memnest-<version>-x86_64-pc-windows-msvc.zip` with a `.sha256`. It needs the Microsoft Visual C++ Redistributable (x64) and is not code-signed.

### Changed

- Search ranking: the order of results is now decided mainly by the two searches (meaning and keywords), not by how many of the question's words a memory contains. Before, the bonus for shared words (up to 0.15) was several times larger than the score from the two rank lists (at most 0.033), so a long text that repeated more of the question's words outranked the memory both searches put first. The rank lists are now fused with RRF k=10 with the keyword list at half weight, and a result's score is 0.10 x rank + 0.08 x share of question words, plus the unchanged importance, type and age terms. Measured without any language model, released 0.5.1 against this build, on data that was not used to choose the constants: LoCoMo conversations 5-9 (categories 1-4, 776 questions) R@1 39.8 to 42.9, R@5 60.2 to 68.7, R@10 69.5 to 76.8 (76 questions gained, 19 lost at R@10); a second LongMemEval-S sample of 100 questions, session R@1 83 to 94, R@5 96 to 98, R@10 98 to 99 (11 gained, none lost at R@1). On the data used for choosing them: LoCoMo conversations 0-4 R@10 68.8 to 79.1, first LongMemEval-S sample session R@1 83 to 89 and R@5 91 to 94. One figure got worse: LoCoMo's adversarial category, which is not part of the scored set, R@1 30.9 to 26.9 (its R@10 rose from 58.1 to 66.6). On a copy of a real store, 40 saved memories asked for by a question came back first for 16 questions instead of 13, and in the first three for 26 instead of 24. A memory that contains none of the question's words gets at most 0.049 from its rank, so it still cannot reach the 0.25 at which the pi extension injects a saved memory; a test holds this. Candidate pools, distance cutoff, MMR and the embedding model are unchanged; no index is rebuilt and no dependency was added. Tables and method: [docs/benchmarks.md](../docs/benchmarks.md) and `benchmarks/retrieval/ANALYSIS.md` section 8.
- Search ranking: a conversation turn gets a small boost when the turn right before or after it in the same conversation also matches the question, because the answer is often the turn next to the one that repeats the question's words. The turn's rank score gains 0.15 x the better of its two neighbours' rank scores, which is at most 0.015 of a final score; a memory with none of the question's words still gets at most 0.049 from its rank. Neighbours are turns of the same captured transcript (project, session, source and working directory); saved memories have none and keep their score. Measured without any language model against the previous build on the same stored data: LongMemEval-S, all 470 questions, session R@1 91.9 to 92.3, R@5 98.3 to 98.3, R@10 98.9 to 98.7; on the 270 of them that no earlier tuning looked at, R@1 92.2 to 92.6, R@5 100 to 99.6, R@10 unchanged. LoCoMo categories 1-4, conversations 5-9 (seen while choosing among variants): R@1 42.9 to 42.8, R@5 68.7 to 70.4, R@10 76.8 to 78.7; conversations 0-4: R@5 70.1 to 72.8, R@10 79.1 to 80.8. On a copy of a real store, 40 saved memories asked for by a question ranked exactly as before and no top-3 list changed; a search over all projects took about 5 ms longer (median 9 to 14 ms). Tables: `benchmarks/retrieval/ANALYSIS.md` section 9.4.
- Automatic recall (`/context`, used by `memnest hook`) now injects a memory only when it shares the prompt's topic. It used to inject the top three results even when none were related. A memory is kept when two topic words from the prompt appear in it and the embedding distance is at most 0.18, or one word and at most 0.13, or it is written in the other language (Korean or not), the prompt has at least two topic words and the distance is at most 0.21. Filler words such as "then", "now", "그러면", "지금" do not count as topic words. On 47 real prompts (23 specific, 24 vague), unrelated injected memories fell from 95 to 14 (2.0 to 0.3 per prompt), all 12 related memories were kept, and vague prompts that inject nothing went from 4 of 24 to 24 of 24. Weakly related memories fell from 10 to 4, and 6 of the 14 remaining unrelated memories are new ones that took a freed slot. When nothing passes, `memnest hook` adds no context at all. Explicit search (`memory_search`, `/search`) is not filtered.
- Documentation fixes: the pinned install example now uses v0.5.1, the supported-versions table lists 0.5.x, the adapters page counts six pi tools, and links to Korean-only documents are labelled as Korean.
- The task card summary in the README now states the replay result exactly (the policy was respected in 3 of 3 blind-graded runs with the card, 0 of 3 without) and notes that a card does not force the model to follow it.
- `install-windows.ps1` no longer installs an elevated WinSW service with data in `%ProgramData%\Memnest`; a service account cannot read the user's transcripts. `uninstall-windows.ps1` warns when such an install is still present, and `docs/operations.md` has the steps to move its data.
- Release archives contain only the scripts and service templates their own system installs with. Linux and macOS archives no longer carry PowerShell, test, and development scripts.

### Fixed

- A corrected memory is no longer dropped as a duplicate. A save whose text was nearly the same as an existing plain memory in the project (for example the same sentence with a changed port number) was answered with `deduplicated` and the id of the old memory, and the new text was not stored. Only an exact repeat of the text is a duplicate now. A near-identical text is stored, and the reply's `same_subject` list names the earlier memory so it can be marked as replaced.
- Search: a query with an apostrophe-s (`Caroline's`), a trailing period, a leading `-` or `+`, an open bracket or a trailing `AND`/`OR` was rejected by the keyword query parser, the error was dropped, and the search ran on vectors only and returned 3 results. Such a query is now searched again as plain words. In the retrieval benchmark this affected 330 of 1,986 LoCoMo questions; with the fix, LoCoMo recall in the first 10 results rose from 69.1% to 70.7%, while on the 17 affected LongMemEval sample questions the right session was ranked first for 11 instead of 16 (still within the first 3 for 16). Both are in [docs/benchmarks.md](../docs/benchmarks.md).
- Windows: `memnest evidence` used a different workspace id than hooks and MCP for the same directory, because the canonical path starts with `\\?\`.
- Windows: the default transcript folders of `memnest watch` are written with backslashes, so a later `--path` to the same folder identifies the same files.
- `setup-clients.py` on Windows reads client files as UTF-8 instead of the system code page, recognises `memnest.exe` as its own hook so a second run adds nothing, and writes hook and restore commands that Git Bash and cmd both run.

### Removed

- Unused code: the `SessionSummary` and `SearchResult` types, `Database::insert_summary`, `get_summaries` and `recent_chunks`, the embedder's `model_name`, `encode` and `encode_batch`, `TextIndex::add`, `remove` and `replace_all_with_project`, and the plain `crypto::encrypt` and `crypto::decrypt` (the row-bound `encrypt_bound` and `decrypt_bound` are unchanged, including the legacy-cipher fallback). `text_search` and `add_text_doc` now exist only in tests.
- New databases no longer create the empty `session_summaries` table. An existing table is left as it is, the same as `graph_edges`.
- The old `pi-extension/contrib` source installer, `scripts/preflight.sh`, two unreferenced architecture diagrams, an unreferenced pi-extension logo, and a commented-out `ort` line in `Cargo.toml`.

### Upgrade notes

- `pi-memnest` 0.5.0 saves cards through `/add`, which this core rejects. Update the extension together with the core.
- A script or MCP-over-HTTP client that uses the vault without `MEMNEST_TOKEN` must send `Authorization: Bearer $(cat <data-dir>/api-token)`.
- A client that posts to `/mcp` without `Content-Type: application/json` gets 415.
- A project named `task-cards` or `task-cards-done` can no longer be written with the memory tools.
- Search scores are computed differently, so the same query can return results in another order and with other scores. The range of scores is the same as before. A script that compares the score with a fixed number should be checked again.

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

See [history import](../docs/history-import.md), [search relevance](../docs/search-relevance.md) (Korean), and [following captures](../docs/following-capture.md) (Korean) for limits and validation.

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

See [the v0.3.0 summary](../docs/next-release.ko.md) (Korean) for user-facing changes since v0.2.1.

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
