# Operations guide

<!-- markdownlint-disable MD013 -->

Running memnest as a real service: requirements, install, retention, recovery, backup, and the checks to run before you change anything.

For what memnest is and how to connect an agent, start at the [README](../README.md).

## Requirements

Core v0.3.0 provides Linux x86_64 and Arm64 binaries, which need neither Git nor Rust. Its integrated setup requires Python 3.11 or newer to validate client configuration. Building from source also needs Git and a Rust toolchain with Rust 2024 edition support. The first embedding operation needs internet access to download the configured model. macOS installation and packaging code is present, but native launchd installation, restart, and removal remain unverified.

Linux setup requires `systemctl` and `systemd-analyze` from systemd. The latter is used read-only to inspect user or system unit search paths before an upgrade. The optional package under `pi-extension/` lists its own runtime requirements in its package README.

## Run as a service

Build the release binary first:

```bash
cd core
cargo build --release
```

### One-command setup

For the v0.3.0 Linux release, download and review the installer before running it:

```bash
curl -fsSL https://raw.githubusercontent.com/Blue-B/memnest/v0.3.0/core/scripts/install.sh \
  -o /tmp/memnest-install.sh
VERSION=v0.3.0 bash /tmp/memnest-install.sh --user
```

From a source checkout, run this from `core/` after building:

```bash
scripts/setup.sh --user --bin target/release/memnest
```

The downloader still recognizes the older v0.2.1 archive layout, but refuses to run that legacy installer over an existing service or with `--autocontext`.

User-mode setup installs and starts the server and conversation watcher; setup detects Claude Code, Codex, Cursor, and pi, and merges the supported MCP entries. Automatic recall is not installed by default: add `--autocontext` to `setup.sh` or `install.sh` to opt in to the Claude Code prompt hook. Existing Memnest client entries are left unchanged. Every changed client file is first copied byte-for-byte to a private timestamped directory. That directory contains the manifest and its own restore script, so uninstalling the service does not remove the rollback tool. A newly created file gets an `ABSENT` marker. Setup finishes by remembering, searching for, and trashing a unique scratch memory. The first round trip can download the embedding model.

For an existing installation, `python3 ~/.local/share/memnest/scripts/setup-clients.py --autocontext` opts in without reinstalling the service. Existing prompt hooks are preserved, not silently disabled on upgrade; remove only the Memnest command from the client's `UserPromptSubmit` configuration if you no longer want it. pi's separate default-off policy is controlled by `MEMNEST_AUTOCONTEXT_MODE` and reported by `/memnest`. Explicit tools and `memnest watch` do not depend on either recall hook.

Setup prints an exact restore command using the timestamped directory. Restore first checks the post-setup SHA-256 for every client file and refuses to overwrite later user or client changes. Review conflicting files before using `--force-restore`, which intentionally replaces those changes with the pre-setup copies.

```bash
python3 ~/.memnest/setup-backups/<timestamp>/setup-clients.py \
  --restore ~/.memnest/setup-backups/<timestamp>/manifest.json
```

On Linux this uses systemd and stores user data in `~/.memnest`. Use `--system` for `/var/lib/memnest`; system mode intentionally does not run a root transcript watcher, so capture must be run by the desktop user. The macOS implementation targets `io.memnest.service` and `io.memnest.watch` in `~/Library/LaunchAgents`, with logs in `~/Library/Logs/Memnest`. It is source-only and unverified on an actual Mac.

The lower-level Linux installer remains available:

```bash
scripts/preflight-linux.sh --user --bin target/release/memnest
scripts/install-linux.sh --user --bin target/release/memnest
```

### Linux service upgrades

The source installer keeps the existing server and watcher units instead of replacing them with templates. For a supported packaged layout, custom `Environment=` entries, logging, permissions, and other directives remain intact. When `MEMNEST_HOST` and `MEMNEST_PORT` are omitted, setup reads the installed endpoint and uses it for service probes, new client entries, and the scratch round trip. Existing client entries are deliberately not rewritten; if you intentionally change the endpoint, update those entries separately.

To explicitly change a supported endpoint assignment, pass the value to setup, for example `MEMNEST_PORT=3222 scripts/setup.sh --user --bin target/release/memnest`. Before modifying a unit, the installer writes a private `*.backup.*` copy beside it and prints the exact restore command. Unchanged units are not rewritten. System-mode staging uses a unique private temporary directory, not a shared `/tmp/memnest.service` file.

This is not a general systemd configuration editor. It requires the packaged `ExecStart` and standalone, unquoted `Environment=MEMNEST_HOST=...` / `Environment=MEMNEST_PORT=...` assignments. It stops before changes for custom commands, external environment files, ambiguous endpoint assignments, conflicting watcher ports, symlink units, or detected drop-ins. Packaged setup supports IPv4 loopback (`127.0.0.1` or `localhost`); other binds need manual configuration. These refusals preserve existing settings rather than silently resetting them.

For an unsupported customization, back up the database and service configuration, stop the relevant service, and replace only the binary at the path in your reviewed `ExecStart`. Leave the units and drop-ins intact, restart the service, then check its actual endpoint. Do not remove custom configuration just to make setup proceed. To roll back an intentional unit change, stop the affected services, run the printed restore commands for both changed units, reload systemd, and restart them. Unit backups do not replace a backup of the previous binary and database.

`python3 core/scripts/test-install-linux.py` checks user/system file effects and setup endpoint agreement in disposable paths. Its service manager, privilege command, and HTTP probe are test doubles, not a native system-level installation.

### Rollback and uninstall

Restore client configuration before using `--remove-data`, because setup manifests live under the data directory. Uninstall keeps memories, logs, and config backups by default:

```bash
~/.local/share/memnest/scripts/uninstall-linux.sh --user # Linux
~/.local/share/memnest/scripts/uninstall-macos.sh         # macOS
```

Pass `--remove-data` only when the retained store and setup backups should be permanently deleted. Uninstall does not guess which shared client entries the user still wants; use the exact manifest restore command first to roll those back.

Validate an installed macOS service with `scripts/validate-installed-macos.sh`. This repository's Linux environment can syntax- and contract-check launchd files, but only a logged-in macOS host can exercise `launchctl`, launch persistence, restart, reinstall, uninstall, and native x86_64/arm64 execution. The release workflow is configured to build and run native binaries, but does not test the launchd lifecycle. That configuration is not a claim that the current changes have run in hosted CI. The macOS packaging code does not add signing or notarization; a future downloaded artifact may require Gatekeeper approval.

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

`memnest status` reports the server address, the conversation watcher's heartbeat, and the last successful transcript write. A missing or stale heartbeat is printed as a warning even when the memory server itself is reachable.

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

Run everything with one command. It builds, runs each package suite against a scratch instance on its own port, and checks the documentation contract:

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

Against a running service, `scripts/verify-contract.sh` checks the claims in this documentation instead of the code behind them. It calls every endpoint the docs advertise, compares the tool list in the README against what `tools/list` returns, confirms the removed surfaces answer 404, and asserts that the files and CLI subcommands described here exist. Point it at a scratch instance rather than your own store:

```bash
scripts/verify-contract.sh http://127.0.0.1:3150 ./target/release/memnest /tmp/memnest-scratch
```

Core tests run serially because environment-variable and vault lifecycle tests share process-global state and interfere with one another under the default parallel Rust test runner. CI uses the same flag.

A smoke test writes into whichever store answers the URL it is given. Never point one at the store you actually use; see [CONTRIBUTING.md](../CONTRIBUTING.md).

## Read-only first-use diagnosis

Target the endpoint explicitly:

```bash
core/target/debug/memnest --host 127.0.0.1 --port 3111 status --diagnose
# If authentication is configured, supply the existing MEMNEST_TOKEN privately
# in the environment; do not paste it into logs, URLs, or shell history.
```

This performs only `GET /health` and MCP `tools/list` (POST `/mcp`), with a five-second
combined HTTP budget, 256 KiB per-response limit, no redirects or proxies, and no
DB/index/model initialization. Exit 0 means healthy contract + advertised scoped
search/paged-get fields, **not** successful embedding or complete client compatibility.
Exit 1 distinguishes transport/unreachable, HTTP 401/403 authentication failure,
invalid/old capability contracts, oversized replies and timeout. It never sends
memory text, invokes a memory tool, changes client settings, or prints the token.
Use only an endpoint you trust: explicit non-loopback HTTP sends the bearer token
without TLS. The flag uses `--host`/`--port`, not `MEMNEST_URL`.

The v0.3.0 health contract reports whether the embedding model is currently loaded,
the number of pending index operations, and whether a rebuild is required. An unloaded
model is normally lazy, not failed: the first write or search initializes it. Older
health contracts are shown as unknown. Remote capture remains unknown because service
health does not expose the watcher's state. `memnest --data-dir <watcher-state-dir>
status` reads that local heartbeat and last stored time, which may differ from the
server data directory. A fresh heartbeat is not proof that every transcript was
indexed. Diagnostic mode does not read the watcher state file. Existing `--doctor` is
**not** this read-only path: it writes a test file and opens indexes; do not use it as
a non-mutating live probe.

Recovery: unreachable → check the intended endpoint and service logs; authentication
failure → compare the server/client token configuration without printing it;
capability mismatch → use matching source core/client builds or released features.
Do not delete the DB or reinstall client settings to fix a token/schema mismatch.
For data recovery, use the offline backup/restore procedure above, retaining the
previous binary and complete data directory; a client-config backup is not a memory backup.

### Disposable first-memory / retrieval check

For a **temporary demo**, not an install or import of your real conversations:

```bash
cd core && cargo build --offline && cd ..
python3 scripts/test-status-diagnostic.py
python3 scripts/evaluate-coding-memory.py --output /tmp/memnest-evaluation.json
```

The evaluation requires the already populated `core/target/test-model-cache`; it
refuses a missing cache and blocks model downloads. The current fixture has 16 records
and 48 authored questions, including 26 questions whose requested fact is absent. It
creates a new temporary DB,
a random loopback endpoint and a disposable token, checks service identity before
writes, stores the explicit fixture, searches it, and removes only its own data and
child process. JSON results and the neighboring `.daemon.log` remain at the output
path. No real installation is probed. Actual retained user data belongs in the
configured service data directory, never this temporary demo. Setup's separate
scratch roundtrip *does* write to its configured service before trashing the probe.
Automatic recall remains opt-in; evaluation does not install capture or recall.

`scripts/preflight.sh` remains a legacy release check,
not this safe demo: it uses fixed/shared defaults and may download a model. Do not
use it for isolated no-download validation. See the fixed fixture and measured
limitations in [the product plan](product-upgrade-plan.ko.md).

Diagnostic exit status is machine-readable: **0** for the two advertised contracts,
**1** for a diagnostic failure. Failure categories currently use human-readable fixed
messages, not distinct numeric codes or JSON. HTTP response strings (including
`version`, errors and tool descriptions) are not echoed: they may contain credentials,
server-local paths or terminal escapes. The explicitly requested endpoint is printed;
the local `--data-dir` and remote health `data_dir` are not. Use ordinary `status` for
the CLI version/local capture report. The stored evaluation JSON retains its original
pre-hardening diagnostic transcript rather than rewriting measured evidence.

Search scores are composite ranking scores, **not relevance probabilities**. The
fixture's negative-empty rate measures direct search, not automatic-context injection
abstention. Optional pi recall has separate scope/type/score gates and is off by
default; this evaluation does not prove how often those gates inject or abstain.
