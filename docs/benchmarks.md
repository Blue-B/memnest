# Retrieval benchmarks (memnest 0.5.1)

These are **retrieval-only** numbers: does the memory that holds the evidence come back in the first
results of `/search`? No language model is used anywhere in the measurement, in saving or in searching.
Numbers were measured on one machine on 2026-11-10 with the released 0.5.1 binary and can be
reproduced with the commands at the end. Per-question hits are in
[`benchmarks/retrieval/results/`](../benchmarks/retrieval/results/) so anyone can audit them.
Why questions are missed, and which small changes would help: [`benchmarks/retrieval/ANALYSIS.md`](../benchmarks/retrieval/ANALYSIS.md).

## What is measured, and what is not

Measured: Recall@k (is at least one evidence turn / evidence session among the first k results),
all-evidence recall (every evidence turn present), MRR@10 and NDCG@10, plus ingest throughput and search latency.

**Not measured: end-to-end question-answering accuracy.** LoCoMo and LongMemEval are usually reported as
the share of questions an LLM answers correctly, judged by another LLM. That depends on the reader model, the
prompt and the judge. Retrieval recall is only the first stage: a question can be answered although the
exact evidence turn was not returned, or answered wrongly although it was. **These numbers are not comparable to
the accuracy numbers other memory tools publish for the same datasets**, and no claim of "better/worse than tool X"
follows from this page.

## Setup

| Item | Value |
|---|---|
| Binary | released `memnest 0.5.1` (`/health` version `0.5.1`, sha256 `39a420937cd9c41d...`), embedding model `intfloat/multilingual-e5-base`, local CPU inference |
| Instance | a scratch instance per run: fresh temporary data directory, loopback port 3981-3989, no `MEMNEST_*` environment overrides, deleted afterwards. The model cache is hard-linked from `~/.memnest/models`; nothing in `~/.memnest` is modified |
| What is stored | **one memory per dialog turn**, through `POST /add` with the metadata the transcript watcher sends (`chunk_type=auto_log`, `importance=log`, `source=*.transcript`, session id, event id). Text: `[<session date>] <speaker> said: <turn text>` for LoCoMo (two named speakers), `[<session date>] User said: ...` / `Assistant answered: ...` for LongMemEval (the watcher's own labels). The real watcher stores no date in the text; it is added here because replayed turns would otherwise carry no date at all (effect measured in ANALYSIS.md section 4.5: about +1.5 points R@10) |
| Scope | one project per LoCoMo conversation, one project per LongMemEval question haystack, so a question only searches its own haystack |
| Search request | `POST /search {"query": "<question text, unchanged>", "project": "<its project>", "n_results": 10}`; everything else is the server default (hybrid BM25 + vector, RRF fusion, MMR `lambda=0.5`, distance cutoff 0.7, 3 vector-only fallback slots). One request per question; Recall@1/3/5 are taken from the same 10-result list |
| Why `n_results=10` | The product default is `n_results=3` (HTTP and MCP). Recall@10 needs ten results, and the first three of a 10-result request match a separate `n_results=3` request to within 0.3 points (ANALYSIS.md section 4.4). So the table shows what an agent that asks for 10 gets, and R@3 is what the default returns |
| Sessions in LongMemEval | a session counts as retrieved when any of its turns is among the returned turns (so at most 10 sessions are seen) |

## LoCoMo (full: all 10 conversations, 5,882 turns)

Dataset `locomo10.json` from `snap-research/locomo` (commit 3eb6f2c, sha256 `79fa87e9...`). 1,986 questions;
4 have no usable evidence and are skipped, 2 evidence ids that do not exist in their conversation were dropped, malformed
ids such as `D9:1 D4:4` were normalised. Categories: single-hop, multi-hop, temporal, open-domain; the fifth category
(adversarial) asks about things the speakers never said and is shown separately. Recall is in percent, "R@k" means
any evidence turn in the top k, "all-evidence R@10" means every evidence turn is in the top 10.

| Category | n | R@1 | R@3 | R@5 | R@10 | all-evidence R@10 | MRR@10 |
|---|---|---|---|---|---|---|---|
| **Categories 1-4** | 1536 | 39.7 | 55.4 | 60.7 | 69.1 | 56.4 | 0.490 |
| single-hop | 841 | 43.3 | 60.0 | 65.8 | 72.9 | 70.2 | 0.529 |
| multi-hop | 282 | 25.2 | 41.5 | 46.8 | 60.3 | 14.5 | 0.355 |
| temporal | 321 | 50.2 | 62.3 | 67.3 | 74.5 | 67.6 | 0.576 |
| open-domain | 92 | 15.2 | 31.5 | 34.8 | 43.5 | 20.7 | 0.238 |
| adversarial (cat. 5) | 446 | 31.2 | 46.9 | 52.7 | 58.3 | 57.4 | 0.404 |
| all 5 categories | 1982 | 37.8 | 53.5 | 58.9 | 66.7 | 56.7 | 0.470 |

Approximate 95% interval for the category 1-4 R@10: 69.1 (66.7 to 71.4), R@1: 39.7 (37.3 to 42.2).
Multi-hop questions have several evidence turns, so their all-evidence recall (14.5%) is much lower than any-hit recall (60.3%).

## LongMemEval-S (**sample: 100 of 470 non-abstention questions, not the full set**)

Dataset `longmemeval_s_cleaned.json` (the file the LongMemEval README currently points to, Hugging Face
`xiaowu0162/longmemeval-cleaned` revision 98d7416, MIT license, sha256 `d6f21ea9...`). Each question has its own haystack
of about 50 sessions and 400-600 turns.

**This is a fixed-seed stratified sample, not the full benchmark.** 100 questions were drawn from the 470 non-abstention
questions, proportionally per question type (seed 1110; ids are listed in the result file). The full set would be
about 246,600 memories (mean turn length about 990 characters, six times LoCoMo's) and did not fit the time budget:
the 49,117 memories of this sample took 3.5 hours with two scratch instances in parallel. The 30 abstention
questions of the full set are skipped for recall by design and none was sampled. With 6 to 27 questions per type, the
per-type rows are rough: a single question moves them by 4 to 17 points. The overall figures have an approximate 95%
interval of 83.0 (74.5 to 89.1) for session R@1 and 94.0 (87.5 to 97.2) for session R@10.

| Question type | n | session R@1 | R@3 | R@5 | R@10 | NDCG@10 | turn R@10 (any) | turn R@10 (all) |
|---|---|---|---|---|---|---|---|---|
| **All (non-abstention)** | 100 | 83.0 | 89.0 | 91.0 | 94.0 | 0.837 | 79.0 | 54.0 |
| knowledge-update | 15 | 100.0 | 100.0 | 100.0 | 100.0 | 0.956 | 93.3 | 80.0 |
| multi-session | 26 | 80.8 | 96.2 | 100.0 | 100.0 | 0.841 | 84.6 | 30.8 |
| single-session-assistant | 12 | 100.0 | 100.0 | 100.0 | 100.0 | 1.000 | 75.0 | 75.0 |
| single-session-preference | 6 | 50.0 | 50.0 | 50.0 | 66.7 | 0.559 | 50.0 | 33.3 |
| single-session-user | 14 | 100.0 | 100.0 | 100.0 | 100.0 | 1.000 | 92.9 | 92.9 |
| temporal-reasoning | 27 | 66.7 | 74.1 | 77.8 | 85.2 | 0.671 | 66.7 | 37.0 |

| Question type | n | session all-answer-sessions R@5 | R@10 |
|---|---|---|---|
| **All (non-abstention)** | 100 | 73.0 | 78.0 |
| knowledge-update | 15 | 86.7 | 93.3 |
| multi-session | 26 | 57.7 | 69.2 |
| single-session-assistant | 12 | 100.0 | 100.0 |
| single-session-preference | 6 | 50.0 | 66.7 |
| single-session-user | 14 | 100.0 | 100.0 |
| temporal-reasoning | 27 | 59.3 | 59.3 |

"session R@k": any answer session among the sessions of the top-k turns; "all-answer-sessions" requires every
answer session (multi-session questions have up to 6). "turn R@10": turns marked `has_answer` by the dataset; "any" = at least one, "all" = all of them.
Temporal-reasoning questions are sent exactly as written (for example "What kitchen appliance did I buy 10 days ago?") without the
dataset's `question_date`, so relative expressions cannot be resolved by search; they are the weakest type.

## Speed (single-machine measurement)

| Measure | Value |
|---|---|
| Ingest throughput, LoCoMo (short turns, mean 164 characters), one instance, serial `POST /add` | **6.7 memories/s** (5,882 memories in 881 s) |
| Search latency, LoCoMo, serial requests, `n_results=10`, 1,986 requests | **p50 38.6 ms, p95 47.6 ms** (wall time at the client, includes embedding the query; server-reported p50 37 ms, p95 46 ms) |
| Machine load during that pass | 1-minute load average 1.6 at start, 1.9 on average (10 logical CPUs) |
| LongMemEval ingest | not measured on a quiet machine. Turns are about six times longer and the sample ran two instances in parallel while other jobs used the machine (about 2 memories/s per instance), so no throughput figure is published for it |

These timings come from one dedicated serial pass on a quiet machine. An earlier LoCoMo pass of the same data
while other jobs were running gave 5.4 memories/s and p95 68 ms; those load-affected figures are not used. Latency grows with the
number of memories in the searched project (exact vector scan over the project): the projects here hold 369-689
(LoCoMo) and 409-606 (LongMemEval) memories.

Hardware: AMD Ryzen 5 9600X (6 cores, 10 logical CPUs visible to WSL2), 23.5 GiB RAM, Linux 6.18 on WSL2, **no GPU** (CPU-only inference).

## Datasets, licenses, provenance

Datasets are downloaded to `~/.cache/memnest-bench` and are never committed; the result files hold the source URL, revision, license and sha256.

* LoCoMo: <https://github.com/snap-research/locomo>, CC BY-NC 4.0 (non-commercial; used here only to measure retrieval, not redistributed).
* LongMemEval: <https://github.com/xiaowu0162/LongMemEval>, cleaned release at <https://huggingface.co/datasets/xiaowu0162/longmemeval-cleaned>, MIT.

## Limits

* Retrieval recall, not QA accuracy (see above). English-only datasets: nothing here says how Korean or mixed-language retrieval performs.
* LongMemEval is a 100-question sample; LoCoMo has two fixed speakers per conversation and noisy evidence labels (some "evidence" turns are
  image-only or only an adjacent turn to the real answer). The turn text of image captions provided by LoCoMo was not ingested.
* The harness stores turns one by one; real use also includes tool output, code and other noise that these dialogues do not contain.
* One machine, one run per configuration. Two LoCoMo runs of the same data differed in the order of the top 10 for 9 of 1,986 questions
  (all metrics within 0.1 point), so tiny differences between builds are not meaningful.
* 330 of the 1,986 LoCoMo questions (17%; they contain an apostrophe, such as "Caroline's", or a colon) lose the keyword (BM25) half of the search in 0.5.1 and return only 3 results;
  see ANALYSIS.md section 4.1. The tables above include that effect: they are what 0.5.1 does today.

## Reproduce

```sh
python3 benchmarks/retrieval/test_metrics.py                                   # metric self-test
python3 benchmarks/retrieval/run.py --binary ~/.local/bin/memnest --dataset locomo                          # ~25 min
python3 benchmarks/retrieval/run.py --binary ~/.local/bin/memnest --dataset longmemeval --sample 100 --seed 1110   # the sample on this page; ~3.5 h
# full LongMemEval-S (about 246,600 memories; ~17 h extrapolated from the sample, not measured), can be sharded over instances:
python3 benchmarks/retrieval/run.py --binary BIN --dataset longmemeval --shard 0/2 --port 3981 --checkpoint /tmp/lme0.jsonl --out-dir /tmp/lme-shards --tag shard0 &
python3 benchmarks/retrieval/run.py --binary BIN --dataset longmemeval --shard 1/2 --port 3982 --checkpoint /tmp/lme1.jsonl --out-dir /tmp/lme-shards --tag shard1 &
python3 benchmarks/retrieval/merge.py --out benchmarks/retrieval/results/<date>-v<version>-longmemeval.json /tmp/lme-shards/*.json
```

Replace the binary path to test another build; the result file name carries the version reported by `/health`.
Give each parallel instance its own `--port` (3981-3989); a run needs the embedding model in `~/.memnest/models`
(hard-linked into the scratch data directory). Run timing passes on an otherwise idle machine and check `uptime`;
the result file records the machine load.
