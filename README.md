# memnest

<!-- markdownlint-disable MD013 -->

[한국어 README](README.ko.md)

Local memory for AI coding agents. Memnest stores durable memories and searchable conversation text on your machine, then exposes the same small tool contract to pi, Claude Code, Codex, and other MCP clients.

[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](./LICENSE)
![Rust](https://img.shields.io/badge/core-Rust-orange.svg)
![Protocol](https://img.shields.io/badge/interface-MCP%20%2B%20HTTP-blue.svg)

![memnest operations dashboard](docs/dashboard.png)

## What it does

- Stores manual memories, project decisions, preferences, and corrections.
- Searches with local BM25 and HNSW vector indexes.
- Preserves redacted user and assistant conversation text without LLM summarization.
- Records recall feedback so one useful or harmful result can affect future ranking.
- Keeps credentials in a separate AES-256-GCM vault.

The Rust core does not call an LLM. Embeddings run locally with `intfloat/multilingual-e5-base`.

## Install

```bash
git clone https://github.com/Blue-B/memnest.git
cd memnest/core
cargo build --release
install -m755 target/release/memnest ~/.local/bin/memnest
memnest --data-dir ~/.memnest
```

The service, dashboard, HTTP API, and Streamable HTTP MCP endpoint share one address:

```text
http://127.0.0.1:3111
http://127.0.0.1:3111/mcp
```

Starting the service does not download anything. The embedding model is fetched on the first operation that needs it, which is the first write or the first search, so that call takes longer than the rest. Run `memnest --warmup-embedding` to pay that cost ahead of time. Linux, WSL, Windows service setup, backup, restore, and retention options are in [`docs/operations.md`](docs/operations.md).

## Connect an agent

### MCP

Point an MCP client at the running service:

```json
{
  "mcpServers": {
    "memnest": { "url": "http://127.0.0.1:3111/mcp" }
  }
}
```

Streamable HTTP is recommended because every client shares one server and one data directory. Use stdio only when a single client owns the store:

```json
{
  "mcpServers": {
    "memnest": {
      "command": "/absolute/path/to/memnest",
      "args": ["--mcp", "--data-dir", "/home/you/.memnest"]
    }
  }
}
```

### pi

```bash
cd memnest/pi-extension
npm install
pi install .
```

The extension registers the same tools, adds project-scoped Autocontext, and provides `/memnest` for status. See [`pi-extension/README.md`](pi-extension/README.md).

### HTTP and custom hosts

The HTTP API is available without MCP. [`adapters/generic-http`](adapters/generic-http) contains a dependency-free JSONL reference adapter.

## Tool contract

All hosts use six memory tools:

```text
memory_remember
memory_search
memory_get
memory_update
memory_delete
memory_feedback
```

An initialized vault adds four secret tools:

```text
secret_set
secret_get
secret_list
secret_delete
```

Search is project-scoped. A client must provide a project or explicitly request `project=all`. Every search returns a `recall_id`; feedback with both `recall_id` and `memory_id` changes only that returned memory. Delete moves a memory to trash instead of erasing it immediately.

### Projects are keyed by directory name

The inferred project is the basename of the working directory, not the full path. `/work/client-a/api` and `/personal/api` are therefore the same project, `api`, and share memories in both directions without warning. Until that changes, give the directories distinct names or pass an explicit `project` on every call. Replacing this key with one that distinguishes full paths is planned.

## Automatic context and conversation capture

`memnest hook` reads a host prompt event from stdin and prints a small project-scoped context block. If the working directory is unknown or the service is unavailable, it prints nothing and does not block the prompt.

Claude Code hook example:

```json
{
  "hooks": {
    "UserPromptSubmit": [
      {
        "hooks": [
          { "type": "command", "command": "memnest hook" }
        ]
      }
    ]
  }
}
```

`memnest watch` is the single transcript capture path for pi, Claude Code, and Codex:

```bash
memnest watch
memnest watch --once
memnest watch --backfill
```

It stores visible user and assistant text after credential redaction. It skips system and developer prompts, reasoning, reminders, tool calls and results, images, and subagent sidechains. Long turns are split into ordered searchable chunks. Repeated utterances stay distinct, while retries of the same transcript event remain idempotent.

The watcher follows the known transcript directories and stores offsets in `<data-dir>/watch-state.json`. A file offset advances only after all chunks were stored or repaired. `--backfill` imports earlier history; the default starts from new transcript data.

## Storage and dashboard

Memnest keeps its state under the selected data directory, normally `~/.memnest`:

```text
memory.db       SQLite records and metadata
text_index/     Tantivy BM25 index
vectors/        HNSW vector index
models/         local embedding model
master.key      vault key
archive/        plaintext JSONL of hard-deleted memories
watch-state.json
```

The dashboard at `http://127.0.0.1:3111` shows stored memories, searches, latency, processing failures, and recall feedback.

## Security

The server binds to `127.0.0.1` by default. A non-local bind is refused unless `MEMNEST_TOKEN` is non-empty, and clients must then send `Authorization: Bearer <token>`.

Regular memory text is local but not encrypted at rest. Credential-shaped strings are redacted before storage, but secrets belong in the vault, not in searchable memory. New stores create `<data-dir>/master.key` and use it to encrypt vault values with AES-256-GCM. If an existing vault cannot be decrypted with the available key, startup fails closed; there is no plaintext fallback anywhere, so back the key up separately from the data directory.

Deletion is not erasure. A deleted memory sits in trash for 30 days, and when trash is finally hard-deleted the full record is appended in plaintext to `<data-dir>/archive/YYYY-MM.jsonl`. Set `MEMNEST_ARCHIVE=0` to stop writing those files, and remove the existing `archive/` directory yourself if the text must be gone.

Do not expose port 3111 directly to the internet. The rest is in [`SECURITY.md`](SECURITY.md).

## Repository

| Directory | Role |
| --- | --- |
| [`core/`](core) | Rust server, CLI, indexes, MCP, vault, watcher, and dashboard |
| [`pi-extension/`](pi-extension) | Thin pi adapter and scoped Autocontext |
| [`adapters/`](adapters) | Integration contract and generic HTTP adapter |
| [`journal/`](journal) | Optional Markdown and git audit mirror, not a database backup |

Development checks:

```bash
cd core && cargo test
cd ../pi-extension && npm install && npm run build && npm run smoke
cd ../adapters/generic-http && node test.mjs
```

Engine attributions are in [`core/THIRD_PARTY_NOTICES.md`](core/THIRD_PARTY_NOTICES.md). Contributions follow [`CONTRIBUTING.md`](CONTRIBUTING.md).

## License

MIT © Blue-B
