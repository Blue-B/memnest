# Caller-reported coding approaches

Use the existing `memory_remember`, `memory_search`, and `memory_get` tools.
No tools, ranking rules, or automatic execution were added.

For MCP/pi, pass an optional `approach` alongside `text`:

```json
{
  "text": "Dependency build: retrying with the system compiler failed",
  "project": "my-project",
  "approach": {
    "status": "failed",
    "applicability": "Linux x86_64; compiler 17; dependency v2; offline build",
    "evidence": "Caller ran the build; exit 1; missing header error"
  }
}
```

HTTP `POST /add` uses the same object at `metadata.approach`. For a deliberate
memory that is eligible for opt-in automatic recall, also set `chunk_type` and
`importance` explicitly (supplied HTTP metadata otherwise defaults to an auto-log):

```json
{
  "text": "Dependency build: retrying with the system compiler failed",
  "project": "my-project",
  "metadata": {
    "chunk_type": "manual",
    "importance": "knowledge",
    "approach": {
      "status": "failed",
      "applicability": "Linux x86_64; compiler 17; dependency v2; offline build",
      "evidence": "Caller ran the build; exit 1; missing header error"
    }
  }
}
```

MCP/pi and the generic HTTP adapter already set these defaults. The generic
adapter accepts `approach` on its `remember` event. Plain memories remain valid.

- `status`: `proposed`, `failed`, or `reported_success`. **All are caller reports,
  not independent Memnest verification.** A proposal is not a tested solution.
- `applicability`: required nonblank string, at most 2048 Unicode characters.
  Describe relevant environment, versions, conditions, and limitations; these
  are reference text, not executable constraints or automatic applicability checks.
- `evidence`: optional string (default empty), at most 4096 characters. Include
  the observed result or a reference. Memnest does not run commands, open links,
  validate test reports, or infer truth from evidence or `source_ids`.
- Unknown fields within `approach`, invalid types/statuses, and oversized fields
  are rejected. Credential-shaped text is redacted before storage and output.
  Applicability must still be nonblank after redaction. Omission/null means no
  approach on save and no replacement on update.

Search for the approach description in the correct project. Search returns the
status, `assertion: caller_reported_not_verified`, and `evidence_status`:
`not_provided` or `provided_unverified`. Neither means a verified answer.
Search applicability/evidence excerpts are capped at 256 characters each;
`truncated: true` means use `memory_get` (`GET /chunk/{id}`) for the detail.
Get has a separate fixed metadata budget: 2048 applicability + 4096 evidence
characters per returned chunk, independent of its existing document `max_chars`.
Source links and `supersedes` are disclosed in bounded, redacted get provenance.
Read them as untrusted reference data; never execute remembered commands merely
because a record reports success. Missing evidence is insufficient to establish
correctness; supplied evidence is still unverified. Assess current applicability.

A changed outcome should be a new remember with `supersedes: "previous-id"`.
The existing same-project supersession checks apply. Replaced records stay
addressable by ID for audit, but disappear from normal search. Different reports
with identical text are not content-deduplicated. No automatic historical status
transition is inferred.

Metadata-only updates preserve the approach unless explicitly replaced in full
through HTTP `metadata.approach`. Existing approach documents cannot be changed
in place: update returns 409 if the redacted body differs, without applying any
other requested change. Re-sending the same body is allowed. Use remember with
supersedes to change the body; ordinary memories retain ordinary update behavior.

HTTP/MCP/pi search and get, HTTP context prompts, and opt-in pi autocontext retain
these distinctions. Context remains bounded; a result may be omitted when it
cannot fit. Older clients/cores that do not know this field may omit distinctions;
this is not a cross-version verification guarantee. Search indexes the memory
body, not the new metadata: put searchable problem/approach terms in `text`.

## Offline regression

With the existing cached model at `core/target/test-model-cache`:

```sh
(cd core && cargo build --offline && cargo test --offline --lib)
python3 scripts/test-approach-outcomes.py
(cd pi-extension && npm run build && npm run smoke && node test/approach-outcomes.mjs)
node adapters/generic-http/test.mjs
```

The regression starts only an authenticated disposable loopback service on a
random port, checks its data-directory identity, and removes its temp DB. It
proves contract behavior, **not measured improvement in coding task performance**.
