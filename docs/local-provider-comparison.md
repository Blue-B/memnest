# Local provider comparison

This is a small reproducible experiment, not a claim that Memnest is the best memory system. It used the fixed `scripts/fixtures/coding-memory.json`: 16 authored records, their 15-record current snapshot, and 48 questions (22 with a nominated answer document and 26 without one).

## Conditions

All four rows ran real storage/retrieval paths. The answering model and prompt were the same local Qwen2.5:3b (Ollama model digest `357c53fb659c`), temperature zero. No paid API was called. Provider sockets were restricted to loopback and paid-provider environment credentials were removed.

- Memnest: actual HTTP service, multilingual-e5-base, hybrid search, top five excerpts.
- memU: pinned source `2c050bc9681a4c0aff1af211a000e73d14f33356`, 0.11.0b3; actual `commit_results` and `progressive_retrieve`; local BGE-M3.
- Hindsight: pinned source `9269b88417ed263e5a8350f2e416ca2b322756b1`, 0.10.2; actual `retain_async` and `recall_async`; local Qwen extraction, local BGE-M3, PostgreSQL/pgvector, RRF reranking. An empty bank was created explicitly for the empty-scope question.
- Files: published Unicode-token intersection baseline, zero-overlap abstention, top five. It is intentionally simple, not a tuned search product.

memU received prepared verbatim memory files. Its external-agent evolution step was not tested. All providers received the same current snapshot, so this does not measure update/conflict resolution from a raw chronological conversation. Their internal model stacks differ. Hindsight's local small-model extraction results must not be substituted for its hosted or published benchmark scores.

## Results

| Measure | Memnest | memU | Hindsight | Files |
| --- | ---: | ---: | ---: | ---: |
| Completed retrieval/answer queries | 48/48 | 48/48 | 48/48 | 48/48 |
| Nominated document at rank 1 | 20/22 | 22/22 | 21/22 | 17/22 |
| Nominated document in top 5 | 21/22 | 22/22 | 22/22 | 17/22 |
| Empty retrieval on no-answer questions | 1/26 | 1/26 | 1/26 | 8/26 |
| Positive answers passing the term check | 16/22 | 18/22 | 10/22 | 10/22 |
| Negative answers saying `NOT_FOUND` | 26/26 | 26/26 | 26/26 | 26/26 |
| Retrieval median | 55.2 ms | 337.5 ms | 448.6 ms | 0.089 ms |
| Retrieval p95 | 86.2 ms | 698.2 ms | 733.5 ms | 0.278 ms |

The term check is in `scripts/compare-local-memory.py`. It checks required numbers, commands, and field names. It is not a semantic answer-quality judge: a paraphrase may fail and an incomplete answer may pass. Document retrieval is not answer accuracy. Answering model time and tokens are separate from retrieval latency.

The run was serial and cache-sensitive on one Linux host. Memnest's recorded ingestion took about 19 seconds in the first run; memU about 12 seconds; Hindsight about 299 seconds. Treat these as observations of this setup, not stable costs or throughput claims. The later complete run retained its own ingestion times in the raw result.

One first run did not provision Hindsight's empty bank and recorded a client/setup error, not a quality result. The corrected run completed its 48 queries. Its file baseline hit a 900-second command deadline after six questions, then resumed the remaining questions with the same fixture/model/prompt. The raw report states that continuation; no unfinished rows were counted as completed.

The honest interpretation is that Memnest was fast here but did not beat memU or Hindsight on finding the nominated document. Its selection reason must therefore include its source-history workflow and operational controls, not an unsupported accuracy claim.

## Optional local source selection

An independent experiment added `evidence_only=true` to Memnest's search. A configured local model selected bounded source excerpts with constrained JSON; the core checked that the returned IDs and text belonged to supplied sources.

It found a nominated answer in 20/22 positive questions and returned no selected excerpt for all 26 negatives. Ordinary search found 21/22 positives and returned an empty list for only one negative. Median/p95 latency became about 2.45/5.92 seconds. These costs include an additional local model whose RAM is not included in the core's roughly 1.7 GB RSS.

This tradeoff is opt-in, not the new default. A model-selected excerpt is not a verified fact, and absence within five 1000-character excerpts does not prove absence from the full history. Default Memnest storage/search still makes no generative model call.

## Reproduction

Use an isolated environment with the pinned provider sources and their dependencies. Run a local Ollama with Qwen2.5:3b and BGE-M3 and a disposable local PostgreSQL/pgvector. Do not use a production memory store.

```sh
/path/to/comparison-venv/bin/python scripts/compare-local-memory.py \
  --model-url http://127.0.0.1:<actual-model-port> \
  --postgres-url postgresql://postgres@127.0.0.1:<actual-db-port>/hindsight \
  --output /private/comparison.json
```

The command validates loopback endpoints, creates its own Memnest scratch store, and records per-question contexts, answers, usage, and errors. Record the actual binary hash, source revisions, package versions, model digests, host, and raw artifact when repeating it. Raw private histories must never replace the authored fixture in a public report without a separate privacy review.
