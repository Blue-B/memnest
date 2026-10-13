# A saved-note lookup and three recall questions

<!-- markdownlint-disable MD013 -->

On 2026-10-06, three Korean questions were fixed before searching an existing Memnest store. They asked why a task manager was replaced, why a browser tool was replaced, and how an earlier service-start problem was fixed. No example memory was inserted for this check.

The current pi session called the real memory tools against the installed core, which reported version 0.3.0. Each initial search used `project=all` and at most five results. This was an exploratory check in an already-informed assistant session, not a blind test or a fresh-agent benchmark.

## What happened

| Question topic | Initial search | Additional lookup | Outcome |
| --- | --- | --- | --- |
| Task-manager replacement | A later investigation note, not the original decision conversation | Read that note, then the existing manual note it referenced | The saved note attributed the replacement to a stale `ctx.cwd` access error after a session switch/fork. |
| Browser-tool replacement | Records about problems with the old tool | Read the first note, then search using the new tool name and the session-conflict terms | Related problems were found, but the requested replacement decision was not established. |
| Service-start repair | Service-update notes, not the requested repair explanation | Read the first note, then search the project with service-error terms | The requested cause and repair were not established. |

For the task-manager question, the route used one search and two source reads. The final note had been written in September after an earlier investigation of the original conversation. It was not the original automatically captured conversation, and it was not returned directly among the first five results.

None of these initial searches supplied the requested original decision or repair explanation directly. One question could be answered indirectly from an existing manual note; two remained unresolved within the recorded follow-up attempts. These three deliberately selected questions do not measure overall accuracy. A miss here does not establish whether the relevant record was absent, poorly ranked, or outside the searched scope.

Ongoing capture also added this session's progress report to the store. It appeared in the later snapshot's task-manager search and was excluded as evidence; a report describing the trial is not an independent source for its answer.

## Released-core check

The installed core was not silently upgraded. A separate check used the previously downloaded public Linux x86_64 v0.3.1 archive, verified against its accompanying SHA-256 checksum.

Only active memory rows and workspace metadata were copied through a read-only SQLite transaction into a private disposable store, containing 28,755 active records. No vault, master key, or archive files were copied. The release core used a random authenticated loopback endpoint and the existing offline model cache.

The same three initial questions again returned the investigation note, old-browser problem notes, and service-update notes rather than the requested original explanations. The referenced task-manager manual note was read through the released core, and its text matched the earlier tool read. This HTTP recheck did not run a new answering AI, repeat the two refined queries, or perform an identical-corpus old/new benchmark. The disposable server was stopped; the production service and installed binary were unchanged.

## Image scope

[English image](recall-demo.png) and [Korean image](recall-demo.ko.png) show the task-manager path only.

The images are arranged summaries of actual requests, not screenshots or video recordings. The English question and reason are translations of Korean text; English retrieval was not tested. Personal package names, source file paths, and memory IDs are omitted. The remaining reason in the Korean image is an excerpt from the retrieved manual note.

This supports a narrow claim: an assistant followed a retrieved reference and read an existing saved explanation. It does not demonstrate immediate recall from captured conversation history, autonomous Claude Code/Codex handoff, faster answers, lower cost, or superiority over another memory product.

Original questions, request results, note reads, and the disposable-check runner are retained privately, not committed. This document cannot provide a public replay of personal records. Use the separate [independent-client demo](demo.md) for a reproducible transport example, noting that its saved decision is fictional.
