# memnest

<!-- markdownlint-disable MD013 MD033 -->

<img src="docs/logo.png" alt="memnest logo" width="440">

[한국어](README.ko.md) | [Install](#install) | [Use it](#use-it) | [Operations](docs/operations.md)

Ask Codex why Claude Code changed a setting last week. Memnest lets any connected client search the captured local record instead of relying on an AI-written summary.

It stores visible pi, Claude Code, and Codex conversation text on your machine and serves one shared store over MCP and HTTP. Memnest makes no generative LLM calls and needs no cloud account.

[![Latest release](https://img.shields.io/github/v/release/Blue-B/memnest?label=release)](https://github.com/Blue-B/memnest/releases/latest)
[![npm: pi-memnest](https://img.shields.io/npm/v/pi-memnest?label=npm%20pi-memnest&color=cb3837)](https://www.npmjs.com/package/pi-memnest)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

[![One independent client saves a decision and another retrieves the same ID and text](docs/demo-result.png)](docs/demo.md)

The checked-in [reproducible demo](docs/demo.md) sends real requests from two independent client processes and requires the saved ID and retrieved text to match. It proves the shared store and transcript parser without pretending that an LLM chose the tools. [Real-client validation](docs/client-validation.md) records the separately verified pi path and the account blockers for Claude Code and Codex.

## Why Memnest

- **Source-backed retrieval.** Results keep stable record IDs. The current source build can page through long records and return available session and capture metadata.
- **One history for several clients.** pi, Claude Code, Codex, and other MCP clients query the same local service instead of maintaining isolated copies.
- **No generated memory layer.** Conversation capture does not call an LLM to rewrite or summarize the text. Search uses local keyword and multilingual embedding indexes.
- **A small public contract.** Five tools cover remembering, searching, reading, correcting, and deleting. Deleted records move to trash, and corrections can retain the prior record as provenance.

For a few rules that should load every time, use `CLAUDE.md` or `AGENTS.md`. Memnest is for a growing record that you want to search when needed. It does not import or synchronize ChatGPT's or Claude's built-in memory.

## Install

### Published release: new Linux installation

The following installs core v0.2.1 without Rust. It installs the server only; connect a client below and run transcript capture separately.

```bash
curl -fsSL https://raw.githubusercontent.com/Blue-B/memnest/v0.2.1/core/scripts/install.sh \
  -o /tmp/memnest-install.sh
# Read the script before running it.
VERSION=v0.2.1 bash /tmp/memnest-install.sh --user
curl -fsS http://127.0.0.1:3111/health
```

Do not rerun the old installer over a customized service: it can overwrite service settings. Back up existing data before upgrading and use the [source-build upgrade instructions](docs/operations.md#one-command-setup-from-source).

The first write or search downloads the embedding model, about 1.1 GB. Embedding can use about 1.9 GB of RAM. Windows and WSL instructions are in the [operations guide](docs/operations.md).

### Next version: source build

From a checkout containing the unreleased changes, with Rust and Python 3.11 or newer:

```bash
cargo build --release --locked --manifest-path core/Cargo.toml
bash core/scripts/setup.sh --user --bin core/target/release/memnest
```

On Linux, setup starts the server and watcher, adds missing Claude Code, Codex, and Cursor connections, and tests saving and searching. Client files are backed up before changes; existing Memnest entries are left alone. pi is installed separately. Automatic recall hooks are opt-in with `--autocontext`.

Supported existing Linux units retain their settings and port. Custom commands, systemd drop-ins, and external environment files stop automatic setup before changes instead of being guessed at or overwritten. See [upgrade and recovery details](docs/operations.md#linux-service-upgrades).

## Connect a client

### pi

For the published version:

```bash
pi install npm:pi-memnest@0.2.0
```

For the next-version source features, build and install the local extension from the repository root instead:

```bash
(cd pi-extension && npm ci && npm run build)
pi install ./pi-extension
```

Both provide the memory tools and `/memnest` status. The published v0.2.0 enables automatic recall by default; the next version defaults to off. Set `MEMNEST_AUTOCONTEXT_MODE=off` to disable it explicitly, or `balanced` to enable it. See the [pi extension guide](pi-extension/README.md).

### MCP

Point a Streamable HTTP MCP client at the running service:

```json
{
  "mcpServers": {
    "memnest": { "url": "http://127.0.0.1:3111/mcp" }
  }
}
```

Stdio MCP and JSON HTTP examples are in the [adapter guide](adapters/README.md). Clients must be connected to the same service to share records.

## Use it

All clients use the same five memory tools: `memory_remember`, `memory_search`, `memory_get`, `memory_update`, and `memory_delete`.

```text
memory_remember(text="Use port 5433 for staging.", project="playbook")
memory_search(query="staging database port", project="playbook")
memory_get(id="<ID returned by search>")
```

When the host provides the current directory, omit `project` to search that workspace plus `playbook`. Use `project=all` only for a deliberate cross-project search. When a fact changes, save its replacement with `supersedes=<old ID>`. Deletion moves the record to trash.

### Read more of a source (next version)

Search keeps its existing 600-character excerpts per result. The experimental total search-text cap was removed.

```text
memory_get(id="<result ID>", offset=0, max_chars=8000)
memory_get(id="<same ID>", offset=<next_offset>, max_chars=8000)
memory_get(id="<transcript ID>", before=1, after=1)
```

Pagination lets you read beyond the original 8,000-character get limit. Nearby records come from the same captured conversation, not other projects. Capture order is not guaranteed to be the original conversation order. If a page is cut short, follow `next_offset`; clipped neighbors can be fetched by their IDs. See the [source-reading contract](docs/bounded-retrieval.md).

### Capture conversations

```bash
memnest watch
memnest watch --backfill
```

Capture is separate from automatic recall. It skips system/developer prompts, reasoning, tool traffic, images, and subagent sidechains. Captured transcripts remain until deleted. Automatic recall is optional; it can still select an irrelevant memory, so inspect sources before acting on them.

## Compatibility and release status

| Available now | Planned for v0.3.0 |
| --- | --- |
| Core v0.2.1: Linux x86_64 and Arm64 archives | Integrated setup with client configuration and recovery |
| pi-memnest v0.2.0 on npm | Source pagination and nearby conversation records |
| Memory tools, local search, and transcript capture | Default-off pi automatic recall and additional source metadata |

The current source is the v0.3.0 candidate, but its package versions, tag, and release have not been changed. The scope is intentionally limited to safer setup and source-backed reading. It does not include a new ranking algorithm, graph, dashboard, or team service.

macOS installation and Intel/Apple Silicon packaging code are present, but no macOS archive is published in v0.2.1. Native installation, restart, and removal have not been verified. Do not treat macOS as a validated release target yet.

The planned features require the updated source, not just the published core or npm package. See the [change summary (Korean)](docs/next-release.ko.md), [core changelog](core/CHANGELOG.md), and [pi changelog](pi-extension/CHANGELOG.md).

## How it works

![Memnest architecture](docs/architecture.png)

One Rust service stores records in SQLite and searches with BM25 plus local multilingual embeddings. SQLite is the source of truth; the search indexes can be rebuilt. The next version extends source reading, not the search model or ranking algorithm.

Memnest makes no generative LLM calls. A connected coding AI still uses its own model to read results and decide what to do. Memnest cannot prove an answer exists or detect that your code made an old memory obsolete.

## Evidence and limits

![Two independent MCP clients retrieve the same saved ID](docs/demo-result.png)

The [reproducible demo](docs/demo.md) saves through one connection and retrieves the same ID and text through another. This verifies the shared store, not autonomous cross-agent reasoning. [Real-client validation](docs/client-validation.md) records pi success and the Claude Code/Codex account blockers separately.

A [MemoryBench adapter and Korean fixture](benchmarks/memorybench/README.md) are included. Three fixture questions retrieved the expected session first. This is a small retrieval check, not a comparison showing better accuracy or speed than other memory tools. Full benchmark and source-read tests have different scopes.

## Data and security

The service binds to loopback by default. Do not expose port 3111 directly to the internet. Records stay local, but a cloud AI provider receives any retrieved text sent to its model.

Regular memories are not encrypted at rest. Redaction catches known credential patterns, not every secret. Use the AES-256-GCM vault for credentials; model-facing vault tools are hidden by default. Deleted records remain in trash for 30 days and may also remain in archive JSONL.

Back up `memory.db` together with `master.key` before upgrading. Read [SECURITY.md](SECURITY.md) for the limits and the [operations guide](docs/operations.md) for backup, restore, retention, and uninstall.

## More documentation

- [Design decisions](docs/design-decisions.md)
- [Development and checks](CONTRIBUTING.md)
- [Release notes](https://github.com/Blue-B/memnest/releases)

## License

MIT © Blue-B
