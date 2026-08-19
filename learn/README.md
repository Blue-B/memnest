# memnest-learn

<!-- markdownlint-disable MD013 -->

An experimental learning and working-memory layer for pi, built on top of a running memnest service.

**Status: experimental, version 0.1.0.** Interfaces, stored formats, and bucket names can change without a migration path. `core/` does not depend on this package, and nothing here is required to run memnest. Read [Known gaps](#known-gaps) before relying on it.

## What it does

The memnest engine deliberately contains no model. This layer adds the steps that need one, and it borrows the host agent's model through `@earendil-works/pi-ai` rather than asking for a separate API key or service.

Three loops run on top of the normal store:

| Loop | Module | Behaviour |
| --- | --- | --- |
| Capture | `capture.ts`, `extract.ts` | Every `MEMNEST_CAPTURE_TURNS` turns, a slice of the conversation is distilled into categorised memories (failure, correction, insight, preference, convention, tool quirk) and written to the bucket its importance selects. Corrections are captured by this pass like everything else, with no separate path. |
| Skill refinement | `skills.ts` | A procedural learning either appends a step or caveat to the closest saved skill or drafts a new one, so a procedure improves while it is used instead of staying frozen at the version first written. |
| User model | `user-model.ts` | On the same capture pass, memories categorised `preference` are folded into a small set of refined facets, so restating a preference sharpens one facet instead of adding another near-duplicate row. Other categories, corrections included, feed normal memory but not the user model. |

Capture routes by importance instead of writing everything to one place. A memory that lands on importance `preference` or `decision` (which covers the `preference`, `correction`, and `convention` categories) is a durable cross-project lesson, so it is written to the shared `playbook` bucket. That bucket is the only one the `learned_rules` injection slot reads, so the routing is what keeps that slot fed: a memory written anywhere else is stored but never injected back. Everything weaker stays project-local.

Two supporting pieces shape what the agent actually sees:

- `kv-snapshot.ts` keeps the injected block byte-stable between deliberate checkpoints (session start, compaction, day rollover). Prefix-caching runtimes invalidate their cache from the first differing token, so a block that changed every turn would force the conversation tail to be reprocessed each turn. A newly captured memory therefore does not rebuild the block; it appears at the next checkpoint.

The injected block is standing context and takes no query. Who the user is, what is open, and which rules they keep restating do not change with the prompt, so its slots are selected by durability (importance, then recall feedback, then recency) rather than by similarity to a search string. Selection reads each bucket directly through `GET /collection/{name}`, because ranking rows by their distance to a fixed placeholder sentence is not relevance, and a threshold on that number only tunes noise. Prompt-aware retrieval is a separate job, done on demand and risk-gated by `autocontext.ts` in the `pi-extension` package.

- `budget.ts` is a sliding-window limiter over the borrowed model. Automatic capture, skill refinement, and user-model work share `MEMNEST_LLM_MAX_CALLS` calls per window; when the window is spent those steps return nothing rather than competing with the user's own requests. Tools invoked by hand are not limited.

`consolidate.ts` clusters near-duplicate memories by trigram similarity and merges each cluster into one canonical entry, retiring the rest without deleting them.

## Requirements

- [bun](https://bun.sh), used by the build and test scripts. Verified with 1.3.14.
- pi, since the entry point is a pi extension and there is no other surface.
- A running memnest service, by default `http://127.0.0.1:3111`.

Peer dependencies are `@earendil-works/pi-coding-agent` and `@earendil-works/pi-ai` at `^0.78.0`, plus `@sinclair/typebox`.

## Install

Not published to npm. Build from a checkout, with the core service already running.

```bash
cd memnest/learn
bun install
bun run build
pi install .
```

`package.json` points pi at `./src/index.ts`, and `prepare` runs the build, so `dist/index.js` is produced during install. That bundle is committed to the repository.

## Hooks and tools

Registered pi hooks:

| Hook | Purpose |
| --- | --- |
| `session_start` | Reset transcript state and build the first injection snapshot. |
| `before_agent_start` | Append the snapshot to the system prompt. Registered only when injection is enabled. |
| `agent_end` | Ingest assistant text, so a failure the model found but the user never restated is still visible to extraction. |
| `input` | Count turns and start a periodic capture pass. |
| `session_before_compact` | Write a handoff to the daily log and the engine, then refresh the snapshot. |
| `session_shutdown` | Final capture flush. |

Registered tools:

| Tool | Purpose |
| --- | --- |
| `scratchpad` | A short-term checklist for the session (add, done, undo, remove, list, clear), kept in a local Markdown file. |
| `skill` | Save, search, or refine a reusable procedure. `update` appends to the closest existing skill. |
| `memory_consolidate` | Merge near-duplicate memories for one topic. Dry run unless `apply` is true. |

## Where it stores things

Engine buckets, through `/add`, `/update`, `/search`, `/context`, `/neighbors`, `/collection/{name}`, and `/summary`:

- the current project bucket, named from `MEMNEST_PROJECT` or the working directory's base name, for captured memories that are not routed to `playbook`
- `_skills` for procedures
- `_user_model` for user facets
- `playbook` for corrections and preferences, written there by capture and read back as the `learned_rules` slot

Local files, under `MEMNEST_LEARN_DIR` (default `~/.pi/agent/memnest-learn`):

- `SCRATCHPAD.md` for the checklist
- `daily/<date>.md` for the append-only log and compaction handoffs

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `MEMNEST_URL` | `http://127.0.0.1:3111` | Address of the running service. |
| `MEMNEST_PROJECT` | working directory base name | Bucket for captured memories. |
| `MEMNEST_LEARN_DIR` | `~/.pi/agent/memnest-learn` | Where the scratchpad and daily logs live. |
| `MEMNEST_LEARN_INJECT` | on | Set to `0` to stop injecting the memory block. Capture and the learning loops keep running. |
| `MEMNEST_CAPTURE_TURNS` | `10` | Turns between automatic capture passes. |
| `MEMNEST_LEARN_RULE_TOP` | `2` | Learned rules injected from `playbook`, most durable first. |
| `MEMNEST_LLM_MAX_CALLS` | `24` | Background model calls allowed per window. |
| `MEMNEST_LLM_WINDOW_MS` | `300000` | Length of that window. |

Every value above was read from `src/`; there are no other environment variables in this package.

## Development

```bash
bun test test/       # 29 tests across 3 files
bun run typecheck    # tsc --noEmit
bun run build
```

The pure modules take their clock, `fetch`, and model as arguments, so the unit tests need no network and no model.

Files ending in `.live.ts` are excluded from `bun test` on purpose. They talk to a real engine and must be pointed at a throwaway one, never a store you care about:

```bash
MEMNEST_URL=http://127.0.0.1:3199 bun run test/integration.live.ts
MEMNEST_URL=http://127.0.0.1:3199 bun run test/loop.integration.live.ts
```

`test/quality.live.ts` additionally drives the real prompts through a real model to judge output quality rather than the data path.

One rough edge: `test/extension-load.test.ts` asserts that `before_agent_start` is registered, so it fails when `MEMNEST_LEARN_INJECT=0` is exported in the shell, because that hook is then intentionally absent. Unset the variable before running the suite.

## Known gaps

- Experimental at 0.1.0. No stability promise, no migration path for stored formats, not published to npm.
- pi only. The layer is a pi extension and exposes no HTTP or MCP surface, so other hosts cannot use it. For cross-host injection and logging, use `memnest hook` and `memnest watch` from the core instead, which cover a smaller job with no model involved.
- No CI. The repository workflows cover `core/`, `journal/`, and `pi-extension/`, so these tests run only when invoked by hand.
- Output quality follows the borrowed model. Extraction, skill drafting, and user-model refinement are prompt-driven, and a weaker host model yields weaker memories. When the model is unavailable each step degrades to doing nothing.
- Consolidation clusters by trigram overlap on returned documents rather than by cosine distance, since the engine returns a composite score. Paraphrases that share few character trigrams are not clustered.

## Related documentation

- [memnest root README](../README.md) for engine setup and the cross-host path
- [pi-memnest](../pi-extension/README.md) for the base pi tools, Autocontext, and AutoLog

## License

MIT © Blue-B
