# Security Policy

## Reporting a vulnerability

Please report security problems privately, not in the public issue tracker.

Use GitHub's private vulnerability reporting: open the
[Security tab](https://github.com/Blue-B/memnest/security/advisories) of this
repository and choose "Report a vulnerability". That opens a draft advisory only
you and the maintainer can read.

This is a personal project with a single maintainer, so there is no on-call
rotation and no guaranteed response window. Expect a first reply within about a
week. If a report goes unanswered for longer than that, a reminder comment on the
draft advisory is welcome.

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
  `<data-dir>/master.key`, which memnest creates on startup with mode 0600, or
  from `MEMNEST_MASTER_KEY` when that is set.
- **Vault encryption falls back to plaintext when no key is available.** If the
  cipher was never initialised, values are stored as they were given. Confirm
  `<data-dir>/master.key` exists before relying on the vault.
- **A data directory has one writer.** Each MCP client started over stdio spawns
  its own process, so pointing two of them at one directory means two writers on
  the same files.

## Out of scope

These are known properties rather than vulnerabilities, so a report about them
will be closed with a pointer back to this section:

- Reading memories with local filesystem access to the data directory
- Reaching an unauthenticated instance that the operator bound to a non loopback
  address after setting `MEMNEST_TOKEN`, from a network the operator exposed it to
- Redaction failing to catch a credential format it does not recognise
- Corruption caused by running two writers against one data directory

A report is in scope when it shows memnest doing something the description above
says it does not: authentication being bypassed, a non loopback bind being
accepted without a token, vault values being recoverable without the key, or
input from a memory or transcript leading to code execution.

## Third party code

Dependency attributions are in
[`core/THIRD_PARTY_NOTICES.md`](core/THIRD_PARTY_NOTICES.md). Vulnerabilities in
a dependency are best reported upstream first; tell us as well if memnest is
affected in a way the upstream report does not cover.
