# pi-memnest

<!-- markdownlint-disable MD013 -->

A local memnest bridge for pi.

`pi-memnest` connects [pi](https://github.com/badlogic/pi-mono) to a running [memnest](https://github.com/Blue-B/memnest) HTTP service. It gives pi the canonical six memory tools and four secret tools. It can also retrieve a small project-scoped memory card before selected prompts.

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

The extension registers exactly ten model tools. The `/memnest` command and Autocontext hook remain available without adding status or admin tools to the model surface.

| Tool | Purpose |
| --- | --- |
| `memory_remember` | Save a durable memory. Values marked sensitive are rejected and must use `secret_set`. |
| `memory_search` | Search the current workspace by default, or use `project=all` explicitly. |
| `memory_get` | Fetch one memory by id. |
| `memory_update` | Correct one memory and refresh its indexes. |
| `memory_delete` | Soft-delete one memory to the internal trash bucket. |
| `memory_feedback` | Record recall telemetry; only an optional `memory_id` changes that result's ranking. |
| `secret_set`, `secret_get`, `secret_list`, `secret_delete` | Manage AES-256-GCM vault values. Vault operations fail closed when crypto is unavailable. |

Ordinary memories are not encrypted at rest. Use the secret tools for credentials, and keep the core data directory and `master.key` private.

## Basic use

```text
memory_remember text="Project X deploys on port 8320" project="project-x" memory_kind="fact" confidence=1
memory_search query="Project X deployment port" project="project-x"
memory_feedback recall_id="recall_..." memory_id="manual_..." outcome="helpful"
memory_update id="manual_..." text="Project X now deploys on port 8420"
memory_delete id="manual_..."
```

The core records each save in `/operations`, completes embedding and indexing, and then acknowledges the write. A semantically deduplicated id remains resolvable to the canonical memory. The first operation can take longer while the core downloads its embedding model.

## Structured memory

`memory_remember` supports four portable kinds:

- `record` for historical turns and outcomes
- `fact` for stable project or environment knowledge
- `rule` for preferences, decisions, and guardrails
- `procedure` for reusable verified workflows

Optional `confidence`, `source_ids`, and `supersedes` fields preserve provenance without tying the data model to pi. The core HTTP and MCP contracts expose the same fields to other platform adapters.

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
| `MEMNEST_AUTOCONTEXT_MIN_SCORE` | `0.25` | Minimum score for the general lane. |
| `MEMNEST_AUTOCONTEXT_RISK_MIN_SCORE` | `0.25` | Minimum score for a risk-triggered card. |
| `MEMNEST_AUTOCONTEXT_EXCLUDE` | `_superseded,default,root,global` | Collections excluded from automatic retrieval. |

## Automatic conversation capture

The extension does not install AutoLog event hooks. Use `memnest watch` as the single capture path so pi conversations are not stored twice. Watch stores redacted user and assistant transcript text without summarization, keeps long turns in ordered chunks without truncation, and retains new identified transcript AutoLog until explicit deletion. Legacy AutoLog keeps the core's configured retention policy.

## Outside pi

MCP does not describe host session events. The core provides host-neutral automatic behavior instead:

- `memnest hook` reads a host's hook payload on stdin and answers with a context pack, in the shape that host expects.
- `memnest watch` follows Claude Code, pi, and Codex transcripts and stores visible conversation text, with no host extension hooks.

See [automatic memory](../README.md#automatic-memory) in the root README. Inside pi the extension exposes the same ten-tool contract and adds the `/memnest` command.

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
