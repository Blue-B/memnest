# Contributing to memnest

Thanks for taking the time to look at this. memnest is a small project, so the
process is short.

## Before you start

For anything larger than a bug fix, open an issue first and describe the problem
you hit. That avoids the case where a change is finished before we find out it
does not fit the design. Small fixes can go straight to a pull request.

Nothing here is published to npm or crates.io yet, so every install is a source
checkout. Expect the layout and the interfaces to still move.

## Repository layout

Only `core/` is required to run memnest. The rest are optional pieces around it.

| Directory | Language | What it is |
| --- | --- | --- |
| `core/` | Rust | The engine: HTTP API, MCP server, indexes, dashboard |
| `pi-extension/` | TypeScript | pi integration |
| `journal/` | JavaScript | Markdown and git audit mirror |
| `learn/` | TypeScript | Experimental pi learning layer |
| `adapters/generic-http/` | JavaScript | Reference JSONL adapter |
| `docs/` | Markdown | Operations guide and screenshots |

## Development checks

Run the checks for the component you touched. These were each executed against
this checkout, and the counts are what they printed.

```bash
cd core                   && cargo test --quiet        # 74 tests
cd pi-extension           && npm run build && npm run smoke
cd journal                && npm run smoke             # see the warning below
cd learn                  && npm test                  # needs bun on PATH
cd adapters/generic-http  && node test.mjs
```

`core` also needs `cargo build --release` to pass, because the CI job builds a
release binary after the tests. `docs/operations.md` suggests
`cargo test -- --test-threads=1` on the grounds that environment variable tests
interfere with each other. The parallel run above passed, and CI runs
`cargo test --quiet`, so use the serial form only if you hit a flaky test.

`learn` has extra scripts: `npm run build` and `npm run typecheck` (`tsc --noEmit`).
`pi-extension` has `npm run e2e` (`node test/e2e-mcp.mjs`) on top of the smoke run.

### The smoke tests talk to a running memnest

This one costs people real data, so it is worth stating plainly.

`journal`'s smoke test defaults to `MEMNEST_URL=http://127.0.0.1:3111` and
`MEMNEST_DB=~/.memnest/memory.db`. It creates new collections and writes chunks
into whichever store answers on that port. If that is your day to day memnest,
the test pollutes it. Point it at a throwaway instance instead:

```bash
memnest --data-dir /tmp/memnest-dev --port 3150 &
MEMNEST_URL=http://127.0.0.1:3150 MEMNEST_DB=/tmp/memnest-dev/memory.db npm run smoke
```

`pi-extension`'s smoke test also reaches for `http://127.0.0.1:3111`, but it
skips the live section when nothing answers, and its calls are read paths plus
one recall feedback marker. It does not create memories.

### Known failing test

`learn`'s `extension registers the expected hooks and tools` currently fails:
the test expects a `session_before_compact` hook that the extension does not
register. It fails on a clean checkout, so it is not something you introduced.
The other 40 tests in `learn` pass.

## Continuous integration

Workflows live in `.github/workflows/` and are filtered by path, so a change
under `core/` does not run the pi-extension job:

- `core-ci.yml` runs `cargo test --quiet` and `cargo build --release` on Linux
  and Windows, validates the installer scripts, and checks license metadata
- `pi-extension-ci.yml` installs with `npm ci`, builds the bundle, and loads it
- `journal-ci.yml` builds the core binary, starts and seeds a server, then runs
  the smoke test against that server rather than your own
- `core-release.yml` builds the release artifacts when a tag is pushed

## Commit messages

The history uses Conventional Commits, with a scope naming the package when the
change belongs to one:

```text
feat(core): serve MCP over streamable HTTP on the API port
fix(pi-extension): make autocontext triggers multilingual
docs: rewrite the README around what a stranger needs
chore: clean up the repository for a public release
```

Scopes in use are `core`, `pi-extension`, `journal`, `learn`, and `adapters`.
`docs` and `chore` changes that span the repository are written without a scope.
Bodies explain why the change was needed, wrapped at roughly 80 columns.

## Pull requests

Say what you changed, why, and which checks you actually ran. If a check did not
run, write that down instead of leaving it implied. A reviewer would rather read
"did not run the journal smoke test, no memnest server available" than guess.

Keep a pull request to one concern. Two unrelated fixes are easier to review as
two pull requests.

## Reporting bugs and asking for features

Use the issue templates. For a bug, the output of `memnest --version` and
`memnest status` plus the host you drive memnest from (pi, Claude Code, another
MCP client, or plain HTTP) usually explains half the problem on its own.

Security problems do not belong in the issue tracker. See [SECURITY.md](SECURITY.md).

## License

Contributions are made under the MIT license that covers the repository. By
opening a pull request you agree your work ships under those terms.
