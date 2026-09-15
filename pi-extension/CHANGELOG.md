# Changelog

All notable changes to `pi-memnest` are recorded here.

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
