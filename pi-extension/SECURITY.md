# Security model & audit checklist

This document covers `pi-palimpsest` (HTTP bridge), `palimpsest` (server),
and `palimpsest-journal` (git mirror) as a single system.

## Threat model

| Asset                       | Confidentiality | Integrity | Availability |
|-----------------------------|:---------------:|:---------:|:------------:|
| Plain memory chunks (`chunks`) | low–medium      | high      | medium       |
| Session summaries           | low–medium      | high      | low          |
| Facts triples               | low             | high      | low          |
| Notes (KV)                  | low–medium      | high      | low          |
| **Secrets** (PATs, keys)    | **CRITICAL**    | high      | medium       |
| `master.key`                | **CRITICAL**    | CRITICAL  | high         |

## Trust boundaries

```
   user │ pi / Claude Desktop / Cursor / Cline / curl
        │       (any MCP or HTTP client)
        ▼
 ──── 127.0.0.1:3111 (loopback only by default) ──────
        ▼
   palimpsest server
        │ AES-256-GCM         (secrets only)
        ▼
   memory.db   |   master.key (chmod 600)
```

- Network: the server binds `127.0.0.1` by default. Do **not** bind a
  public interface unless you put a reverse proxy with auth in front.
- Disk: `master.key` is the single point of failure for secrets. Lose
  it → all secrets are irreversibly unreadable. Leak it + leak the DB →
  all secrets are decryptable.

## Pre-publish audit checklist

Run through this before pushing a release.

### 1. Process / permissions
- [ ] `palimpsest` runs as a non-root user (use the systemd `[Service]`
      template under `contrib/palimpsest.service` which targets `--user`).
- [ ] `~/.palimpsest/master.key` is mode `600` (`ls -la` shows `-rw-------`).
- [ ] `~/.palimpsest/memory.db` is mode `600` or `640`.
- [ ] No process other than `palimpsest` has FD-level access to either file
      (`lsof ~/.palimpsest/master.key`).

### 2. Network surface
- [ ] `ss -tln | grep 3111` reports `127.0.0.1:3111`, not `0.0.0.0`.
- [ ] If you opened `--host 0.0.0.0` for LAN access, there is at least
      one of: SSH tunnel, mTLS, basic auth, or firewall rule.
- [ ] CORS is not enabled for unknown origins (palimpsest 0.1.x has no
      CORS by default — verify if a future version adds it).

### 3. Secrets
- [ ] No `secret_get` results are ever logged to disk or to stderr.
- [ ] Logging level for `palimpsest` is `info` or quieter in production
      (verify the systemd unit doesn't pass `RUST_LOG=trace`).
- [ ] `secret_list` is idempotent and never returns values — periodically
      diff its output against expected (`palimpsest-secret list`).
- [ ] Rotate secrets at least every 90 days:
      ```bash
      # rotate a GitHub PAT
      palimpsest-secret set github_pat_blue_b "$NEW_PAT"
      # old value is overwritten, audit log lives in palimpsest-journal git history
      ```

### 4. Backups
- [ ] Daily snapshot of `~/.palimpsest/memory.db` is automated and stored
      off-machine.
- [ ] **`master.key` is backed up ONCE to a separate, encrypted, offline
      medium** (1Password / Bitwarden secure note / paper in safe). Never
      commit it to git.
- [ ] Restore drill performed at least quarterly:
      ```bash
      systemctl --user stop palimpsest
      mv ~/.palimpsest ~/.palimpsest.bak
      mkdir ~/.palimpsest
      cp /backup/memory-2026-05-17.db ~/.palimpsest/memory.db
      cp /vault/master.key ~/.palimpsest/master.key
      chmod 600 ~/.palimpsest/master.key
      systemctl --user start palimpsest
      palimpsest-secret list   # values are decryptable
      ```

### 5. palimpsest-journal git repo
- [ ] `.gitignore` includes `master.key`, `memory.db*`, `text_index/`,
      `vectors/`, `models/`.
- [ ] Repo is a **private** GitHub/GitLab repo (push only over SSH/HTTPS
      with token).
- [ ] `secrets/*.enc.md` files contain `$enc$…` ciphertext only — grep
      proves no plaintext keys ever leaked:
      ```bash
      cd ~/memory-journal
      git grep -E '(ghp_|sk-|AKIA|BEGIN PRIVATE KEY)' && echo FAIL || echo OK
      ```
- [ ] `--include-sensitive` is **off** by default. Re-running smoke test
      (`npm run smoke` in palimpsest-journal) confirms.

### 6. Multi-client coexistence
- [ ] Only ONE long-running server writes to `memory.db` at a time. If
      multiple stdio MCP clients are spawning their own `palimpsest --mcp`
      AND you have a systemd HTTP server, SQLite's WAL handles read
      contention but writes can race. Pick: HTTP-only (clients use
      pi-palimpsest / curl) OR stdio-only (no systemd unit).

### 7. Audit trail
- [ ] `palimpsest-journal sync --push` runs at least daily (cron or
      systemd timer).
- [ ] Commits are signed (`git config commit.gpgsign true`).
- [ ] PR review is required on the `main` branch of the journal repo if
      a team has write access.

## Reporting vulnerabilities

Open a private issue at the upstream palimpsest repo, or email the
maintainer of this bridge. Do not file public GitHub issues for
disclosure of secret-handling bugs.

## Known limitations

- palimpsest 0.1.x has no key rotation API for `master.key` itself.
  Re-keying requires: dump all secrets via `secret_get` → delete DB →
  start with new `master.key` → re-`secret_set` each entry. Plan for
  this if your threat model demands periodic re-keying.
- The HNSW vector index is rebuilt from `chunks.embedding` on startup;
  corrupted vector files won't lose data but can cause `memory_search`
  recall to drop briefly until rebuild completes.
