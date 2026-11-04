# memnest

<!-- markdownlint-disable MD013 MD033 -->

<img src="docs/logo.png" alt="memnest logo" width="220">

[English](README.md) | [한국어](README.ko.md)

Your coding AI forgets everything when the session ends. Memnest lets it look up what you decided before, and why, from the original conversation instead of an AI-written summary.

- **Original text, not summaries.** Nothing rewrites your history. Saving and searching call no model API, so there is no API key and no extra token bill. Every result carries a source ID you can open.
- **One memory for every coding AI.** Claude Code, Codex, Cursor, and pi connect to the same local service. Decide something in Claude Code, look it up from Codex.
- **Stays on your machine.** One Rust binary with SQLite and a local multilingual embedding model.

[![Latest release](https://img.shields.io/github/v/release/Blue-B/memnest?label=release)](https://github.com/Blue-B/memnest/releases/latest)
[![npm: pi-memnest](https://img.shields.io/npm/v/pi-memnest?label=npm%20pi-memnest&color=cb3837)](https://www.npmjs.com/package/pi-memnest)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

## Features

| Feature | What it does | Works with |
|---|---|---|
| Search | Finds past decisions in the original conversation, with a source ID to open | All clients |
| Save and correct | Saves a decision when asked; a changed fact replaces the old one and keeps the link | All clients |
| Conversation capture | `memnest watch` stores visible conversation text from local transcripts | Claude Code, Codex, pi |
| [Task cards](docs/task-cards.md) | A handoff note that carries multi-day work into a new session, with your own words kept verbatim | pi, Claude Code, Codex (other MCP clients: save and open only) |
| Automatic recall (opt-in) | Adds related memories to each prompt | pi, Claude Code, Codex |
| [Code evidence](docs/code-evidence.md) | Attaches file fingerprints to a record and compares them later | All clients |
| [History import](docs/history-import.md) | Adds conversations from before installation | CLI |
| Vault (opt-in) | Stores credentials encrypted, apart from regular memories | All clients |
| [Team access](docs/team-access.md) (optional) | Project grants, audit, retention | Server |
| [Web connectors](docs/web-connectors.md) | Lets cloud assistants that call from their own servers reach Memnest through a public URL | ChatGPT, Claude connectors, Grok |

Local apps such as Claude Code, Codex, Cursor, and pi connect to `127.0.0.1` directly and need no web connector. `memnest watch` reads transcripts under the home directory of the system it runs on: macOS, Linux, or WSL. Inside WSL, apps that run on the Windows side need `memnest watch --path /mnt/c/Users/<you>/.claude/projects`.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/Blue-B/memnest/main/core/scripts/install.sh | bash
```

This starts the server and conversation watcher, and adds Memnest to the Claude Code, Codex, and Cursor configurations it finds, including the task card hooks. Existing configuration is backed up first, and the restore command is printed. Codex runs new hooks only after you trust them once with `/hooks`.

Requirements: macOS on Apple Silicon, or Linux with systemd (x86_64 with glibc 2.35+, Arm64 with glibc 2.39+), and Python 3. The first write or search downloads an embedding model of about 1.1 GB.

- **pi:** `pi install npm:pi-memnest@0.4.0`, then check the connection with `/memnest`. See [extension settings](pi-extension/README.md).
- **Other MCP clients:** connect to `http://127.0.0.1:3111/mcp`. See [connection examples](adapters/README.md).
- **Pinned versions, upgrades, source builds:** see [operations](docs/operations.md).

## Use it

Ask your AI in plain words:

> Why did we switch task managers? Find the discussion behind that decision.
>
> What did we try the last time this error happened?

It searches, reads the source record, and answers with source IDs you can check. Searches cover the current workspace plus the shared `playbook`; ask for `project=all` to search everything. For a few rules that should apply every time, `AGENTS.md` or `CLAUDE.md` is simpler.

Tools: `memory_search`, `memory_get`, `memory_remember`, `memory_update`, `memory_delete`, and `task_card`.

[![One client saves a decision, a separate client retrieves the same record](docs/demo-result.png)](docs/demo.md)

## Task cards

When one task runs for days in a single session, compaction drops your corrections from the summary. A task card is that task's handoff note. Your AI writes the card, Memnest appends your own messages verbatim, and a fresh session the next day opens the card and continues. Cards ship in the next release.

Measured by replaying a real complaint from a five-day pi session:

| Measure | Without a card | With a card |
|---|---|---|
| Context tokens for that reply | 321,213 | 174,118 |
| Avoids a proposal against your stated policy (blind graded, 3 runs) | 0/3 | 3/3 |
| New session, five questions about the task: score, tokens per answer | 19/20, 73,002 | 22/22, 19,632 |

[How it works, client differences, and limits](docs/task-cards.md)

## Limits

- Search can miss a record, and it cannot tell whether an old decision still holds.
- Capture stores visible conversation text, not reasoning, tool output, images, or built-in ChatGPT and Claude memory.
- Your connected AI writes the answers. A cloud provider receives the memories it is sent.
- Regular memories are not encrypted. Do not expose port 3111 to the internet.
- A web connector URL works like a password into your memories.
- Deleted records stay in trash for 30 days; backups and old transcripts may keep copies. See [security](SECURITY.md).

## Documentation

- [Operations and backups](docs/operations.md), [design decisions](docs/design-decisions.md), [contributing](CONTRIBUTING.md)
- [Real-client validation](docs/client-validation.md), [reproducible demo](docs/demo.md), [retrieval evaluation](benchmarks/memorybench/README.md), [local comparison](docs/local-provider-comparison.md)
- [Latest release](https://github.com/Blue-B/memnest/releases/latest), [changelog](core/CHANGELOG.md)

## License

MIT © Blue-B
