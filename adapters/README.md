# Memnest adapters

Memnest keeps the core platform-neutral. Any agent can use the HTTP API or the stdio MCP server. An adapter only translates host lifecycle events into those stable operations.

## Supported surfaces

| Surface | Intended use |
| --- | --- |
| HTTP | Long-running local service shared by several clients. |
| stdio MCP | Tool access from MCP-compatible agents and editors. |
| `pi-extension/` | First-class pi tools, Autocontext, optional AutoLog, and `/memnest` status command. |
| `adapters/generic-http/` | Dependency-free JSONL reference adapter for other hosts. |

## Adapter contract

An adapter should provide these operations:

- `health`: check the local service
- `remember`: write a durable record, fact, rule, or procedure
- `message`: optionally capture a user or assistant message
- `summary`: store a session summary
- `search`: retrieve memory and receive a `recall_id`
- `feedback`: submit helpful, harmful, or ignored for a `recall_id`

Every write may include `adapter`, `adapter_version`, `session_id`, `cwd`, `source`, and `role`. Structured memories may additionally include `memory_kind`, `confidence`, `source_ids`, `supersedes`, and `verified_at`.

Adapters must not embed an agent loop or model client. They should remain small transport translators so Claude Code, Codex, OpenCode, Cursor, and future hosts can share the same local store.

## Generic HTTP reference

Send one JSON object per line:

```bash
printf '%s\n' \
  '{"type":"health"}' \
  '{"type":"remember","project":"demo","memory_kind":"fact","text":"Deploy uses port 8320"}' \
  '{"type":"search","project":"demo","query":"deploy port"}' \
  | node adapters/generic-http/memnest-adapter.mjs
```

Configuration:

- `MEMNEST_URL`, default `http://127.0.0.1:3111`
- `MEMNEST_TOKEN`, optional bearer token
- `MEMNEST_ADAPTER`, default `generic-http`
- `MEMNEST_TIMEOUT_MS`, default `3000`

Run the dependency-free contract test:

```bash
node adapters/generic-http/test.mjs
```

## MCP clients

Build the core and point any MCP-compatible client at the same executable and data directory:

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

This is the recommended integration path for Claude Code, Codex, OpenCode, Cursor, and similar clients when they support local stdio MCP. Use the generic HTTP adapter when a host exposes lifecycle hooks but not MCP.

## Contribution checklist

A new platform adapter should:

1. identify itself through `adapter` and `adapter_version`
2. default high-volume message or tool capture to off
3. redact credentials before logging errors
4. use bounded request timeouts
5. drain pending writes on clean shutdown
6. include a contract test that does not require the host application
