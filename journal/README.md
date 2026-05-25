<p align="center">
  <img src="./docs/logo.png" alt="palimpsest-journal" width="160" />
</p>

<h1 align="center">palimpsest-journal</h1>

<p align="center">
  <strong>Your AI memory as a git-backed markdown repo you own, edit, diff, and revert.</strong>
  <br/>
  <em>The missing human layer for any AI memory system.</em>
</p>

<p align="center">
  <a href="https://www.npmjs.com/package/palimpsest-journal"><img src="https://img.shields.io/npm/v/palimpsest-journal.svg?style=flat&color=blue" alt="npm version" /></a>
  <a href="https://www.npmjs.com/package/palimpsest-journal"><img src="https://img.shields.io/npm/dm/palimpsest-journal.svg?style=flat&color=blue" alt="downloads" /></a>
  <a href="https://github.com/Blue-B/palimpsest-journal/blob/main/LICENSE"><img src="https://img.shields.io/npm/l/palimpsest-journal.svg?style=flat&color=green" alt="license" /></a>
  <a href="https://github.com/Blue-B/palimpsest-journal/actions/workflows/ci.yml"><img src="https://img.shields.io/github/actions/workflow/status/Blue-B/palimpsest-journal/ci.yml?branch=main&style=flat&label=CI" alt="CI" /></a>
  <a href="https://github.com/sponsors/Blue-B"><img src="https://img.shields.io/badge/sponsor-❤-ea4aaa.svg?style=flat" alt="sponsor" /></a>
</p>

---

`palimpsest-journal` is the missing **human layer** on top of
[palimpsest](https://github.com/badlogic/palimpsest). It exports every
memory chunk, fact, note, session summary, and (encrypted) secret to a
plain markdown tree under git — then lets you `diff`, `revert`, `push`,
review, and edit them by hand like any other source file.

It does not replace palimpsest, mem0, agentmemory, Letta, or Zep. It
turns the one you already have into something you can **trust and
collaborate on**, because the memory finally lives where you already
have tools: in git.

---

## Why this exists

Every persistent-memory product today treats memory as a black box:

- You can read it through a search API.
- You **can't** see what changed when.
- You **can't** revert a bad write.
- You **can't** edit a wrong fact.
- You **can't** review what a teammate's agent learned.
- You **can't** ship the memory through your existing PR + audit
  pipeline.

`palimpsest-journal` makes memory a first-class versioned artifact:

| Pain                                        | Black-box memory     | palimpsest-journal                |
| ------------------------------------------- | -------------------- | --------------------------------- |
| AI learned a wrong fact                     | Live with it / wipe  | `vim chunks/…` + `pjournal import` |
| AI wrote a 1-million-token mess overnight   | Manual cleanup       | `git revert HEAD`                  |
| Need to share memory across machines        | Custom sync code     | `git push`                         |
| Audit: who/when added this memory?          | Best-effort logs     | `git log` + `git blame`            |
| Team needs to review what an agent learned  | Slack screenshots    | open a PR on the journal repo      |
| Compliance asks for tamper-evident history  | Hope DB is append-only | git commit hashes, signed commits |

---

## Install

```bash
# zero native deps — works on Node 20+ and Bun
npm install -g palimpsest-journal
```

Requires a running palimpsest server (default `http://127.0.0.1:3111`)
and access to its sqlite store (default `~/.palimpsest/memory.db`).

## Quick start

```bash
pjournal init ~/memory-journal             # one-time
pjournal sync --push                        # export DB -> commit -> git push

# the AI keeps writing memories during the day...
pjournal sync                               # incremental commit, no push

# you spot a wrong memory
vim ~/memory-journal/chunks/myproject/manual_abc.md
pjournal import                             # push your edit back into palimpsest

# the AI overnight wrote 200 bad memories
cd ~/memory-journal && git log --oneline -10
git revert <bad-commit>                     # restore to last-known-good
```

## What's in the repo

```
~/memory-journal/
├── README.md
├── .gitignore                # never commit master.key, sqlite, vector index
├── chunks/<project>/<id>.md  # one memory per file, frontmatter + body
├── facts/<hash>.md           # structured (subject, predicate, object)
├── notes/<key>.md            # key-value notes
├── secrets/<key>.enc.md      # AES-256-GCM ciphertext only, never plaintext
└── sessions/<project>/<id>.md
```

A `chunks/*.md` file:

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
CLIProxyAPI troubleshooting: '429 model_cooldown' on a specific Claude
model while other models / same account work means stale in-memory
cooldown cache. Fix: `systemctl --user restart cliproxyapi`.
```

Edit the body, run `pjournal import`, and the corrected memory becomes
searchable in palimpsest — with a provenance marker
(`<!-- palimpsest-journal: edited-from=<old_id> -->`) so reviewers can
trace the lineage.

## Security model

- **Secrets are never exported in plaintext.** Only the AES-256-GCM
  ciphertext blob and metadata leave the local store. The decryption
  key (`~/.palimpsest/master.key`) lives outside the repo and is in the
  default `.gitignore`.
- **Sensitive chunks are skipped by default.** Pass
  `--include-sensitive` to opt in (e.g. for a private repo you'll push
  to a personal remote).
- **Imports go through the HTTP server**, not the sqlite file. We never
  poke the DB directly, so the BM25 / vector / fact indices stay
  consistent.
- **Git history is tamper-evident.** A signed `git commit -S` flow
  works out of the box because we just shell out to your `git`.

## Commands

```text
pjournal init   <dir>                    # initialize a journal repo
pjournal export                          # DB -> markdown (idempotent)
pjournal sync   [--push] [--message ...] # export then git add+commit (+push)
pjournal import                          # apply your *.md edits back to palimpsest
pjournal log    [-n N]                   # show commit history
pjournal status                          # show pending changes

Common flags:
  --dir <path>          journal dir (default: ~/.palimpsest/journal)
  --db  <path>          palimpsest sqlite (default: ~/.palimpsest/memory.db)
  --url <url>           palimpsest server (default: http://127.0.0.1:3111)
  --project <a,b,c>     limit to specific projects (export/sync)
  --since <iso>         only export chunks newer than this timestamp
  --include-sensitive   include chunks flagged sensitive=true
  --prune               delete repo files that no longer exist in DB
  --remote <name>       git remote (default: origin)
  --branch <name>       git branch (default: main)
```

## Workflows

### Solo dev: sync to a private GitHub repo

```bash
pjournal init ~/memory-journal
cd ~/memory-journal && gh repo create memory --private --source=. --push
crontab -l | { cat; echo "*/15 * * * * pjournal sync --push"; } | crontab -
```

You now have an off-machine, time-versioned backup of every AI memory.

### Team: PR review for agent memory

```bash
# the agent runs in CI / a shared box and pushes to a `learning` branch
pjournal sync --push --branch learning

# a teammate reviews the PR: did the agent learn something we don't want
# the team to act on? Did it mis-attribute a fact?
gh pr view ...
gh pr review --approve  # merges into main, becomes the source of truth
```

### Compliance: SOC2-ready memory audit trail

Every change is a signed git commit. Every revert is a signed git
commit. The `secrets/` tree carries AES-256-GCM ciphertext — even if the
repo leaks, the master.key never does.

## How it relates to other memory systems

This is intentionally **not** a memory system. It is a thin adapter on
top of one. If you already use:

- **palimpsest** — first-class support today.
- **mem0 / agentmemory / Letta / Zep** — pluggable in principle; only
  palimpsest is implemented in 0.x. PRs welcome.

The whole codebase is ~870 lines (including the 94-line smoke harness).
The value is the workflow, not the algorithm.

### Capability comparison

|                                                | palimpsest-journal | mem0 export | agentmemory dump | Letta replay | Zep “documents” |
|------------------------------------------------|:------------------:|:-----------:|:----------------:|:------------:|:---------------:|
| Memory lives as plain `.md` files              |         ✅         |     ❌      |        ❌         |      ❌       |        ❌        |
| `git diff` between two memory snapshots        |         ✅         |     ❌      |        ❌         |      ❌       |        ❌        |
| `git revert` a bad memory write                |         ✅         |     ❌      |        ❌         |      ❌       |        ❌        |
| `git blame` on a fact                          |         ✅         |     ❌      |        ❌         |      ❌       |        ❌        |
| Code-review (PR) flow for agent learning       |         ✅         |     ❌      |        ❌         |      ❌       |        ❌        |
| Manual edit re-applied with provenance marker  |         ✅         |     ❌      |        ❌         |      ❌       |        ❌        |
| AES-GCM secrets exported as ciphertext only    |         ✅         |     n/a    |        n/a       |      n/a     |       n/a       |
| Zero native deps                               |         ✅         |     ❌      |        ❌         |      ❌       |        ❌        |
| Works under Bun, Node 20+, no Docker           |         ✅         |     ❌      |        ❌         |      ❌       |        ❌        |

## Contributing & support

Issues and PRs welcome at [github.com/Blue-B/palimpsest-journal](https://github.com/Blue-B/palimpsest-journal/issues).

If you adopt this in a team and it removes a real audit gap, consider [sponsoring](https://github.com/sponsors/Blue-B) to fund maintenance and a hosted review UI (planned for 0.3.x).

## License

MIT © Blue-B.
