# Team access, audit, and retention

These are optional single-host team controls, not an SSO service, a multi-region platform, or a compliance certification. A trusted machine owner can read the data files. Use a TLS reverse proxy with rate limiting for remote access; do not expose the service port to the internet.

## Named principals

Set `MEMNEST_ACCESS_POLICY` to an operator-controlled JSON file. It declares named principals, SHA-256 digests of high-entropy bearer tokens, exact read/write project grants, and optional project retention days. The server validates the complete policy before opening its listener. At least one administrator is required. Write grants must also be read grants; no wildcard grants are interpreted.

Create a private starter policy without printing credentials:

```sh
python3 scripts/create-access-policy.py --out /private/new-policy \
  --read company-project --write company-project
```

The new directory must not already exist. `policy.json` contains digests, not bearer values. `admin.env` and `client.env` contain generated credentials and are mode 0600; keep them private, transfer them through your approved credential channel, and never commit them. They are not automatically installed or sourced. Review the policy, load the corresponding environment into the service/client, and restart the service to apply a change or revoke a token.

`MEMNEST_TOKEN` still supplies a client's bearer header. When policy mode is enabled, only policy-listed tokens authenticate; the legacy global token is not a bypass. Without a policy, the original trusted-local-owner mode remains available.

On Unix, policy mode requires the data directory itself to be owner-only (mode 0700). Once the store has opened in policy mode, a persisted marker prevents this version from reopening it merely by removing `MEMNEST_ACCESS_POLICY`. Do not open a protected store with an older binary that does not understand the marker. Rollback requires restoring the reviewed earlier binary/configuration and matching backup, not ignoring this protection.

## Scope enforcement

Grants apply to storage, search, raw ID lookup, updates, project moves, deletion, restoration, and permanent logical deletion. Both HTTP and MCP use the same checks. `project=all` searches only the principal's read grants. Workspace-derived searches cannot import an ungranted `playbook` or legacy alias into the result.

IDs do not grant access. Reading an inaccessible ID returns a generic not-found response. A multi-ID permission failure is checked before any member of the batch is changed. This does not claim rollback of every possible later storage failure.

New hidden records retain a server-written authorization scope. Restoring a deleted superseded record keeps it superseded rather than making stale information active. Legacy hidden rows without a trustworthy scope are administrator-only; do not infer their ownership from a basename or caller-supplied metadata.

The global secret vault, global statistics, and audit view are administrator-only. Local stdio/library callers retain the trusted-filesystem-owner contract, not network-principal isolation. Run untrusted agents outside the service owner's OS account; an API token cannot restrict someone who can already read the database.

## Audit

`GET /audit` is an administrator view of the newest 200 events. The full ledger is the `access_events` table in SQLite, included in normal database backups. Records attribute requests and memory access/mutations to authenticated principal IDs. Queries, memory bodies, response bodies, bearer values, and secret names are not retained in this ledger.

A request's attempt is recorded before its handler runs. Audit failures block access. A completion-audit failure can happen after a mutation committed, so a failure response does not prove the mutation was undone; inspect the initial event and operation state before retrying. This is an operator-controlled audit trail, not a tamper-proof external ledger.

## Retention and deletion

Add a `retention_days` mapping to the policy, for example `"retention_days": {"company-project": 90}`. Values are 1 to 3650 days. This maximum age uses the record's stored creation timestamp, not an inferred source event date, and overrides pinned/importance/type exemptions for that project. Unconfigured projects keep the normal preservation-first policy. Existing hidden records without a known scope need administrator review rather than guessed ownership.

Expired records first move to trash. Trash defaults to 30 days; `MEMNEST_TRASH_RETENTION_DAYS` accepts 0 to 3650. A value of zero makes the next lifecycle pass eligible to remove trash. Administrators can run the existing lifecycle immediately with `POST /retention`.

Archival plaintext copies are now off by default. `MEMNEST_ARCHIVE=1` explicitly opts in; existing archives are not removed by changing this setting.

`POST /purge` accepts `{"ids":["<returned memory ID>"]}` and permanently removes already-trashed records from the serving database and derived indexes. Active records return a conflict and must be soft-deleted first. Current project write permission is required. Identified transcript tombstones prevent replay of the same event from recreating it.

Purge is not physical secure erasure. It does not delete previous archive files, backups, original host transcripts, legacy derived copies, or text already sent to another model. SSD/WAL behavior and operator-held copies need an organization-level deletion plan. The response states these limits rather than claiming every copy vanished.

## Model routing

In pi, `MEMNEST_ALLOWED_MODEL_PROVIDERS` is an optional comma-separated allowlist. For example, a trusted deployment can allow only its registered local provider. The bridge blocks new searches, memory reads, credential reads, and automatic recall for unapproved or unknown model contexts before sending a retrieval request.

This checks the runtime's declared provider ID. It cannot attest that a proxy stays local, restrict other API clients, stop an authorized user from copying data, or erase older conversation history. Start a fresh session when changing data/model boundaries and enforce actual egress at your trusted gateway. Local storage by itself does not keep retrieved text away from a cloud model.

The optional core `evidence_only` search uses only a credential-free loopback Ollama URL configured through `MEMNEST_EVIDENCE_URL`; it does not permit a cloud inference URL. Its model-selected excerpts are not truth verification.

## Executable checks

```sh
python3 scripts/test-access-policy.py
(cd pi-extension && npm run build && node test/model-policy.mjs)
```

The isolated tests exercise real HTTP/MCP reads and mutations, forged scope metadata, cross-project IDs and batches, history restoration, secret/statistics/audit restrictions, content-free attribution, protected-store restart, pinned record expiry, permanent deletion, and transcript replay. They do not expose or erase the production store.
