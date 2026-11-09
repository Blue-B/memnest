# pi-memnest

<!-- markdownlint-disable MD013 -->

[English](README.md) | [한국어](README.ko.md)

Connects [pi](https://github.com/badlogic/pi-mono) to a running [memnest](https://github.com/Blue-B/memnest) service: five memory tools and task cards. The memory engine is not in this package, so start the core first.

## Install

Needs Node.js 20+, pi, and a memnest service (normally `http://127.0.0.1:3111`).

```bash
pi install npm:pi-memnest@0.5.0
```

If Memnest is already registered from a local path or another version, remove that entry first to avoid duplicate tools. A pinned version is not changed by `pi update --extensions`.

Set `MEMNEST_URL` for another address and `MEMNEST_TOKEN` if the core requires a token. Check the connection with `/memnest`: it shows service health, memory count, data directory, whether Autocontext is on, the active task cards, and which card this session records into.

## Tools

| Tool | Purpose |
| --- | --- |
| `memory_remember` | Save a memory. Kinds: `record`, `fact`, `rule`, `procedure`. `supersedes` replaces an older memory. |
| `memory_search` | Search the current workspace and `playbook`; `project=all` searches everything. |
| `memory_get` | Read a memory in pages (`offset`, `max_chars`) with nearby captures (`before`, `after`). |
| `memory_update` | Correct one memory. |
| `memory_delete` | Move one memory to trash. |
| `task_card` | Save, list, open, close, and finish task cards. |
| `secret_set`, `secret_get`, `secret_list`, `secret_delete` | Encrypted vault. Off unless `MEMNEST_EXPOSE_SECRET_TOOLS=1` is set before pi starts. |

```text
memory_remember text="Project X deploys on port 8320" memory_kind="fact"
memory_search query="Project X deployment port"
memory_remember text="Project X now deploys on port 8420" memory_kind="fact" supersedes="manual_..."
```

Without `project`, the extension sends pi's working directory and the core maps it to a private workspace. Ordinary memories are not encrypted at rest; enable the vault tools only for an agent you trust with plaintext credentials. The first search downloads the embedding model, so it takes longer.

## Task cards

A task card is a handoff note for work that continues over several days. It works without any setting.

```text
task_card action=save title="blog migration" text="# Goal ..."   # same title replaces the previous card
task_card action=list text="images"                              # rank by topic; omit text for newest
task_card action=get title="blog migration"                      # open and pin it for this session
task_card action=close title="blog migration"                    # stop recording when you move on
task_card action=done title="blog migration"                     # finish; save again to reopen
```

The first prompt of a session lists up to 20 active cards. An opened card is pinned into the system prompt, and every message you send is appended to it word for word, so a summary cannot drop what you said. Short one-time reminders cover the cases where models missed a card in real use: no card yet, a compaction, `memory_remember` with no card open, and an open card unsaved for 8 messages. Cards are shared with Claude Code, Codex, and other MCP clients through the core. [How it works, measurements, and limits](https://github.com/Blue-B/memnest/blob/main/docs/task-cards.md)

## Autocontext

Off by default. To add related memories to each prompt, set `MEMNEST_AUTOCONTEXT_MODE=balanced` before starting pi. Each card shows memory IDs so the agent can check the full text with `memory_get`; retrieved text is treated as reference data, not instructions.

| Variable | Default | Purpose |
| --- | --- | --- |
| `MEMNEST_AUTOCONTEXT_MODE` | `off` | `balanced` or `aggressive` turns recall on. |
| `MEMNEST_AUTOCONTEXT_DISABLE` | unset | `1` turns recall off regardless of mode. |
| `MEMNEST_AUTOCONTEXT_TOP` | `2` | Results per card. |
| `MEMNEST_AUTOCONTEXT_MAX_INJECTIONS` | `4` | Cards per session. |
| `MEMNEST_AUTOCONTEXT_MIN_SCORE` | `0.25` | Minimum score to inject. |
| `MEMNEST_AUTOCONTEXT_EXCLUDE` | `_superseded,default,root,global` | Collections never recalled. |
| `MEMNEST_AUTOCONTEXT_N` | `20` | Candidates fetched before ranking. |
| `MEMNEST_AUTOCONTEXT_MIN_LEN` | `16` | Shorter prompts are skipped. |
| `MEMNEST_AUTOCONTEXT_DOC_CHARS` | `240` | Characters kept per result. |
| `MEMNEST_AUTOCONTEXT_TIMEOUT_MS` | `1500` | The prompt goes on without a card after this. |

## Conversation capture

The extension stores no conversation itself. `memnest watch` on the core side captures pi, Claude Code, and Codex transcripts, so nothing is stored twice. See [conversation capture and history import](https://github.com/Blue-B/memnest/blob/main/docs/history-import.md).

## Development

```bash
npm install      # also builds dist/index.mjs, the file pi loads
npm run smoke
npm run e2e      # needs a core release build
```

[Changelog](./CHANGELOG.md), [third-party notices](./THIRD_PARTY_NOTICES.md)

## License

MIT © Blue-B
