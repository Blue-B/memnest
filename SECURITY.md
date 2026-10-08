# Security Policy

## Reporting a vulnerability

Please report security problems privately, not in the public issue tracker.

First choice: GitHub's private vulnerability reporting. Open the
[Security tab](https://github.com/Blue-B/memnest/security/advisories) of this
repository and choose "Report a vulnerability". That opens a draft advisory only
you and the maintainer can read.

Fallback, when that button is not there or a draft advisory goes unanswered:
email the maintainer at `source_vs@naver.com`. Say that the report is a memnest
security issue in the subject line. Do not include working exploit output, key
material, or the contents of your data directory in the first mail; describe the
problem and wait for a reply before sending anything sensitive.

This is a personal project with a single maintainer, so there is no on-call
rotation and no guaranteed response window. Expect a first reply within about a
week. If a report goes unanswered for longer than that, a reminder comment on the
draft advisory, or the email fallback above, is welcome.

When you report, the useful details are the version (`memnest --version`), how
memnest was reached (loopback HTTP, MCP over stdio, MCP over HTTP, or one of the
subcommands), and the smallest reproduction you have.

## Supported versions

| Version | Supported |
| --- | --- |
| 0.4.x | Yes |
| Older | No |

There is one active line. Fixes land on the current version rather than being
backported.

## Threat model

memnest trusts the machine owner and must not be exposed directly to the internet.
Without an access policy, it retains the local-owner contract: a reachable client
with the global token (or any loopback client when no token is set) can read memory.
Filesystem owners remain able to read the unencrypted database.

The HTTP server defaults to `127.0.0.1`. Non-loopback binding requires the legacy
`MEMNEST_TOKEN` or a valid `MEMNEST_ACCESS_POLICY`. Policy mode authenticates named
principals from bearer-token digests and enforces exact project read/write grants
on HTTP and MCP. The secret vault, global statistics, and audit view are administrator-only.
Hidden legacy rows without a trustworthy scope are administrator-only.
See [team access](docs/team-access.md) for startup, revocation, audit and retention limits.
There is no built-in TLS, password/SSO service, or SaaS tenant administration.
Remote use requires a reviewed TLS/rate-limiting reverse proxy. Stdio and local
library callers retain filesystem-owner privileges.

Some consequences worth stating directly:

- **Memory text is not encrypted at rest.** Anyone with read access to the data
  directory can read every stored memory, note, fact, and session.
- **Incoming text is scanned for credential shaped strings and redacted.** Treat
  that as a safety net that catches common patterns, not as a guarantee, and not
  as permission to send secrets through it.
- **The secret vault is the only path meant for sensitive values.** It encrypts
  them with AES-256-GCM using a key derived with Argon2id from
  `<data-dir>/master.key`, which is created with mode 0600 before key bytes are
  written, or from `MEMNEST_MASTER_KEY`. New ciphertext is bound to its secret
  key or server name. Model-facing vault tools are hidden unless
  `MEMNEST_EXPOSE_SECRET_TOOLS=1` is set.
- **The vault fails closed. There is no plaintext fallback anywhere.** An empty
  or unreadable `master.key` aborts startup. A key that cannot decrypt the vault
  values already in the store aborts startup with `vault key validation failed`,
  and that includes the case where `master.key` went missing and a fresh random
  key was generated in its place. A stored value that is not valid ciphertext is
  an error, never returned as if it were the plaintext. Losing the key means
  losing the vault contents, so back the key up separately from the data
  directory.
- **Deletion is not erasure of every copy.** Soft deletion moves memory to trash,
  with a default 30-day recovery window. `POST /purge` logically deletes already-trashed
  records and their serving indexes, and identified transcript tombstones block same-event replay.
  Plaintext archival is off by default; `MEMNEST_ARCHIVE=1` opts in. Old archives,
  backups, original host transcripts, legacy derived copies, already-exported text,
  and physical SSD/WAL bytes are outside this deletion guarantee. Vault values are not archived.
- **A data directory has one writer.** The first service or stdio MCP process
  holds an operating-system file lock. A second writer fails at startup instead
  of racing SQLite or the derived indexes.
- **Model routing is not endpoint attestation.** The pi provider allowlist blocks new
  reads/injections for unapproved models but cannot verify a proxy's destination,
  govern other clients, or erase previous context. Use a fresh session and approved gateway.
  The optional evidence selector accepts only a loopback Ollama URL and labels its output
  as model-selected, not truth-verified. Default search remains generation-free.
- **Retrieved memory is untrusted input.** Prompt-time context labels transcript
  rows as conversation evidence, tells the agent not to follow stored commands,
  and escapes markup. This reduces prompt-injection risk but does not make a
  malicious memory trustworthy.

## Out of scope

These are known properties rather than vulnerabilities, so a report about them
will be closed with a pointer back to this section:

- Reading memories with local filesystem access to the data directory
- Reaching an unauthenticated instance that the operator bound to a non loopback
  address after setting `MEMNEST_TOKEN`, from a network the operator exposed it to
- Redaction failing to catch a credential format it does not recognise
- Reading a hard-deleted memory out of `<data-dir>/archive/` when archiving was
  left enabled

## Workspace boundaries

An inferred workspace is keyed by a hash of the normalized absolute working
directory, so same-basename directories do not share new writes. The full path
is not used as the public collection name. Automatic recall includes the current
workspace and `playbook`, and fails closed when neither `cwd` nor an explicit
project is available.

Legacy basename collections are included only while one registered workspace
owns that basename. Once the basename is ambiguous, memnest disables the alias
for every claimant instead of assigning old rows by guesswork. Operators can
still address a legacy collection with an explicit `project`.

A report is in scope when it shows memnest doing something the description above
says it does not: authentication being bypassed, a non loopback bind being
accepted without a token, vault values being recoverable without the key, or
input from a memory or transcript leading to code execution.

## Code of conduct

The project also has a [Code of Conduct](CODE_OF_CONDUCT.md). Conduct reports go
to the same maintainer address, not to the security advisory queue.

## Third party code

Dependency attributions are in
[`core/THIRD_PARTY_NOTICES.md`](core/THIRD_PARTY_NOTICES.md). Vulnerabilities in
a dependency are best reported upstream first; tell us as well if memnest is
affected in a way the upstream report does not cover.
