# Operations guide

<!-- markdownlint-disable MD013 -->

Running memnest as a real service: requirements, install, retention, recovery, backup, and the checks to run before you change anything.

For what memnest is and how to connect an agent, start at the [README](../README.md).

## Requirements

The engine needs Git, a Rust toolchain with Rust 2024 edition support, and internet access on the first embedding operation so fastembed can download the configured model. Core CI builds and tests on Linux and Windows.

The optional packages under `pi-extension/` and `journal/` list their own runtime requirements in their package README files.

## Run as a service

Build the release binary first:

```bash
cd core
cargo build --release
```

### Linux with systemd

```bash
scripts/preflight-linux.sh --user --bin target/release/memnest
scripts/install-linux.sh --user --bin target/release/memnest
curl -fsS http://127.0.0.1:3111/health
```

The user service stores data in `~/.memnest`. Use `--system` for a system service with data in `/var/lib/memnest`.

### WSL

Run the PowerShell installer from the repository root and point `RepoPath` at the `core` directory inside the WSL distribution:

```powershell
.\core\scripts\install-wsl.ps1 -Distro Ubuntu-24.04 -RepoPath /home/<user>/memnest/core
```

The script installs the Linux user service and registers a Windows logon task that starts it.

### Windows service

Build `memnest.exe` from `core`, then run an administrator PowerShell prompt from the repository root:

```powershell
.\core\scripts\preflight-windows.ps1
.\core\scripts\install-windows.ps1 -BinPath .\core\target\release\memnest.exe
```

The native service stores data in `%ProgramData%\Memnest\data` and binds to localhost.

The service wrapper is WinSW. The installer pins one version and its SHA-256 and verifies the file before installing it, whether the wrapper was downloaded or found next to the script, and it deletes the file and stops on a mismatch. Overriding `-WinSWVersion` therefore requires passing the matching `-WinSWSha256` (or dropping a `WinSW-x64.exe.sha256` beside the wrapper); the install refuses to run elevated against bytes it cannot check.

Uninstallers for each layout are in `core/scripts/`.

## Retention and recovery

Retention depends on memory type and importance:

- manual and consolidated memories do not expire automatically
- knowledge, decision, and preference memories do not expire automatically
- new transcript AutoLog records with `.transcript` source and event identity do not expire automatically
- legacy AutoLog records keep the configurable 30-day default (`MEMNEST_TTL_AUTOLOG_DAYS`)
- filtered records expire after 7 days
- pinned memories are excluded from automatic retention and from normal prune requests

Expired and manually deleted memories move to `_trash` first, where search does not return them. Restore by id while they are still there:

```bash
curl -s http://127.0.0.1:3111/restore \
  -H 'content-type: application/json' \
  -d '{"ids":["manual_..."]}'
```

Trash older than 30 days is hard-deleted. Before that deletion the full record is appended to `<data-dir>/archive/YYYY-MM.jsonl`. Set `MEMNEST_ARCHIVE=0` to disable archive files.

Preview a cleanup without changing data:

```bash
curl -s http://127.0.0.1:3111/prune \
  -H 'content-type: application/json' \
  -d '{"project":"root","older_than_days":30,"dry_run":true}'
```

Real prune and lifecycle operations append records to `<data-dir>/audit.log`. `/health` reports the latest lifecycle run. `/stats` reports collection sizes, age buckets, disk use, and cleanup recommendations.

## Monitoring

`/operations` returns recent recall events and processing jobs as JSON: query scope, result count, latency, adapter, outcome, and job state. The dashboard at `http://localhost:3111/` shows the same data with feedback controls.

Operational history is capped at 90 days and holds redacted queries and status metadata, never memory bodies or secret values.

Processing jobs that were queued or running when the service stopped are marked failed on the next startup, so interrupted work is visible instead of appearing active forever.

## Backup and restore

Stop any service using the data directory before copying or restoring it.

```bash
memnest --data-dir ~/.memnest --backup-dir ~/memnest-backup
memnest --data-dir ~/.memnest --restore-dir ~/memnest-backup --force
```

`memnest-journal` is a readable audit mirror, not a replacement for this data-directory backup. A git revert in the journal changes the journal files only.

## CLI reference

```bash
memnest status                                   # service state and dashboard URL
memnest dashboard                                # canonical clickable dashboard URL
memnest --data-dir ~/.memnest                    # HTTP API, dashboard, and MCP over POST /mcp
memnest --mcp --data-dir ~/.memnest              # stdio MCP server
memnest hook                                     # answer a host prompt hook with a context pack
memnest watch                                    # follow session transcripts and store new turns
memnest --doctor --data-dir ~/.memnest           # environment and store checks
memnest --warmup-embedding --data-dir ~/.memnest # download the model ahead of first use
memnest --help
```

Common options: `--host`, `--port`, `--data-dir`, `--backup-dir`, `--restore-dir`, `--import-jsonl`.

`hook` reads a host's hook payload on stdin and writes the reply on stdout, choosing the shape from the payload unless `--format` pins it. It never blocks a prompt: an unreachable service means no output and exit 0. `watch` is the single automatic capture path for Claude Code, pi, and Codex transcripts. It keeps a byte offset per file in `<data-dir>/watch-state.json`, follows new files from the end unless `--backfill` asks for existing history, and advances only after storage succeeds or an idempotent retry is confirmed. Both talk to the service over HTTP and take `--url`, falling back to `MEMNEST_URL`.

`--viewer-port` is deprecated. The dashboard is served on `--port` alongside the API.

## Configuration

| Variable | Effect |
| --- | --- |
| `MEMNEST_DATA_DIR` | Data directory, same as `--data-dir` |
| `MEMNEST_TOKEN` | Required for a non-local bind; clients send `Authorization: Bearer <token>` |
| `MEMNEST_EMBED_MODEL` | Embedding model, defaults to `intfloat/multilingual-e5-base` |
| `MEMNEST_EMBED_DIM` | Embedding dimension, defaults to 768 |
| `MEMNEST_TTL_AUTOLOG_DAYS` | Legacy AutoLog retention window, defaults to 30; transcript events are permanent |
| `MEMNEST_ARCHIVE` | Set to `0` to stop writing archive JSONL before hard deletion |

## Development checks

Each block below is a subshell, so every line starts from the repository root instead of from wherever the previous line left you:

```bash
(cd core                  && cargo check && cargo test --locked -- --test-threads=1)
(cd pi-extension          && npm install && npm run build && npm run smoke)
(cd journal               && npm install && npm run smoke)
(cd adapters/generic-http && node test.mjs)
```

Core tests run serially because environment-variable and vault lifecycle tests share process-global state and interfere with one another under the default parallel Rust test runner. CI uses the same flag.

`journal`'s smoke test has no default target and exits 2 until `MEMNEST_URL` and `MEMNEST_DB` point at a scratch instance. Never point it at the store you actually use; see [CONTRIBUTING.md](../CONTRIBUTING.md).
