# memnest

<!-- markdownlint-disable MD013 MD033 -->

<img src="docs/logo.png" alt="memnest logo" width="220">

[한국어](README.ko.md) | [Install](#install) | [Use it](#use-it) | [Operations](docs/operations.md)

Your coding AI forgets everything when the session ends. Memnest lets it look up what you decided before, and why, from the original conversation instead of an AI-written summary.

- **Original text, not summaries.** No LLM rewrites your history. Saving and searching call no model API, so there is no API key and no extra token bill. Every result carries a source ID you can open.
- **One memory for every coding AI.** Claude Code, Codex, Cursor, and pi connect to the same local service. Decide something in Claude Code, look it up from Codex.
- **Stays on your machine.** One Rust binary with SQLite and a local multilingual embedding model. Credentials go to a separate encrypted vault.

[![Latest release](https://img.shields.io/github/v/release/Blue-B/memnest?label=release)](https://github.com/Blue-B/memnest/releases/latest)
[![npm: pi-memnest](https://img.shields.io/npm/v/pi-memnest?label=npm%20pi-memnest&color=cb3837)](https://www.npmjs.com/package/pi-memnest)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/Blue-B/memnest/main/core/scripts/install.sh | bash
```

This installs the latest release for your user, starts the server and conversation watcher, and adds Memnest to the Claude Code, Codex, and Cursor configurations it finds. Existing configuration is backed up first, and the restore command is printed.

Requirements: Linux x86_64 (glibc 2.35+, e.g. Ubuntu 22.04, Debian 12) or Arm64 (glibc 2.39+, e.g. Ubuntu 24.04), systemd, and Python 3. The first write or search downloads an embedding model of about 1.1 GB, and embedding can use about 1.9 GB of RAM.

<details>
<summary>Read the script first, pin a version, or upgrade</summary>

```bash
curl -fsSL https://raw.githubusercontent.com/Blue-B/memnest/main/core/scripts/install.sh \
  -o /tmp/memnest-install.sh
less /tmp/memnest-install.sh
VERSION=v0.4.3 bash /tmp/memnest-install.sh --user
```

Before upgrading an existing installation, back up `memory.db` together with `master.key`.

</details>

## See it work

[![One client saves a decision, a separate client retrieves the same record](docs/demo-result.png)](docs/demo.md)

Two separate clients, one local store: the record saved through MCP comes back with the same ID from an independent search. [Reproduce it](docs/demo.md) on your own machine.

## When to use it

Use the accumulated record for questions such as:

> Why did we switch task managers? Find the discussion behind that decision.
>
> What deployment approach did we last settle on for this project, and why?
>
> What did we try the last time this error happened?

The AI searches memory, reads the relevant source, and uses it to answer. Results include source IDs so you can check the evidence. These are example requests, not an autonomous demo or a guarantee that an answer will be found.

For a few rules that should apply every time, `AGENTS.md` or `CLAUDE.md` is simpler. Memnest is for a growing history of conversations and decisions that you want to look up when needed.

### Connect pi

With the core running, install the matching extension:

```bash
# Published extension for core 0.4.x
pi install npm:pi-memnest@0.4.0

# Or connect a reviewed source checkout
pi install ./pi-extension
```

If Memnest is already registered from another source, remove that registration first to avoid loading it twice. Use `/memnest` in pi to check the connection. See [extension settings](pi-extension/README.md).

<details>
<summary>Local development: vault tool settings</summary>

The extension exposes only the five memory tools by default. To let pi store and retrieve credentials with `secret_set`, `secret_get`, `secret_list`, and `secret_delete`, enable the vault tools before starting pi:

```bash
export MEMNEST_EXPOSE_SECRET_TOOLS=1
pi
```

Restart pi after changing the variable; `/reload` alone cannot change the running process environment. `/memnest` reports whether the vault tools are available and shows this activation hint when they are hidden.

</details>

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

### Source-backed coding records (0.4.0)

Save an approach as proposed, failed, or reported success without treating that report as verification. Optionally attach selected local files with `memory_remember(code_files=["relative/file"])`, then request `memory_get(check_code=true)` when you want to compare their bytes. Unrelated edits do not trigger warnings. See [code evidence](docs/code-evidence.md).

Optional [team access](docs/team-access.md) adds named project grants, audit and retention. A [free local comparison](docs/local-provider-comparison.md) records where Memnest did and did not perform well; it does not claim universal accuracy superiority.

## Important limits

- **Retrieval is not truth checking.** Search can miss a relevant record. Memnest cannot determine whether an old decision still holds. A following-source hint is not a verified reply relationship.
- **Capture is selective.** It stores visible conversation text, not reasoning, tool traffic, images, or subagent conversations. It does not import ChatGPT's or Claude's built-in memory.
- **Storage is local; your connected AI answers.** Default storage/search makes no generative LLM calls. Explicit `evidence_only=true` opts in to a configured local source-selector model, whose judgment remains unverified. A cloud AI provider receives retrieved text that you send to its model.
- **Regular memories are not encrypted.** Redaction handles known credential patterns, not every possible secret. Do not expose port 3111 directly to the internet.
- **A web connector is a network path into your memories.** ChatGPT and Grok connectors need a public URL, retrieved text reaches the provider's servers, and that URL acts as a credential. See [web connectors](docs/web-connectors.md).
- **Deletion is not immediate erasure.** Records default to 30 days in trash. New plaintext archives are off by default; old archives, backups and external transcripts may still retain copies. See [security and deletion limits](SECURITY.md).

## Documentation and checks

One Rust service stores source records in SQLite and searches with BM25 plus local multilingual embeddings. Search indexes can be rebuilt from the source records.

- [Web connectors for ChatGPT and Grok](docs/web-connectors.md), [real-client validation](docs/client-validation.md) and [reproducible shared-store demo](docs/demo.md)
- [Following-source checks and limits](docs/following-capture.md), [retrieval evaluation](benchmarks/memorybench/README.md), [a recorded three-question recall check](docs/recall-check-2026-10-06.md)
- [Design decisions](docs/design-decisions.md), [operations and backups](docs/operations.md), [contributing](CONTRIBUTING.md)
- [Latest release](https://github.com/Blue-B/memnest/releases/latest) and [changelog](core/CHANGELOG.md)

## License

MIT © Blue-B
