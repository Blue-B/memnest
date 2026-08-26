# Operations guide

<!-- markdownlint-disable MD013 -->

Running memnest as a real service: requirements, install, retention, recovery, backup, and the checks to run before you change anything.

For what memnest is and how to connect an agent, start at the [README](../README.md).

## Requirements

The engine needs Git, a Rust toolchain with Rust 2024 edition support, and internet access on the first embedding operation so fastembed can download the configured model. Core CI builds and tests on Linux and Windows.

The optional package under `pi-extension/` lists its own runtime requirements in its package README.

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

`/stats` reports search latency from counters kept in process memory: how many searches ran since startup, the average, and the slowest one. Restarting the service resets them.

No query text is recorded. A slow search shows up as a number without leaving a copy of what was asked, and past conversation stays searchable through `memnest watch` transcripts instead.

Processing jobs that were queued or running when the service stopped are marked failed on the next startup, so interrupted work is visible instead of appearing active forever.

## Workspace scope and index recovery

When a client supplies `cwd`, memnest derives a non-reversible public workspace ID from the normalized absolute path. That workspace searches its own rows plus `playbook`. `project=all` is still the only implicit-scope bypass. Existing basename collections are included only while the basename belongs to one registered workspace; an ambiguous alias is disabled rather than guessed.

SQLite is authoritative. Every chunk insert, update, replacement, trash move, restore, and hard delete writes an `index_queue` row in the same transaction. Tantivy and HNSW changes clear that row only after both indexes are durable. Pending work, missing files, an old index schema, or a corrupt HNSW sidecar triggers a complete rebuild from all SQLite rows without a row-count cap. Index directories are staged beside the live directory and then renamed. Only one process may own a data directory for writing.

## Backup and restore

A backup may run while the service is active. The CLI uses SQLite `VACUUM INTO` for a consistent database snapshot, copies durable auxiliary files, omits rebuildable indexes and the model cache, then validates SQLite and encrypted vault rows before renaming the staged directory into place.

```bash
memnest --data-dir ~/.memnest --backup-dir ~/memnest-backup
```

Stop the service before restore. Restore rejects source and target paths that overlap. It builds and validates a temporary sibling first, then swaps directories. `--force` permits replacing a non-empty target but never deletes that target before the staged copy passes validation.

```bash
memnest --data-dir ~/.memnest --restore-dir ~/memnest-backup --force
```

The backup includes `master.key`, but keep a second protected copy of that key.

New vault rows use `$enc2$` ciphertext whose AES-GCM associated data includes the secret key or server name. Moving ciphertext to another row therefore fails decryption. Existing `$enc$` rows use the compatibility decrypt path and remain readable until rewritten. Model-facing secret tools are hidden unless `MEMNEST_EXPOSE_SECRET_TOOLS=1`; the localhost HTTP vault API remains available.

## CLI reference

```bash
memnest status                                   # service state and endpoint URL
memnest --data-dir ~/.memnest                    # HTTP API and MCP over POST /mcp
memnest --mcp --data-dir ~/.memnest              # stdio MCP server
memnest hook                                     # answer a host prompt hook with a context pack
memnest watch                                    # follow session transcripts and store new turns
memnest --doctor --data-dir ~/.memnest           # environment and store checks
memnest --warmup-embedding --data-dir ~/.memnest # download the model ahead of first use
memnest --help
```

Common options: `--host`, `--port`, `--data-dir`, `--backup-dir`, `--restore-dir`, `--import-jsonl`.

`hook` reads a host's hook payload on stdin and writes the reply on stdout, choosing the shape from the payload unless `--format` pins it. It never blocks a prompt: an unreachable service means no output and exit 0. `watch` is the single automatic capture path for Claude Code, pi, and Codex transcripts. It keeps a byte offset per file in `<data-dir>/watch-state.json`, follows new files from the end unless `--backfill` asks for existing history, and advances only after storage succeeds or an idempotent retry is confirmed. Both talk to the service over HTTP and take `--url`, falling back to `MEMNEST_URL`.

`--viewer-port` is deprecated. Every endpoint is served on `--port`.

## Configuration

| Variable | Effect |
| --- | --- |
| `MEMNEST_DATA_DIR` | Data directory, same as `--data-dir` |
| `MEMNEST_TOKEN` | Required for a non-local bind; clients send `Authorization: Bearer <token>` |
| `MEMNEST_EMBED_MODEL` | Embedding model, defaults to `intfloat/multilingual-e5-base` |
| `MEMNEST_EMBED_DIM` | Embedding dimension, defaults to 768 |
| `MEMNEST_TTL_AUTOLOG_DAYS` | Legacy AutoLog retention window, defaults to 30; transcript events are permanent |
| `MEMNEST_ARCHIVE` | Set to `0` to stop writing archive JSONL before hard deletion |
| `MEMNEST_EXPOSE_SECRET_TOOLS` | Set to `1` to expose four vault tools to MCP and the pi extension; hidden by default |
| `MEMNEST_REBUILD_INDEXES` | Set to `1` for a full SQLite-to-Tantivy/HNSW rebuild at startup |

## Development checks

Run everything with one command. It builds, runs each package suite against a scratch instance on its own port, checks the documentation contract, and prints the latest CI conclusions first:

```bash
scripts/preflight.sh
```

Prefer it over the individual commands: it runs every suite against one scratch instance, so a change in `core/` cannot break a dependent package while each suite separately stays green.

The underlying commands, if you want one at a time. Each block is a subshell, so every line starts from the repository root:

```bash
(cd core                  && cargo check && cargo test --locked -- --test-threads=1)
(cd pi-extension          && npm install && npm run build && npm run smoke)
(cd adapters/generic-http && node test.mjs)
```

A red run on GitHub Actions does not always mean a test failed. This repository is private, so Actions is billed, and a payment problem stops every job before its first step. Such a run finishes in seconds with no steps recorded and still reports failure, which looks identical to a real break in the run list. Check whether any step ran before trusting the colour.

Against a running service, `scripts/verify-contract.sh` checks the claims in this documentation instead of the code behind them. It calls every endpoint the docs advertise, compares the tool list in the README against what `tools/list` returns, confirms the removed surfaces answer 404, and asserts that the files and CLI subcommands described here exist. Point it at a scratch instance rather than your own store:

```bash
scripts/verify-contract.sh http://127.0.0.1:3150 ./target/release/memnest /tmp/memnest-scratch
```

Core tests run serially because environment-variable and vault lifecycle tests share process-global state and interfere with one another under the default parallel Rust test runner. CI uses the same flag.

A smoke test writes into whichever store answers the URL it is given. Never point one at the store you actually use; see [CONTRIBUTING.md](../CONTRIBUTING.md).
