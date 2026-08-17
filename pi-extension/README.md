# pi-memnest

<!-- markdownlint-disable MD013 -->

A local memnest bridge for pi.

`pi-memnest` connects [pi](https://github.com/badlogic/pi-mono) to a running [memnest](https://github.com/Blue-B/memnest) HTTP service. It gives pi explicit memory, note, fact, collection, health, and secret tools. It can also retrieve a small memory card before selected prompts and optionally record conversations.

This extension does not contain the memory engine. Start the Rust core before installing it.

## Requirements

- Node.js 20 or newer for building the extension
- pi
- a memnest HTTP service, normally at `http://127.0.0.1:3111`

`pi-memnest` is not currently available from the public npm registry. Install it from a memnest source checkout.

## Install from source

Start the core service first. From the repository root:

```bash
cd core
cargo build --release
./target/release/memnest --data-dir ~/.memnest
```

In another terminal, build and install the extension:

```bash
cd /path/to/memnest/pi-extension
npm install
pi install .
```

The extension uses `MEMNEST_URL` when set, otherwise it connects to `http://127.0.0.1:3111`. Set `MEMNEST_TOKEN` when the core requires bearer authentication.

Check the connection and reveal the dashboard URL from pi:

```text
/memnest
```

The command reports service health, memory count, the active data directory, and the canonical dashboard link.

## Tools

The current extension registers 20 tools.

### Memories and context

| Tool | Purpose |
| --- | --- |
| `memory_remember` | Save a memory and route it to a project collection. Preference and decision memories default to `playbook`. |
| `memory_update` | Correct an existing memory by id and refresh its indexes. |
| `memory_search` | Search with BM25 and vector retrieval. Results include a `recall_id` for outcome tracking. |
| `memory_feedback` | Mark a recall as helpful, harmful, or ignored. |
| `memory_get` | Fetch the full text of a memory returned as a truncated excerpt. |
| `memory_context` | Build a character-bounded prompt block from memories, notes, and facts. |
| `memory_stats` | Read store statistics. |
| `memory_sessions` | List recent session summaries. |
| `memory_facts_list` | List structured subject, predicate, object facts. |

### Notes, secrets, and collections

| Tool | Purpose |
| --- | --- |
| `note_set`, `note_get`, `notes_list`, `note_delete` | Manage small key-value notes. |
| `secret_set`, `secret_get`, `secret_list`, `secret_delete` | Manage values in the AES-256-GCM secret vault. |
| `collections_list` | List project collections and counts. |
| `memory_health` | Check whether the server is reachable. |
| `memnest_autocontext_status` | Inspect Autocontext state or preview a live retrieval. |

Ordinary memories are not encrypted at rest. Use the secret tools for credentials, and keep the core data directory and `master.key` private.

## Basic use

```text
memory_remember text="Project X deploys on port 8320" project="project-x" memory_kind="fact" confidence=1
memory_search query="Project X deployment port" project="project-x"
memory_feedback recall_id="recall_..." outcome="helpful"
memory_context query="Project X deployment" project="project-x"
memory_update id="manual_..." text="Project X now deploys on port 8420"
```

The core records each save in `/operations`, completes embedding and indexing, and then acknowledges the write. A semantically deduplicated id remains resolvable to the canonical memory. The first operation can take longer while the core downloads its embedding model.

## Structured memory

`memory_remember` supports four portable kinds:

- `record` for historical turns and outcomes
- `fact` for stable project or environment knowledge
- `rule` for preferences, decisions, and guardrails
- `procedure` for reusable verified workflows

Optional `confidence`, `source_ids`, `supersedes`, and `verified_at` fields preserve provenance without tying the data model to pi. The core HTTP and MCP contracts expose the same fields to other platform adapters.

## Autocontext

Autocontext is enabled in `balanced` mode by default. It does not inject a full memory dump at session start. Before a substantive prompt, it retrieves a small card only when the prompt carries a risk signal. Five rules define those signals, and each one matches English and Korean:

| Rule | Prompt refers to |
| --- | --- |
| `memory` | Earlier work, something remembered, forgotten, or discussed before. |
| `credential` | Accounts, logins, keys, tokens, authentication, subscriptions, or plans. |
| `absence` | A claim that something is missing, broken, unsupported, or impossible. |
| `money` | Revenue, pricing, billing, promotion, or user growth. |
| `config` | Settings, environment variables, options, defaults, or thresholds. |

A prompt that matches none of them gets no card in this mode. Use `MEMNEST_AUTOCONTEXT_MODE=aggressive` to add the general first-turn and topic-shift lane, which injects on every topic change instead. Use `MEMNEST_AUTOCONTEXT_MODE=off` to disable retrieval.

Common controls:

| Variable | Default | Purpose |
| --- | --- | --- |
| `MEMNEST_AUTOCONTEXT_MODE` | `balanced` | `balanced`, `aggressive`, or `off`. |
| `MEMNEST_AUTOCONTEXT_TOP` | `2` | Maximum results included in one card. |
| `MEMNEST_AUTOCONTEXT_MAX_INJECTIONS` | `4` | Maximum cards in one session. |
| `MEMNEST_AUTOCONTEXT_MIN_SCORE` | `0.12` | Minimum score for the general lane. |
| `MEMNEST_AUTOCONTEXT_RISK_MIN_SCORE` | `0.12` | Minimum score for a risk-triggered card. |
| `MEMNEST_AUTOCONTEXT_EXCLUDE` | `_superseded,default,root,global` | Collections excluded from automatic retrieval. |

Run `memnest_autocontext_status` to see the active mode, counters, and a test retrieval.

## AutoLog

AutoLog is off by default. Enable it explicitly:

```bash
MEMNEST_AUTOLOG=1 pi
```

When enabled, lifecycle hooks send user messages and assistant final messages to memnest without blocking the agent loop. Thinking content, short noise, and memnest's own tool calls are skipped. Pending writes are drained when the session ends.

Tool-result capture remains off unless `MEMNEST_AUTOLOG_TOOLS=1` is set. Tool results are higher volume and are truncated before storage.

AutoLog records with log importance have a 30-day core retention period by default. Configure that policy on the core with `MEMNEST_TTL_AUTOLOG_DAYS`.

## Outside pi

Autocontext and AutoLog need hooks into a host's session events, which MCP does not describe; it covers tool calls a model chooses to make. Those two behaviours therefore used to require an extension, and pi was the only host that had one. The core now provides them directly, so another host is not left with tools alone:

- `memnest hook` reads a host's prompt hook payload on stdin and answers with a context pack, in the shape that host expects.
- `memnest watch` follows the session transcripts a host already writes and stores the turns it finds, with no host configuration.

See [automatic memory](../README.md#automatic-memory) in the root README. Inside pi this extension remains the fuller surface, since it adds the 20 tools above and the `/memnest` command.

## Development

```bash
npm install
npm run build
npm run smoke
```

`npm install` runs the `prepare` script and builds `dist/index.mjs`. pi loads that bundled ESM file rather than `src/index.ts`.

An end-to-end MCP check is also available when its prerequisites are running:

```bash
npm run e2e
```

## Related documentation

- [memnest root README](../README.md) for engine setup, lifecycle, service installation, backup, and security
- [CHANGELOG.md](./CHANGELOG.md) for extension changes
- [memnest-journal](../journal/README.md) for the optional Markdown audit mirror

## License

MIT © Blue-B
