# memnest

<!-- markdownlint-disable MD013 MD033 -->

<img src="docs/logo.png" alt="memnest logo" width="220">

[English](README.md) | [한국어](README.ko.md)

Your coding AI forgets everything when the session ends. Memnest lets it look up what you decided before, and why, from the original conversation instead of an AI-written summary.

- **Original text, not summaries.** Saving and searching call no model API, so there is no API key and no extra token bill. Every result has a source ID you can open.
- **One memory for every coding AI.** Claude Code, Codex, Cursor, and pi share one local service. Decide something in Claude Code, find it from Codex.
- **Stays on your machine.** One Rust binary with SQLite and a local multilingual embedding model.

[![Latest release](https://img.shields.io/github/v/release/Blue-B/memnest?label=release)](https://github.com/Blue-B/memnest/releases/latest)
[![npm: pi-memnest](https://img.shields.io/npm/v/pi-memnest?label=npm%20pi-memnest&color=cb3837)](https://www.npmjs.com/package/pi-memnest)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/Blue-B/memnest/main/core/scripts/install.sh | bash
```

This starts the server and the conversation watcher, and connects the Claude Code, Codex, and Cursor setups it finds. Existing configuration is backed up first, and the restore command is printed. Codex runs the new hooks after you trust them once with `/hooks`.

Needs macOS on Apple Silicon or Linux with systemd (x86_64 with glibc 2.35+, Arm64 with glibc 2.39+), and Python 3. The first write or search downloads an embedding model of about 1.1 GB.

- pi: `pi install npm:pi-memnest@0.5.0`, then check with `/memnest`.
- Other MCP clients: connect to `http://127.0.0.1:3111/mcp` ([examples](adapters/README.md)).

## Use it

Ask in plain words:

> Why did we switch task managers? Find the discussion behind that decision.

Your AI searches, reads the source record, and answers with source IDs you can check. Search covers the current workspace and the shared `playbook`; ask for `project=all` to search everything.

Tools: `memory_search`, `memory_get`, `memory_remember`, `memory_update`, `memory_delete`, `task_card`.

[![One client saves a decision, a separate client retrieves the same record](docs/demo-result.png)](docs/demo.md)

## Task cards

For work that runs over several days. Your AI writes the card, Memnest adds your own messages to it word for word, and the next session opens the card and continues. In a replay of a real five-day pi session, the reply read 174K context tokens instead of 321K, and stopped proposing things against your stated policy (3 of 3 blind-graded runs, against 0 of 3). [How it works and limits](docs/task-cards.md)

## Also included

Conversation capture (`memnest watch`), [history import](docs/history-import.md), [code evidence](docs/code-evidence.md), an encrypted vault for credentials, [team access](docs/team-access.md), and [web connectors](docs/web-connectors.md) for ChatGPT, Claude, and Grok. Automatic recall is off unless you turn it on.

## Limits

- Search can miss a record, and it cannot tell whether an old decision still holds.
- Capture keeps visible conversation text only: no reasoning, tool output, or images.
- Memories your AI reads are sent to its provider. Regular memories are not encrypted, so do not expose port 3111 to the internet; a web connector URL works like a password.
- Deleted records stay in trash for 30 days, and backups may keep copies. See [security](SECURITY.md).

## More

[Operations, upgrades, and backups](docs/operations.md), [changelog](core/CHANGELOG.md), [contributing](CONTRIBUTING.md)

## License

MIT © Blue-B
