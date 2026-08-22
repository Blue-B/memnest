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
| 0.2.x | Yes |
| Older | No |

There is one active line. Fixes land on the current version rather than being
backported.

## Threat model

memnest is a local service. It assumes the machine it runs on is trusted and
that the service is not exposed to the internet. Anyone who can reach the port
or read the data directory can read your memories.

The HTTP server binds to `127.0.0.1`. A bind to any other address is refused
unless `MEMNEST_TOKEN` is set, in which case requests must carry
`Authorization: Bearer <token>`. That token is the only authentication memnest
has. There is no TLS, no user accounts, and no per memory access control. If you
need remote access, put a reviewed reverse proxy with TLS in front of it rather
than exposing the port.

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
- **Hard deletion still leaves a plaintext copy by default.** Deleting a memory
  moves it to `_trash`; trash older than 30 days is hard-deleted, and the full
  record is appended to `<data-dir>/archive/YYYY-MM.jsonl` in plaintext first.
  Set `MEMNEST_ARCHIVE=0` to stop writing those files, and delete the existing
  `archive/` directory yourself if a memory must really be gone. Vault values are
  not archived.
- **A data directory has one writer.** The first service or stdio MCP process
  holds an operating-system file lock. A second writer fails at startup instead
  of racing SQLite or the derived indexes.
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
