# Changelog

[English](CHANGELOG.md) | [한국어](CHANGELOG.ko.md)

All notable changes to `pi-memnest` are recorded here.

## [Unreleased]

## [0.5.0] - 2026-11-08

### Added

- Task cards, on by default. They add the `task_card` tool to save, list, open, and finish handoff cards for work that spans sessions. Active cards live in a shared `task-cards` collection and finished ones in `task-cards-done`. Saving the same title supersedes the previous card. The extension appends every user message to the open card verbatim, outside the model's summary. Up to three opened cards are pinned into the system prompt on every run with a fixed user-policy-first line; the pinned text is re-rendered only when first pinned and after a compaction, so it does not break the prompt cache between prompts. A new session lists up to 20 active titles ranked by its first request; it says when to save a new card, including when other cards exist. A user message is written into the open card when the run settles, so a card opened by mistake or closed because the user moved on does not keep it. The first prompt after a compaction asks the model to save what changed; with no card open, it reminds once per session to save one if the work continues later. The first `memory_remember` of a session with no card open gets one line asking for a card when the fact belongs to work that spans sessions. A card open but not saved for 8 user messages, counted across a resume, gets one reminder to close it if the conversation moved on, or save it. `/memnest` lists the active cards with their last save in local time and the cards this session records into. Measurement and limits are in `docs/task-cards.md`.

### Compatibility and limits

- The default surface grows to the five memory tools plus `task_card`, and one prompt hook. It lists cards on a session's first prompt and pins an opened card. Autocontext stays opt-in.
- Cards are found through hybrid search on a marker word, at most 50 per query; a topic search reaches cards beyond the newest 50. Session-start cost grows with active cards (about 0.43 s at 1,020 active, 3 ms at 20 active plus 1,000 finished), and lists with 50 or more active cards ask the model to offer cleanup.

## [0.4.0] - 2026-10-07

### Added

- `memory_remember(code_files=[...])` explicitly captures bounded file fingerprints through the matching core CLI, without storing file contents.
- `memory_get(check_code=true)` explicitly compares a saved baseline in the current workspace, including remote-store baselines through a network-free local compare command.
- `memory_search(evidence_only=true)` forwards optional local-model source selection and preserves its unverified assertion.
- Optional `MEMNEST_ALLOWED_MODEL_PROVIDERS` blocks new memory/credential reads and automatic recall for unapproved or unknown model contexts before network access.

### Compatibility and limits

- Five memory tools remain the default surface; automatic recall and code checks remain opt-in.
- Provider allowlisting is a trusted-client routing safeguard, not endpoint attestation, retroactive history erasure, or a complete DLP boundary.
- New source/evidence options require core 0.4.0. Existing basic operations remain compatible.

## [0.3.1] - 2026-09-28

### Added

- Show the core's optional `following_id` in search results with a warning that it is a later assistant capture, not a verified reply. Read that source with the existing `memory_get` tool.
- Keep compatibility with cores that omit the hint; existing result excerpts and search limits are unchanged.

This release also includes the 0.3.0 changes below, which were not previously published to npm.

## [0.3.0] - 2026-09-15

### Changed

- Autocontext is off by default. Only an explicit `balanced` or `aggressive` mode enables it; unrecognized modes fail closed. The five memory tools and conversation capture remain available independently.
- `/memnest` reports the pi Autocontext state.
- Opt-in memory cards include ID, project, and creation time for source verification. Missing IDs/projects and missing or nonnumeric/nonfinite scores are no longer accepted.
- `memory_get` accepts offsets, document budgets and neighboring captures, and returns JSON text with continuation and provenance. Responses from an older core are bounded client-side and marked `paging: "client"`; neighboring captures require the updated core.
- Search results include `doc_len` while retaining their existing excerpts.

### Validation

- Added default-off negative cases for connection checks, ordinary edits, translation and changed decisions, plus opt-in checks for malformed results, superseded memories and workspace boundaries. These test retrieval policy, not embedding or answer accuracy.

## [0.2.0] - 2026-08-31

### Changed

- Autocontext requests `durable_only` search and filters client-side: injected cards only contain deliberate or consolidated memories. Raw transcript capture stays available through explicit `memory_search` calls.

## [0.1.0] - 2026-08-30

First public npm release.

### Added

- Five canonical memory tools: `memory_remember`, `memory_search`, `memory_get`, `memory_update`, and `memory_delete`.
- Four opt-in vault tools backed by the memnest core's encrypted secret store.
- Workspace-scoped Autocontext for prompt-time semantic recall.
- A `/memnest` command for service health, memory count, active data directory, and search latency.
- A bundled Node.js 20 ESM extension with a six-file npm package.

### Behavior

- Omitted projects use pi's absolute working directory so the core can isolate workspaces and include the shared `playbook` collection.
- Autocontext searches substantive prompts in their original language and injects only results that pass its score threshold.
- Retrieved text is labeled as untrusted reference data and stored markup is escaped before prompt injection.
- Conversation capture remains in the core's `memnest watch` command, avoiding duplicate extension-side AutoLog hooks.
- Search output omits the removed `recall_id` field instead of displaying an undefined value.

### Security

- Vault tools are disabled unless `MEMNEST_EXPOSE_SECRET_TOOLS=1` is set before pi starts.
- Sensitive values are rejected by `memory_remember` and must use the vault.
- HTTP failures and unavailable vault crypto fail closed.
