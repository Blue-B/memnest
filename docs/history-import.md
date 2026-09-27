# Explicit history import

## Problem and implementation plan

The normal watcher follows newly discovered files from their end. A tracked
file's cursor therefore does **not** prove that its earlier conversation was
stored. Previously `watch --backfill --once` reused that cursor and could finish
successfully without importing that historical prefix. It also stopped after
one bounded read cycle, even when the file had more history.

Keep continuous capture unchanged. Make explicit one-shot import independent of
its cursor, and reuse the existing parser, redaction, event identity, `/add`,
indexing and deletion tombstones. No new memory schema, ranking algorithm,
dependency or generative model is needed.

## Preview, then import

Use the **same original absolute path** used by capture, not a moved copy or an
alternate spelling of that path:

```sh
memnest watch --path /absolute/path/session.jsonl --backfill --once --dry-run
memnest watch --path /absolute/path/session.jsonl --backfill --once
```

`--path` is required for one-shot replay; a directory selects its JSONL files
recursively. Repeat the flag to select multiple paths. Preview makes no HTTP
requests and creates no watch state. Neither mode reads or writes the running
watcher's `watch-state.json`, so an import cannot rewind or overwrite its cursor.
A missing/unreadable selection fails instead of silently claiming success.

The JSON report contains:

- `eligible_turns` per file and `eligible_chunks`: visible conversation text
  accepted by the existing parser, after redaction and chunk splitting.
- `new_chunks`: chunks newly accepted by the service during import.
- `skipped_chunks`: acknowledgements for already known or deleted event IDs.
- `scanned_bytes`: complete-line boundary selected at the start of this run.

**Eligible is not missing.** Preview does not inspect DB coverage. Skipped does
not mean visible: a deleted record is intentionally not restored. `status`
reports watcher activity, not completeness of past history.

One-shot import reads bounded batches until every selected complete prefix is
processed, rather than stopping after one 4 MiB cycle. An incomplete final line
and later appends belong to subsequent capture/replay. A service or file-read
failure exits nonzero; earlier chunks might already be stored. Retry the same
source from the beginning: stable event IDs make that retry idempotent.

## Boundaries

- Normal `watch` and continuous `watch --backfill` keep their existing behavior.
  The latter reads **newly discovered** files from the start and resumes tracked
  ones; use `--once --backfill` for an explicit complete replay.
- This is not an automatic import of all historical files. The operator selects
  the sources and service (`--url`, or the existing service URL default).
- User/assistant visible text is captured, not tool traffic, reasoning, system
  prompts, images, or excluded sidechains. Existing malformed/oversized-line
  warnings still apply. A report is not proof that every JSONL byte was stored.
- Existing IDs depend on source path, byte offset, project and chunk sequence.
  Moving, aliasing or rewriting a source can change identity. Do not use a copy
  or different path spelling to replay deleted records. This change does not
  migrate older relative-path identities or promise cross-copy deduplication.
- The service's existing deletion/tombstone support is required. Import into an
  older incompatible service is not a deletion-safety guarantee.
- Import time is not the historical event time. This change does not infer dates
  or validate an old assistant's claims, and does not alter search ranking.

## Validation plan

1. Unit checks: preview never calls HTTP; replay traverses multiple read batches;
   unfinished final lines are deferred; API failure rejects the run; reports
   distinguish newly accepted and skipped chunks; explicit paths are required.
2. Real CLI/HTTP regression: first let normal watch skip a fixture's history,
   reproduce the old binary's ineffective backfill, then preview and import it.
   Require no state/source changes, correct retrieval, no duplicate rows on
   replay, and no resurrection after deletion.
3. Existing lifecycle regression: verify deletion after actual scheduled trash
   GC, service restart and index rebuild still blocks backfill resurrection.
4. Real-history check: copy only active memories and workspace scope into a
   private isolated DB. Freeze the missing source and recall questions before
   running. Require all parser-eligible chunks, unchanged source/cursor, zero
   additional rows on replay, and original source retrieval in the top five for
   each selected question, followed by `memory_get` evidence inspection.
5. Preserve any later manually curated answer already in the snapshot; do not
   count its retrieval as recovery of the missing original source. Do not claim
   answer-model accuracy, speed superiority, or advantage over competitors.

```sh
cargo test --offline --manifest-path core/Cargo.toml --lib watch::tests
cargo test --offline --manifest-path core/Cargo.toml --bin memnest history_preview
cargo build --offline --manifest-path core/Cargo.toml
python3 scripts/test-history-import.py --old /absolute/path/to/pre-change/memnest
python3 scripts/test-memory-lifecycle.py
```

The history runner optionally accepts `--case <private.json>` with `snapshot`,
`transcript`, `session_id` and `queries`. It imports only into a random-port,
authenticated loopback service with offline model cache, and retains private
artifacts without copying vaults. The server is stopped in `finally`.

See [the recorded validation results](history-import-validation-2026-09-27.md),
including the unmet first-search recall criterion and the rejected search experiment.
