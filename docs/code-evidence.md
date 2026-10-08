# Selected-file evidence checks

Core 0.4.0 saves a caller-reported approach together with fingerprints of explicitly selected working files. A later, explicit check compares those files with the saved fingerprints. It does not warn on every edit, change retrieval ranking, validate a solution, or execute a remembered command.

Use matching core/CLI 0.4.0. The CLI refuses to save against an older core before making a write. Basic reads remain compatible; the new options require the updated core.

## Save and check

After building the core (`cd core && cargo build --offline`), use its binary against a development service running the same source. `--url` must be a loopback HTTP service; it defaults to `MEMNEST_URL`, then `http://127.0.0.1:3111`. If authentication is enabled, supply `MEMNEST_TOKEN` in the environment, not an argument.

From the workspace whose files you want to associate with a report:

```sh
/path/to/new/memnest evidence --cwd "$PWD" --url "$MEMNEST_URL" remember \
  --text "Build repair: the compiler change failed in this environment" \
  --file Cargo.toml \
  --file src/main.rs \
  --status failed \
  --applicability "Describe the OS, dependency versions, and exact attempted change" \
  --evidence "Describe the observed error or cite the existing test report"
```

Paths are relative to `--cwd`. Choose only files that bear on the approach. The command records their current bytes, not their state when some historical task ran. It cannot reconstruct an old baseline from an old conversation. Do not attach today's files as if they independently proved a past result.

Status is required: `proposed`, `failed`, or `reported_success`. Applicability and result text remain caller reports. See [approach outcomes](approach-outcomes.md).

The reply is the normal save result, including a memory ID. Pass that returned ID to check later:

```sh
/path/to/new/memnest evidence --cwd "$PWD" --url "$MEMNEST_URL" check "$MEMORY_ID"
```

Check prints JSON and does not modify the memory. It retrieves only the baseline and bounded metadata, not the full conversation. A successful check exits zero even if files changed or cannot be compared; inspect the JSON status. Transport, authentication, invalid metadata, and command errors exit nonzero.

| Status | Meaning |
| --- | --- |
| `unchanged` | All selected file bytes match. This does not verify the approach or its applicability. |
| `changed` | At least one selected file differs or is missing. This does not invalidate the approach. |
| `unavailable` | At least one selected file could not be safely read, for example a symlink or oversized file. Other file results may still be present. |
| `different_workspace` | The canonical local workspace path differs. No saved file path is read. |
| `not_recorded` | This memory has no baseline. It is not implicitly current. |

Each selected file has its own `unchanged`, `changed`, `missing`, or `unavailable` result. Unrelated files are never compared. A changed Git HEAD alone does not change the file-comparison status.

## What is stored

Optional `metadata.code_evidence` contains:

- The canonical-path-derived workspace ID, using the existing workspace identity function.
- The local capture timestamp.
- Git HEAD when available, as context only.
- Up to 16 relative file paths and their SHA-256 digests.

The fingerprints describe working files, including uncommitted changes. HEAD does not imply that those files match a commit or that a test ran there. File contents are not stored in this field. Paths and hashes can still disclose information; do not treat hashing as secret storage.

Each path is at most 256 UTF-8 bytes and each selected file is at most 1 MiB. Absolute paths, traversal, Git internals, credential-shaped path strings, symlinks, and nonregular files are rejected. File reads happen only in the local CLI. The HTTP/MCP server validates and stores the metadata without reading any referenced file.

This remains a trusted-local-workstation operation, not a filesystem sandbox against another process racing to replace files. It is not enterprise access control. A second clone or machine has a different workspace path and is deliberately not compared automatically.

## Existing tools and replacement

Saved records are ordinary searchable memories. `GET /chunk/{id}` and MCP `memory_get` return `code_evidence` with the assertion `caller_recorded_fingerprints_not_solution_verification`. Direct MCP `memory_remember` accepts the same bounded object; those caller-supplied digests are not independently verified by the server. In pi 0.4.0, `memory_remember(code_files=["relative/file"])` explicitly captures a local baseline and `memory_get(check_code=true)` explicitly compares it. Neither is automatic. `memnest evidence capture` is network-free; `memnest evidence compare` accepts a bounded baseline on stdin, so a remote-store baseline can be checked locally without asking the server to read files.

To replace the report or its baseline, save a new report with `--supersedes "$OLD_MEMORY_ID"`. Existing same-project checks apply. The old record remains available by ID but is hidden from ordinary search. In-place document changes and baseline patches are rejected; ordinary importance updates preserve the baseline.

No existing records are relabeled or retroactively given baselines. Default search, auto-recall, and transcript capture are unchanged. The executable validation used Linux x86_64; native macOS and Windows path handling have not been exercised.

## Executable regression and limits

```sh
cd /path/to/memnest
(cd core && cargo build --offline && cargo test --offline --lib code_evidence::tests)
python3 scripts/test-code-evidence.py
```

The script uses a disposable Git workspace and authenticated loopback service, with the existing `core/target/test-model-cache`. It does not read the production memory DB. It runs actual CLI saves and checks, HTTP and MCP reads/writes, selected-file edits, an unrelated Git commit, workspace mismatch, old rows without a baseline, supersession, rejected in-place edits, authentication, traversal rejection, missing files, and symlink refusal.

These checks establish the contract and the narrow file-comparison behavior. They do not establish fewer coding errors, lower token cost, better retrieval, improved installation, or superiority over Hindsight, memU, or other products. The separate [local provider comparison](local-provider-comparison.md) covers a small fixture and local model stack; it is not evidence that file checks improve coding outcomes.
