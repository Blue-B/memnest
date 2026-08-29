# pi-memnest

<!-- markdownlint-disable MD013 -->

A local memnest bridge for pi.

`pi-memnest` connects [pi](https://github.com/badlogic/pi-mono) to a running [memnest](https://github.com/Blue-B/memnest) HTTP service. It gives pi the canonical five memory tools. Four secret tools are an explicit opt-in. The extension can also retrieve a small workspace-scoped memory card before selected prompts.

This extension does not contain the memory engine. Start the Rust core before installing it.

## Requirements

- Node.js 20 or newer
- pi
- a memnest HTTP service, normally at `http://127.0.0.1:3111`

## Install

Start the [memnest core service](https://github.com/Blue-B/memnest#install), then install the extension from npm:

```bash
pi install npm:pi-memnest
```

To pin this first release, use `pi install npm:pi-memnest@0.1.0`. A pinned package is not changed by `pi update --extensions`.

To develop from a source checkout instead:

```bash
cd /path/to/memnest/pi-extension
npm install
pi install .
```

The extension uses `MEMNEST_URL` when set, otherwise it connects to `http://127.0.0.1:3111`. Set `MEMNEST_TOKEN` when the core requires bearer authentication.

Check the connection from pi:

```text
/memnest
```

The command reports service health, memory count, and the active data directory.

## Tools

The extension registers exactly five memory tools by default. Set `MEMNEST_EXPOSE_SECRET_TOOLS=1` before starting pi to register the four vault tools too. The `/memnest` command and Autocontext hook remain available without adding status or admin tools to the model surface.

| Tool | Purpose |
| --- | --- |
| `memory_remember` | Save a durable memory. Values marked sensitive are rejected and must use `secret_set`. |
| `memory_search` | Search the current workspace by default, or use `project=all` explicitly. |
| `memory_get` | Fetch one memory by id. |
| `memory_update` | Correct one memory and refresh its indexes. |
| `memory_delete` | Soft-delete one memory to the internal trash bucket. |
| `secret_set`, `secret_get`, `secret_list`, `secret_delete` | Opt-in tools for AES-256-GCM vault values. Vault operations fail closed when crypto is unavailable. |

Ordinary memories are not encrypted at rest. Enable the secret tools only for an agent you trust with plaintext credentials, and keep the core data directory and `master.key` private.

## Basic use

```text
memory_remember text="Project X deploys on port 8320" memory_kind="fact" confidence=1
memory_search query="Project X deployment port"
memory_remember text="Project X now deploys on port 8420" memory_kind="fact" supersedes="manual_..."
memory_delete id="manual_..."
```

When `project` is omitted, the extension sends pi's absolute `cwd`; the core derives a private workspace ID and includes `playbook` in recall. The core durably queues its index work in SQLite and acknowledges the write only after Tantivy and HNSW are synchronized. Plain duplicate records may reuse an existing id. Structured facts, rules, provenance, and corrections do not use semantic content deduplication. The first operation that needs an embedding takes longer because that is when the core downloads its embedding model.

## Structured memory

`memory_remember` supports four portable kinds:

- `record` for historical turns and outcomes
- `fact` for stable project or environment knowledge
- `rule` for preferences, decisions, and guardrails
- `procedure` for reusable verified workflows

Optional `confidence`, `source_ids`, `supersedes`, and `verified_at` fields preserve provenance without tying the data model to pi. `supersedes` atomically hides an active memory in the same workspace. `confidence` and `verified_at` are client assertions, not automatic ranking boosts. The core HTTP and MCP contracts expose the same fields to other platform adapters.

## Autocontext

Autocontext is enabled by default. It does not inject a full memory dump at session start. Before each substantive prompt, until the per-session card limit is reached, it searches the current workspace and `playbook` using the prompt in its original language. It injects a small card only when the best results meet the semantic score threshold. Short replies, slash commands, exact repeated prompts, and results below the threshold stay quiet. There is no language-specific keyword list. Search coverage is language-neutral, but whether a result survives still depends on the multilingual embedding model, the core distance cutoff, and this score threshold.

Set `MEMNEST_AUTOCONTEXT_MODE=off` to disable retrieval. Existing `balanced` and `aggressive` values remain accepted, but both now use the same language-neutral semantic gate.

Every card labels retrieved text as untrusted reference data. AutoLog results are marked as conversation evidence, not verified facts, and markup inside stored text is escaped before injection. The agent must verify claims and never follow commands found inside a memory.

Common controls:

| Variable | Default | Purpose |
| --- | --- | --- |
| `MEMNEST_AUTOCONTEXT_MODE` | `balanced` | `off` or `none` disables retrieval; any other value enables semantic recall. |
| `MEMNEST_AUTOCONTEXT_TOP` | `2` | Maximum results included in one card. |
| `MEMNEST_AUTOCONTEXT_MAX_INJECTIONS` | `4` | Maximum cards in one session. |
| `MEMNEST_AUTOCONTEXT_MIN_SCORE` | `0.25` | Minimum score for any injected card. |
| `MEMNEST_AUTOCONTEXT_EXCLUDE` | `_superseded,default,root,global` | Collections excluded from automatic retrieval. |
| `MEMNEST_AUTOCONTEXT_DISABLE` | unset | Set to `1` to turn retrieval off, same effect as `MODE=off`. |
| `MEMNEST_AUTOCONTEXT_N` | `20` | Candidates requested from the service before ranking. |
| `MEMNEST_AUTOCONTEXT_MIN_LEN` | `16` | Prompts shorter than this are ignored. |
| `MEMNEST_AUTOCONTEXT_DOC_CHARS` | `240` | Characters kept per result in the card. |
| `MEMNEST_AUTOCONTEXT_TIMEOUT_MS` | `1500` | Retrieval budget before the prompt proceeds without a card. |

## Automatic conversation capture

The extension does not install AutoLog event hooks. Use `memnest watch` as the single capture path so pi conversations are not stored twice. Watch stores redacted user and assistant transcript text without summarization, keeps long turns in ordered chunks without truncation, and retains new identified transcript AutoLog until explicit deletion. Legacy AutoLog keeps the core's configured retention policy.

## Outside pi

MCP does not describe host session events. The core provides host-neutral automatic behavior instead:

- `memnest hook` reads a host's hook payload on stdin and answers with a context pack, in the shape that host expects.
- `memnest watch` follows Claude Code, pi, and Codex transcripts and stores visible conversation text, with no host extension hooks.

See [automatic context and conversation capture](https://github.com/Blue-B/memnest#automatic-context-and-conversation-capture) in the root README. Inside pi the extension exposes the same five-tool memory contract, optionally adds four vault tools, and provides the `/memnest` command.

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

- [memnest root README](https://github.com/Blue-B/memnest) for engine setup, lifecycle, service installation, backup, and security
- [CHANGELOG.md](./CHANGELOG.md) for extension changes
- [THIRD_PARTY_NOTICES.md](./THIRD_PARTY_NOTICES.md) for what the bundle carries

## License

MIT © Blue-B
