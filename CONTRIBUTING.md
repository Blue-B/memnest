# Contributing to memnest

Thanks for taking the time to look at this. memnest is a small project, so the
process is short.

## Before you start

By participating you agree to the [Code of Conduct](CODE_OF_CONDUCT.md).

For anything larger than a bug fix, open an issue first and describe the problem
you hit. That avoids the case where a change is finished before we find out it
does not fit the design. Small fixes can go straight to a pull request.

Nothing here is published to npm or crates.io yet, so every install is a source
checkout. Expect the layout and the interfaces to still move.

## Repository layout

Only `core/` is required to run memnest. The rest are optional pieces around it.

| Directory | Language | What it is |
| --- | --- | --- |
| `core/` | Rust | The engine: HTTP API, MCP server, indexes |
| `pi-extension/` | TypeScript | pi integration |
| `adapters/generic-http/` | JavaScript | Reference JSONL adapter |
| `docs/` | Markdown | Operations guide and screenshots |

## Development checks

Run the checks for the component you touched. Each block is a subshell, so it
starts from the repository root rather than from wherever the previous line
left you:

```bash
(cd core                   && cargo test --locked -- --test-threads=1)
(cd pi-extension           && npm install && npm run build && npm run smoke)
(cd adapters/generic-http  && node test.mjs)
```

Core tests run serially on purpose. Several of them set process-global state,
the `MEMNEST_*` environment variables and the shared vault cipher, so the
default parallel runner makes them race each other. CI uses the same
`--test-threads=1`.

`core` also needs `cargo build --release --locked` to pass, because the CI job
builds a release binary after the tests, and `python3 scripts/check-licenses.py`
plus `python3 scripts/generate-third-party-notices.py --check` after any
dependency change.

`pi-extension` has `npm run e2e` (`node test/e2e-mcp.mjs`) on top of the smoke
run. It spawns a real memnest binary over stdio, so point `MEMNEST_BIN` at one:
`MEMNEST_BIN=../core/target/release/memnest npm run e2e`. `dist/index.mjs` is
committed and CI rejects a bundle that does not match a fresh `npm run build`,
so commit the rebuilt bundle with any `src/` change.

### The smoke tests talk to a running memnest

This one costs people real data, so it is worth stating plainly.

A smoke test writes into whichever store answers the URL it is given, so it
must never be pointed at your day to day memnest. Give it a scratch instance:

```bash
memnest --data-dir /tmp/memnest-dev --port 3150 &
MEMNEST_URL=http://127.0.0.1:3150 MEMNEST_DB=/tmp/memnest-dev/memory.db npm run smoke
```

`pi-extension`'s smoke test also reaches for `http://127.0.0.1:3111`, but it
skips the live section when nothing answers, and its calls are read paths only.
It does not create memories.

## Continuous integration

Workflows live in `.github/workflows/` and are filtered by path, so a change
under `core/` does not run the pi-extension job:

- `core-ci.yml` runs the serial test suite and a release build on Linux and
  Windows, executes the installer scripts, checks license metadata, and fails
  when `THIRD_PARTY_NOTICES.md` is out of date
- `pi-extension-ci.yml` installs with `npm ci`, builds the bundle, checks the
  committed `dist/index.mjs` against that build, loads it, and runs the MCP
  end-to-end test against a freshly built core binary
- `adapters-ci.yml` runs the generic HTTP contract test
- `core-release.yml` refuses a tag that does not match the version in
  `core/Cargo.toml`, tests, builds, then unpacks the archive and runs the
  packaged binary before anything is published

## Commit messages

The history uses Conventional Commits, with a scope naming the package when the
change belongs to one:

```text
feat(core): serve MCP over streamable HTTP on the API port
fix(pi-extension): make autocontext triggers multilingual
docs: rewrite the README around what a stranger needs
chore: clean up the repository for a public release
```

Scopes in use are `core`, `pi-extension`, and `adapters`.
`docs` and `chore` changes that span the repository are written without a scope.
Bodies explain why the change was needed, wrapped at roughly 80 columns.

## Pull requests

Say what you changed, why, and which checks you actually ran. If a check did not
run, write that down instead of leaving it implied. A reviewer would rather read
"did not run the pi-extension smoke test, no memnest server available" than guess.

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
