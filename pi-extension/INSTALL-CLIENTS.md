# Install palimpsest as a memory layer in your AI client

This guide registers the core `palimpsest --mcp` stdio server in 7 popular
clients. The same `~/.palimpsest/` data store is shared between all of them
and any local pi/opencode instance — memories you record in one client are
searchable in every other.

All clients below use **the same command**:

```text
palimpsest --mcp
```

Optional flags you may add to `args`:

| Flag                          | Effect                                                     |
| ----------------------------- | ---------------------------------------------------------- |
| `--data-dir <path>`           | Override `~/.palimpsest` (e.g. shared NAS, encrypted vol). |
| `--port 3111` / `--host`      | Already the server defaults; only override for testing.    |
| `--warmup-embedding`          | Pre-load the embedding model (slower start, faster first query). |

---

## 0. Prerequisites

```bash
# Install palimpsest server itself (Rust binary).
# Pick ONE of:

# A) From source (any platform with cargo) — core lives in the monorepo's core/ subdir
git clone https://github.com/Blue-B/palimpsest
cd palimpsest/core && cargo build --release
cp target/release/palimpsest ~/.local/bin/

# B) From release (Linux/macOS x64 and aarch64)
curl -fsSL https://github.com/Blue-B/palimpsest/releases/latest/download/palimpsest-$(uname -s)-$(uname -m).tar.gz | tar xz -C ~/.local/bin

# Verify
palimpsest --version   # → 0.1.0 or later
palimpsest --mcp --help
```

You do **not** need to run `palimpsest` as a separate service when using
the `--mcp` mode — each client spawns its own short-lived stdio server.
But if you also want pi/curl access, see §8 (systemd service).

---

## 1. Claude Desktop

**Config path**
- macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`
- Windows: `%APPDATA%\Claude\claude_desktop_config.json`
- Linux: not officially supported (use Cline or Continue instead).

```json
{
  "mcpServers": {
    "palimpsest": {
      "command": "palimpsest",
      "args": ["--mcp"]
    }
  }
}
```

**WSL users on Windows Claude Desktop**:
```json
{
  "mcpServers": {
    "palimpsest": {
      "command": "wsl.exe",
      "args": ["-e", "palimpsest", "--mcp"]
    }
  }
}
```

Restart Claude Desktop. The server should appear in the MCP icon
(bottom of the chat input) with 17 tools.

---

## 2. Cursor

**Config path**: `~/.cursor/mcp.json` (global) or `<project>/.cursor/mcp.json`

```json
{
  "mcpServers": {
    "palimpsest": {
      "command": "palimpsest",
      "args": ["--mcp"]
    }
  }
}
```

Or via the GUI: `Settings → Features → MCP → + Add new MCP server`,
select `stdio`, command `palimpsest`, args `--mcp`.

---

## 3. Cline (VSCode extension)

**Config path**:
- macOS/Linux: `~/Library/Application Support/Code/User/globalStorage/saoudrizwan.claude-dev/settings/cline_mcp_settings.json`
- Windows: `%APPDATA%\Code\User\globalStorage\saoudrizwan.claude-dev\settings\cline_mcp_settings.json`

```json
{
  "mcpServers": {
    "palimpsest": {
      "command": "palimpsest",
      "args": ["--mcp"],
      "disabled": false,
      "autoApprove": ["memory_search", "memory_stats", "memory_health"]
    }
  }
}
```

`autoApprove` lets memory reads happen without per-call confirmation.
Writes (`memory_add`, `secret_set`, …) still need explicit approval.

---

## 4. Continue.dev

**Config path**: `~/.continue/config.json`

```json
{
  "experimental": {
    "modelContextProtocolServers": [
      {
        "transport": {
          "type": "stdio",
          "command": "palimpsest",
          "args": ["--mcp"]
        }
      }
    ]
  }
}
```

Continue ≥ 0.9.200 picks the tools up automatically on next chat.

---

## 5. Zed

**Config path**: `~/.config/zed/settings.json`

```json
{
  "context_servers": {
    "palimpsest": {
      "command": {
        "path": "palimpsest",
        "args": ["--mcp"]
      }
    }
  }
}
```

Restart Zed. Tools become available to `@assistant` panels.

---

## 6. opencode

opencode has a dedicated plugin (`palimpsest-opencode`) that runs in
the same process — no extra stdio server needed.

```bash
# Install plugin
git clone https://github.com/Blue-B/palimpsest-opencode \
  ~/.config/opencode/plugins/palimpsest-opencode
cd ~/.config/opencode/plugins/palimpsest-opencode
npm install && npm run build
```

opencode auto-discovers plugins on next start.

---

## 7. pi (this repository)

pi uses the HTTP bridge (`pi-palimpsest`, this repo) rather than stdio,
because pi extensions live in a long-running shared process and reusing
a single HTTP server is cheaper.

In `~/.pi/agent/settings.json`:
```json
{
  "packages": [
    "pi-palimpsest"
  ]
}
```

or (development install):
```json
{
  "packages": [
    "../../pi-palimpsest"
  ]
}
```

Restart pi. 12 memory tools appear; the remaining 5 (graph_query,
lifecycle_run, note_get/set, server_*) are available only through the
stdio MCP mode — register palimpsest in Claude Desktop or Cursor for
those features.

---

## 8. Optional: shared HTTP server (systemd)

When you want pi + curl + the web dashboard (`http://127.0.0.1:3111/`) to
work alongside MCP clients, run palimpsest as a long-lived service.

`~/.config/systemd/user/palimpsest.service`:
```ini
[Unit]
Description=palimpsest memory server
After=default.target

[Service]
ExecStart=%h/.local/bin/palimpsest
Restart=on-failure
RestartSec=2s

[Install]
WantedBy=default.target
```

```bash
systemctl --user daemon-reload
systemctl --user enable --now palimpsest
systemctl --user status palimpsest
```

Stdio MCP clients (Claude Desktop etc.) still spawn their own short-lived
`palimpsest --mcp` and access the same `~/.palimpsest/memory.db` over the
SQLite file — they coexist with the long-running HTTP server.

---

## 9. Verifying

After registration, ask the assistant:

> Run `memory_stats`.

You should see something like:
```json
{ "total_chunks": 708, "total_sessions": 2, "total_facts": 3, "total_notes": 0 }
```

Or in any shell:
```bash
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | palimpsest --mcp
```
should return a 17-item array.

---

## 10. Troubleshooting

| Symptom | Likely cause | Fix |
|---|---|---|
| Client shows 0 tools | `palimpsest` not in PATH | Use absolute path: `/home/you/.local/bin/palimpsest`. |
| "spawn ENOENT" on Windows | `palimpsest.exe` missing | Use the `wsl.exe -e palimpsest` form, or cross-compile. |
| Tools listed but every call errors | Two clients fighting for the same `--data-dir` write lock | Use the systemd service (§8) and remove `--mcp` from non-primary clients (they can use HTTP via a tiny wrapper, ask in issues). |
| `memory_search` returns empty | Embedding model still warming up | Wait ~5–10s after server start, or add `--warmup-embedding`. |
| `secret_get` returns "decryption failed" | `master.key` mismatch (moved data dir without key) | Copy `~/.palimpsest/master.key` to the new data dir. |
