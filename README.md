# memnest

<!-- markdownlint-disable MD013 MD033 -->

<img src="docs/logo.png" alt="memnest logo" width="220">

[한국어](README.ko.md) | [Install](#install) | [Use it](#use-it) | [Operations](docs/operations.md)

Local memory for finding what you did with a coding AI, and why you decided to do it.

Memnest stores pi, Claude Code, and Codex conversation text on your machine. Connected AIs can search those records and read the source after a session ends. Connect several clients to the same Memnest service to use the same history.

[![Latest release](https://img.shields.io/github/v/release/Blue-B/memnest?label=release)](https://github.com/Blue-B/memnest/releases/latest)
[![npm: pi-memnest](https://img.shields.io/npm/v/pi-memnest?label=npm%20pi-memnest&color=cb3837)](https://www.npmjs.com/package/pi-memnest)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

## When to use it

Use the accumulated record for questions such as:

> Why did we switch task managers? Find the discussion behind that decision.
>
> What deployment approach did we last settle on for this project, and why?
>
> What did we try the last time this error happened?

The AI searches memory, reads the relevant source, and uses it to answer. Results include source IDs so you can check the evidence. These are example requests, not an autonomous demo or a guarantee that an answer will be found.

For a few rules that should apply every time, `AGENTS.md` or `CLAUDE.md` is simpler. Memnest is for a growing history of conversations and decisions that you want to look up when needed.

## Install

Linux x86_64 and Arm64 archives are available; Rust is not required. The first write or search downloads an embedding model of about 1.1 GB. Embedding can use about 1.9 GB of RAM.

Read the downloaded script before running it. Before upgrading an existing installation, back up `memory.db` together with `master.key`.

```bash
curl -fsSL https://raw.githubusercontent.com/Blue-B/memnest/v0.3.1/core/scripts/install.sh \
  -o /tmp/memnest-install.sh
VERSION=v0.3.1 bash /tmp/memnest-install.sh --user
```

Setup starts the server and conversation watcher and adds missing connections to supported Claude Code, Codex, and Cursor configurations. It backs up existing configuration before changing it and prints recovery commands.

### Connect pi

With the core running, install the matching extension:

```bash
pi install npm:pi-memnest@0.3.1
```

If Memnest is already registered from another source, remove that registration first to avoid loading it twice. Use `/memnest` in pi to check the connection. See [extension settings](pi-extension/README.md).

Other MCP clients connect to `http://127.0.0.1:3111/mcp`. See [connection examples](adapters/README.md), [other environments and source installation](docs/operations.md), and [upgrade and recovery](docs/operations.md#linux-service-upgrades). macOS is source-only; native installation and removal have not been validated.

## Use it

Ask your connected AI to look up a previous conversation or decision. Automatic recall is off by default; you can start by requesting a search when you need one. See [extension settings](pi-extension/README.md) to enable automatic recall.

Clients that call tools directly use this flow:

```text
memory_search(query="reason for switching task managers")
memory_get(id="<ID returned by search>")
```

Five tools cover saving, searching, reading, correcting, and deleting. Use `memory_remember` to explicitly save an important decision. When a fact changes, save its replacement with `supersedes=<old ID>` to link it to the earlier record.

When the host provides the current directory, searches include that workspace and the shared `playbook`. Specify `project=all` for a deliberate cross-project search. See [source reading](docs/bounded-retrieval.md) for long records and nearby conversation entries.

Conversations from before installation are not all imported automatically. Select the original files you need with [history import](docs/history-import.md).

## Important limits

- **Retrieval is not truth checking.** Search can miss a relevant record. Memnest cannot determine whether an old decision still holds. A following-source hint is not a verified reply relationship.
- **Capture is selective.** It stores visible conversation text, not reasoning, tool traffic, images, or subagent conversations. It does not import ChatGPT's or Claude's built-in memory.
- **Storage is local; your connected AI answers.** Memnest makes no generative LLM calls. A cloud AI provider receives retrieved text that you send to its model.
- **Regular memories are not encrypted.** Redaction handles known credential patterns, not every possible secret. Do not expose port 3111 directly to the internet.
- **Deletion is not immediate erasure.** Records remain in trash for 30 days and may remain in archive JSONL. See [security and deletion limits](SECURITY.md).

## Documentation and checks

One Rust service stores source records in SQLite and searches with BM25 plus local multilingual embeddings. Search indexes can be rebuilt from the source records.

- [Real-client validation](docs/client-validation.md) and [reproducible shared-store demo](docs/demo.md)
- [Following-source checks and limits](docs/following-capture.md), [retrieval evaluation](benchmarks/memorybench/README.md)
- [Design decisions](docs/design-decisions.md), [operations and backups](docs/operations.md), [contributing](CONTRIBUTING.md)
- [Latest release](https://github.com/Blue-B/memnest/releases/latest) and [changelog](core/CHANGELOG.md)

## License

MIT © Blue-B
