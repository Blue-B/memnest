# Design decisions

<!-- markdownlint-disable MD013 -->

Why memnest is built the way it is. Each entry states the constraint that forced the choice, what was rejected, and what would justify revisiting it. Numbers here come from the code and from published benchmarks, not from preference.

## Embeddings run locally, through fastembed

The store holds decisions, corrections, and conversation history. Sending that to a hosted embedding API would mean every memory leaves the machine on its way in and every query leaves on its way out. A local memory service that phones home for each write is not a local memory service, so a hosted embedding API was ruled out before any model was compared.

That decision picks the runtime before it picks the model. The engine is Rust, and `fastembed` is the practical way to run ONNX embedding models in-process without a Python sidecar. It also fixes the menu: only models fastembed ships can be selected. An arbitrary HuggingFace checkpoint is not an option, and the error message in `core/src/embedding/mod.rs` says so explicitly.

The cost of this is a first-run download. Starting the service fetches nothing; the model arrives on the first write or first search, which makes that one operation noticeably slow. `--warmup-embedding` exists to pay that cost on purpose rather than during a user's first question.

## multilingual-e5-base is the default, and it is not the most accurate option

Among the multilingual models fastembed supports, e5-base is not the top scorer. Published Korean retrieval numbers put it behind both larger alternatives:

| Model | Korean retrieval | Dimensions | Available in fastembed |
| --- | --- | --- | --- |
| BAAI/bge-m3 | 0.713 avg NDCG (Korean MTEB) | 1024 | yes |
| multilingual-e5-large | 61.6 MRR@10 (Mr. TyDi ko) | 1024 | yes |
| multilingual-e5-base | 55.8 MRR@10 (Mr. TyDi ko) | 768 | yes, default |
| multilingual-e5-small | 54.3 MRR@10 (Mr. TyDi ko) | 384 | yes |

BGE-M3 is the stronger retriever and is frequently the first recommendation for Korean on-premise RAG. It is supported here and can be selected. It is not the default for two reasons specific to how this service is used.

The first is the latency budget. Prompt-time recall is not a search page a user chose to visit; it runs while someone waits to send a message. The pi extension gives that path 1500 ms end to end (`MEMNEST_AUTOCONTEXT_TIMEOUT_MS`), after which the prompt proceeds with no card at all. A larger model spends that budget on encoding a single short query, and the failure mode is not a slower answer but a silently missing one.

The second is that this process stays resident. memnest runs as a background service on a workstation that is also running an editor, a browser, and the agent itself. A 768-dimension model at roughly 3 KB per vector (`core/src/embedding/mod.rs`) keeps both the model and the index small enough to be forgettable. Resource use a user notices is a reason to uninstall a memory tool.

There is also a design reason the gap matters less here than the table suggests: retrieval is hybrid. A weaker dense model is paired with BM25 rather than asked to carry the query alone, and the two are fused. On Mr. TyDi, dense retrieval alone scored 16.7 average MRR@10 while BM25 combined with the same dense retriever scored 41.7. Fusion recovers more than the difference between these model sizes.

Revisit this when any of the following becomes true: the recall path stops being synchronous with the prompt, measured recall quality on real stored memories is the top complaint, or fastembed ships a model with e5-large accuracy at e5-base cost. Changing it requires no code:

```bash
MEMNEST_EMBED_MODEL=BAAI/bge-m3
MEMNEST_EMBED_DIM=1024
```

Existing vectors were written at the old dimension, so a switch means a full index rebuild.

## Two indexes instead of one

Keyword search and vector search fail in different directions. BM25 matches an exact token, which is what a query like `port 3111` or a crate name needs, and misses a paraphrase entirely. Vector similarity matches the paraphrase and can drift past the literal string the user typed. Neither failure is rare in a memory store, where one memory is a config value and the next is a decision written in prose.

Because the query decides which behavior is needed and the query is not known at write time, a write pays for both. That is the cost this design accepts: every stored memory is indexed twice.

## RRF for fusion, not score blending

The two searches return incomparable numbers. A BM25 score of 7.3 and a cosine similarity of 0.82 cannot be added, and normalizing them requires assumptions about each distribution that do not hold across queries.

Reciprocal Rank Fusion discards the scores and uses only positions: `1.0 / (k + rank)` summed across both lists (`core/src/index/hybrid.rs`). A result ranked highly by both retrievers outranks one that dominates a single list. `k = 60` is the value from the original RRF paper and damps the gap between adjacent ranks, so a first place finish in one retriever cannot monopolize the fused list.

## MMR after fusion

Ranking by relevance alone lets near-duplicate memories occupy the entire result set, which is common here because superseded and re-saved memories are textually similar by construction. MMR selects each result by weighing relevance against dissimilarity to what is already chosen (`core/src/server/api.rs`), with `mmr_lambda = 0.5` splitting the two evenly.

## SQLite is the source of truth, indexes are derived

Three stores that can disagree is three times the corruption surface. Here SQLite holds the record and the indexes hold nothing that cannot be regenerated from it, so recovery is always defined: delete the index directory and rebuild.

The write path makes that guarantee real. The record and an `index_queue` row are written in one transaction, and the queue row is cleared only after both indexes are durable. A crash between those points leaves the row in place, and startup replays it. Without the queue there would be no way to tell a finished write from an interrupted one.

## Workspace scope is a hash, not a directory name

Memories are partitioned by working directory, but the partition key is a stable hash of the normalized absolute path rather than the path itself. A collection named after a client directory would leak that client's name into any listing or export.

Directory-basename collections from earlier versions still resolve as an alias, but only while exactly one registered workspace claims that name. The moment a second `api` workspace appears, the alias is disabled for both rather than guessing which one owns the existing rows.

## Secrets are separate from searchable memory

Ordinary memory text is stored unencrypted. Encrypting it would break the indexes that make it useful, and a memory store nobody can search has no reason to exist.

Credentials therefore do not belong in that path at all. They go to an AES-256-GCM vault whose ciphertext is bound to the secret key or server name, so moving a row elsewhere fails decryption. Values that look like credentials are redacted before they reach storage, which limits the damage when someone pastes a token into a memory by accident. Model-facing vault tools stay hidden unless `MEMNEST_EXPOSE_SECRET_TOOLS=1` is set, so an agent cannot read secrets it was never meant to see.

## The prompt hook never blocks a prompt

`memnest hook` runs on the critical path of a user pressing enter. If the service is down, the working directory is unknown, or anything else goes wrong, it prints nothing and exits 0. A memory tool that can prevent someone from sending a message is worse than one that occasionally forgets.

Retrieved text is labeled untrusted reference data, and markup inside stored text is escaped before injection. Stored memories are attacker-influenced input in any setup where an agent saves what it reads.

## Transcript capture is one path, not one per host

MCP describes tool calls, not session events, so a host that speaks MCP still cannot report "the user said this". The alternative to a per-host extension is to read what each host already writes to disk: `memnest watch` follows pi, Claude Code, and Codex transcript directories and stores only visible user and assistant text, skipping system prompts, reasoning, tool calls, images, and subagent sidechains.

This keeps host support to a table of transcript formats rather than N extensions to maintain, and it means a host with no extension API is still covered.
