# memnest-journal

<!-- markdownlint-disable MD013 -->

A Markdown and git audit mirror for local memnest data.

`memnest-journal` exports selected memnest database records into a readable directory and can commit that directory to git. It is useful for reviewing what an agent stored, comparing snapshots, and keeping an off-machine audit copy in a private repository.

It is not the memnest engine, a live database replica, or an automatic rollback system. A git revert changes the journal files only. Use the core backup and restore commands when you need to recover the actual memnest data directory.

## What it exports

A journal has this layout:

```text
~/.memnest/journal/
├── README.md
├── .gitignore
├── chunks/<project>/<id>.md
├── facts/<hash>.md
├── notes/<key>.md
├── secrets/<key>.enc.md
└── sessions/<project>/<id>.md
```

Each record is rendered as Markdown with frontmatter. Memory chunks are grouped by project, facts use stable hashed filenames, notes use their key, and sessions are grouped by project.

The exporter reads the SQLite database directly in read-only mode. It does not export vector index files, the text index, the database, or `master.key`.

## Current status

The public npm registry does not currently provide `memnest-journal`. Install it from this repository.

Version 0.1.0 is an audit-oriented exporter with a limited import path:

- export supports chunks, facts, notes, stored secret records, and session summaries
- sync exports, stages, and commits the journal, with an optional push
- import applies modified chunk files by adding a new memory with a provenance marker
- import does not replace or delete the original memory
- facts, sessions, and secrets are read-only in the journal
- modified note files are detected but are not written back to memnest in this version

## Requirements

- Git
- a local memnest data directory for export
- a running memnest HTTP service for import
- Bun, or a Node.js runtime that provides `node:sqlite`

This checkout was tested with Node.js 22.22 and Bun. The fallback code can use `better-sqlite3` when that module is installed and resolvable, but it is not included as a package dependency.

## Install from source

From the memnest repository root:

```bash
npm install -g ./journal
```

Confirm the CLI is available:

```bash
pjournal --help
```

## Quick start

Initialize the default journal path:

```bash
pjournal init
pjournal sync
pjournal status
pjournal log -n 10
```

The default locations are:

- journal: `~/.memnest/journal`
- memnest database: `~/.memnest/memory.db`
- memnest HTTP service: `http://127.0.0.1:3111`

If your engine uses another data directory, pass the matching `--db` path. Use `--dir` on every command when the journal is not at the default path.

### Add a private remote

`pjournal init` creates a local git repository but does not add a remote. Configure one before using `--push`:

```bash
cd ~/.memnest/journal
git remote add origin <private-repository-url>
pjournal sync --push
```

A private remote is recommended because ordinary memory and note text is not encrypted.

## Review a stored memory

A chunk file looks like this:

```markdown
---
id: manual_1a6465da04984426
project: playbook
chunk_type: manual
importance: preference
session_id: ""
sensitive: false
created_at: 2026-05-17T08:26:15.658664381+00:00
---
Project X deploys on port 8320.
```

Use normal git commands to inspect journal history:

```bash
git -C ~/.memnest/journal log --oneline
git -C ~/.memnest/journal diff HEAD~1 HEAD
git -C ~/.memnest/journal blame chunks/playbook/manual_1a6465da04984426.md
```

These commands inspect the exported history. They do not change the live memnest database.

## Import a corrected chunk

Start the memnest HTTP service, edit an existing file under `chunks/`, and run:

```bash
pjournal import
```

Version 0.1.0 posts the edited body to `/add` as a new memory and appends a marker such as:

```html
<!-- memnest-journal: edited-from=manual_... at=2026-05-17T08:26:15.658Z -->
```

The original memory remains in memnest and can still appear in search. If you need an in-place correction, use the core `/update` API, an MCP `memory_update` tool, or pi's `memory_update` tool instead.

`pjournal import` only considers modified or staged files under `chunks/` and `notes/`. In this release, chunk edits are applied, note edits are counted as pending, and facts, sessions, and secrets are not imported.

## Sync and filter safety

`pjournal sync` performs a full export, removes journal files not present in that export, stages all changes, and commits them. This is appropriate for an unfiltered full snapshot.

Do not combine `pjournal sync` with `--project` or `--since` in version 0.1.0. The current pruning behavior can remove journal files that were excluded by the partial export.

For a filtered read-only export, use `pjournal export` without `--prune`:

```bash
pjournal export --project project-a,project-b
pjournal export --since 2026-07-01T00:00:00Z
```

Review the working tree before committing filtered output.

## Commands

```text
pjournal init   [dir]                     initialize a journal repository
pjournal export [options]                 export database records to Markdown
pjournal sync   [--push] [--message text] export, prune, stage, and commit
pjournal import                           apply supported modified files
pjournal log    [-n N]                    show journal commit history
pjournal status                           show pending journal changes
```

Common options:

```text
--dir <path>          journal directory, default ~/.memnest/journal
--db <path>           memnest SQLite database, default ~/.memnest/memory.db
--url <url>           memnest HTTP service, default http://127.0.0.1:3111
--project <a,b,c>     filter exported chunks by project
--since <iso>         filter chunks by timestamp
--include-sensitive   include chunks marked sensitive=true
--prune               remove unmatched files during pjournal export
--push                push after pjournal sync
--remote <name>       git remote, default origin
--branch <name>       ref passed to git push, default main
--message <text>      commit message for pjournal sync
```

`--branch` does not create or switch branches. It is passed to `git push`, so create and check out the intended branch with git first.

## Security boundaries

- Sensitive chunks are skipped unless `--include-sensitive` is supplied.
- Ordinary chunks, notes, facts, and session summaries are plaintext Markdown. Use a private repository.
- Secret export copies the value stored in the database without decrypting it. A normal memnest installation stores secret values as `$enc$...` ciphertext, but the journal does not independently verify that format before writing the file. Inspect the generated `secrets/` tree before any push.
- `master.key`, SQLite files, and index directories are added to the generated `.gitignore`.
- The CLI uses your existing git configuration. It does not enable signed commits or enforce branch protection.
- Git history is useful audit evidence, but this package does not claim compliance with a particular standard.

## Backup and recovery

For actual recovery, stop the engine and back up or restore the complete core data directory:

```bash
memnest --data-dir ~/.memnest --backup-dir ~/memnest-backup
memnest --data-dir ~/.memnest --restore-dir ~/memnest-backup --force
```

The journal can help identify what changed, but it does not contain the complete database and indexes required for a lossless restore.

## Development

```bash
npm run smoke
```

The smoke test covers initialization, export, git sync, chunk import, stored secret export, and sensitive-chunk filtering against a temporary memnest instance.

## Related documentation

- [memnest root README](../README.md) for engine installation, lifecycle, backup, and security
- [CHANGELOG.md](./CHANGELOG.md) for package changes and known limitations
- [pi-memnest](../pi-extension/README.md) for the pi integration

## License

MIT © Blue-B
