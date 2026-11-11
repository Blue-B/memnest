# Retrieval benchmarks (memnest 0.5.1)

These are **retrieval-only** numbers: does the memory that holds the evidence come back in the first
results of `/search`? No language model is used anywhere in the measurement, in saving or in searching.
Numbers were measured on one machine on 2026-11-10 with the released 0.5.1 binary and can be
reproduced with the commands at the end. Per-question hits are in
[`benchmarks/retrieval/results/`](../benchmarks/retrieval/results/) so anyone can audit them.
Why questions are missed, and which small changes would help: [`benchmarks/retrieval/ANALYSIS.md`](../benchmarks/retrieval/ANALYSIS.md).
The first tables describe the released 0.5.1. Two later sections show unreleased builds measured with the same harness:
the keyword-query fix, and the ranking change that followed from the analysis
([After the ranking change](#after-the-ranking-change-unreleased-build-1f20127-same-harness), measured 2026-11-11).

## At a glance, and how to read this page

The test works like this. A public conversation dataset is stored in memnest one dialog turn per memory. The dataset's
authors marked, for each question, which turn or conversation holds the answer. Each question is sent to search unchanged,
and it counts as found when the marked item is in the results. Only ids are compared, so no person or model judges anything.

- **LoCoMo**: 10 conversations between two people over several months and about 2,000 questions. The exact turn that
  holds the answer has to be found.
- **LongMemEval-S**: about 50 conversations between a user and an assistant and 500 questions. The conversation (session)
  that holds the answer has to be found. Only two samples of 100 questions (200 in total) were run here.
- **R@1, R@5, R@10**: out of 100 questions, in how many the answer was the first result, among the first 5, among the
  first 10. 69 means 69 found and 31 missed.

| Test | What is counted | Released 0.5.1 | Build with the ranking change |
|---|---|---|---|
| LongMemEval-S (200 questions) | right conversation is first | 83% | 91.5% |
| | right conversation in the first 5 | 93.5% | 96% |
| LoCoMo (the 5 conversations not used to tune the ranking, 776 questions) | the turn with the answer is first | 39.8% | 42.9% |
| | in the first 5 | 60.2% | 68.7% |
| | in the first 10 | 69.5% | 76.8% |

For the same LongMemEval-S "first 5" figure, other tools report 93 to 95% when they store every turn, as memnest does, and
95 to 98% when they store whole conversations. Those figures are self-reported under different conditions, so no ranking
follows ([sources and conditions](#what-others-publish-for-these-datasets)).

What these numbers do not say: the datasets are English everyday conversations, so they say nothing about Korean or about
coding-work records. They count whether the memory was found, not whether a model then answered correctly. The build with
the ranking change is not released yet.

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

## After the keyword-query fix (unreleased build, same harness)

The miss analysis found that 0.5.1 drops the keyword half of a search when the question contains an apostrophe-s, a trailing
period and some other characters, and then returns 3 results instead of 10 (330 of 1,986 LoCoMo questions, 17 of the 100
sampled LongMemEval questions). The fix searches such a question again as plain words. The same harness was run against a
build of commit `7937264` (0.5.1 plus that one change). It is not a pure gain, and both directions are shown.

LoCoMo, full run, categories 1-4 (n = 1,536), recall in percent:

| Build | R@1 | R@3 | R@5 | R@10 | all-evidence R@10 | R@50 (probe) | MRR@10 |
|---|---|---|---|---|---|---|---|
| 0.5.1 | 39.7 | 55.4 | 60.7 | 69.1 | 56.4 | 82.1 | 0.490 |
| with the fix | 40.0 | 54.7 | 60.9 | 70.7 | 58.1 | 86.9 | 0.491 |

Paired on the same questions, R@10 gained 45 questions and lost 21. Questions that got fewer than 10 results: 238 before,
2 after. R@3 is 0.7 points lower. All five categories: R@10 66.7 to 69.0. Search latency p50 37.5 ms, p95 45.3 ms and
ingest 6.9 memories/s in that run (1-minute load 2.2 on average).

LongMemEval-S: only the 17 affected questions of the sample were run again; the other 83 do not reach the changed code and
were not re-run. Session recall on those 17:

| Build | R@1 | R@3 | R@5 | R@10 |
|---|---|---|---|---|
| 0.5.1 (vector search only for these questions) | 16 of 17 | 16 | 16 | 16 |
| with the fix (keyword and vector) | 11 of 17 | 16 | 16 | 16 |

Five questions whose answer session was first are now second or third. With the 83 unchanged rows, the sample's session R@1
would be 78 instead of 83; R@3, R@5 and R@10 stay at 89, 91 and 94. So on long conversational turns the vector half alone
ranked the first result better than the hybrid ranking does. The fix is kept because a search should not silently return 3
results and lose exact-term matching depending on punctuation, but the first-rank ordering of the hybrid ranking is an open
problem that this measurement exposes. Result files:
`2026-11-10-unreleased-7937264-locomo.json` and `2026-11-10-unreleased-7937264-longmemeval-affected17.json`.

## After the ranking change (unreleased build `1f20127`, same harness)

The miss analysis led to an ablation of the ranking itself ([ANALYSIS.md section 8](../benchmarks/retrieval/ANALYSIS.md)).
Its main finding: the bonus for the share of question words found in a memory (up to 0.15) was several times larger than
the score from the two retrievers' rank lists (at most 0.033), so a long text that repeated more of the question's words
outranked the memory both retrievers put first. Commits `42e7a66` and `1f20127` change the fusion (RRF k 60 to 10, the
keyword list counts half as much as the vector list) and the balance (0.10 for rank, 0.08 for the keyword share). Candidate
pools, distance cutoff, fallback slots, MMR and the embedding model are unchanged, and nothing is stored differently.

How it was measured:

* **Same stored data for every build.** The turns were ingested once with the released 0.5.1 binary into scratch data
  directories that were kept (`run.py --data-dir`), and each build, 0.5.1 included, was started on a copy and answered
  the questions (`--kept-only`). That is why the 0.5.1 rows here differ a little from the tables above: there the
  questions were asked in the process that had just stored the turns, here after a restart. The same build then orders
  the results differently for 869 of 1,986 LoCoMo questions, and its figures move by up to 0.3 points (the cause was not
  isolated). Every row in this section was measured after a restart, so the comparison is like for like.
* **Tuned and held-out data are reported separately.** The constants were chosen on LoCoMo conversations 0-4 and on the
  LongMemEval-S sample above (seed 1110). LoCoMo conversations 5-9 and a second LongMemEval-S sample of 100 (seed 1111,
  no question in common with the first) were kept aside. The second LongMemEval sample was asked once, after the final
  commit. LoCoMo's held-out half was measured twice, because a problem outside the benchmark was found after the first run
  and fixed (a score limit, below); both results are in ANALYSIS.md section 8.7 and differ only in one column by 0.2.
* "wins/losses" are paired on the same questions: questions that gained a hit at that k / questions that lost one.

### LoCoMo

Categories 1-4, any-evidence recall in percent:

| Data | Build | R@1 | R@3 | R@5 | R@10 | all-evidence R@10 | R@50 (probe) | MRR@10 |
|---|---|---|---|---|---|---|---|---|
| all 10 conversations (n = 1,536) | 0.5.1 | 39.5 | 55.5 | 61.0 | 69.1 | 56.5 | 82.2 | 0.489 |
| | develop `8c41b79` (keyword-query fix) | 39.6 | 54.8 | 61.1 | 70.6 | 58.1 | 86.9 | 0.490 |
| | **`1f20127`** | **43.6** | **62.8** | **69.4** | **77.9** | 64.8 | 91.1 | 0.548 |
| tuned: conversations 0-4 (n = 760) | 0.5.1 | 39.2 | 56.6 | 61.8 | 68.8 | 57.9 | 81.8 | 0.489 |
| | develop `8c41b79` | 39.1 | 56.2 | 62.2 | 70.7 | 59.5 | 86.4 | 0.489 |
| | `1f20127` | 44.3 | 63.7 | 70.1 | 79.1 | 67.0 | 90.7 | 0.556 |
| **held-out: conversations 5-9 (n = 776)** | 0.5.1 | 39.8 | 54.5 | 60.2 | 69.5 | 55.2 | 82.5 | 0.489 |
| | develop `8c41b79` | 40.2 | 53.5 | 60.1 | 70.6 | 56.7 | 87.4 | 0.491 |
| | **`1f20127`** | **42.9** | **62.0** | **68.7** | **76.8** | 62.6 | 91.6 | 0.541 |

| `1f20127` wins/losses | R@1 | R@3 | R@5 | R@10 |
|---|---|---|---|---|
| against 0.5.1, all 10 conversations | 111/48 | 168/56 | 178/49 | 166/31 |
| against 0.5.1, held-out half | 55/31 | 86/28 | 93/27 | 76/19 |
| against develop, held-out half | 52/31 | 86/20 | 88/21 | 66/18 |

The held-out gain is smaller than the tuned one: against 0.5.1, R@1 +3.1 (tuned half +5.1) and R@10 +7.3 (+10.3).

Per category with `1f20127`, all 10 conversations:

| Category | n | R@1 | R@3 | R@5 | R@10 | all-evidence R@10 | MRR@10 |
|---|---|---|---|---|---|---|---|
| **Categories 1-4** | 1536 | 43.6 | 62.8 | 69.4 | 77.9 | 64.8 | 0.548 |
| single-hop | 841 | 45.8 | 66.0 | 73.6 | 82.4 | 79.7 | 0.577 |
| multi-hop | 282 | 32.3 | 55.3 | 61.3 | 70.9 | 20.9 | 0.448 |
| temporal | 321 | 54.2 | 68.8 | 73.5 | 79.8 | 73.5 | 0.630 |
| open-domain | 92 | 21.7 | 35.9 | 41.3 | 52.2 | 32.6 | 0.305 |
| adversarial (cat. 5) | 446 | 26.9 | 46.4 | 55.4 | 66.6 | 65.2 | 0.390 |
| all 5 categories | 1982 | 39.9 | 59.1 | 66.2 | 75.4 | 64.9 | 0.513 |

**One thing got worse:** the adversarial category (not part of the scored set) lost first-rank hits: R@1 30.9 (0.5.1) and
32.5 (develop) to 26.9, while its R@10 rose from 58.1 and 63.0 to 66.6. The cause was not investigated.

### LongMemEval-S (two samples of 100, not the full set)

Session recall in percent:

| Sample | Build | R@1 | R@3 | R@5 | R@10 | turn R@10 (any) |
|---|---|---|---|---|---|---|
| tuned: seed 1110 (the sample above) | 0.5.1 | 83 | 89 | 91 | 94 | 79 |
| | develop `8c41b79` | 78 | 89 | 91 | 94 | 79 |
| | `1f20127` | 89 | 93 | 94 | 95 | 92 |
| **held-out: seed 1111** | 0.5.1 | 83 | 93 | 96 | 98 | 84 |
| | develop `8c41b79` | 83 | 93 | 96 | 98 | 83 |
| | **`1f20127`** | **94** | **98** | **98** | **99** | 93 |

| `1f20127` wins/losses against 0.5.1 | R@1 | R@3 | R@5 | R@10 |
|---|---|---|---|---|
| tuned sample | 7/1 | 4/0 | 3/0 | 1/0 |
| held-out sample | 11/0 | 5/0 | 2/0 | 1/0 |

The five questions that had dropped from rank 1 with the keyword-query fix are first again. The second sample is easier
than the first for every build, which is the spread to expect from samples of 100 (approximate 95% interval of 98 of 100:
93.0 to 99.4; of 94 of 100: 87.5 to 97.2). Over both samples (200 questions) session R@5 is 96.0 with `1f20127` and 93.5
with 0.5.1; R@1 is 91.5 and 83.0. Requiring every answer session in the first 5: 80 and 88 of 100 with `1f20127` (tuned,
held-out).

Per question type with `1f20127`, held-out sample:

| Question type | n | session R@1 | R@3 | R@5 | R@10 | NDCG@10 | turn R@10 (any) | turn R@10 (all) |
|---|---|---|---|---|---|---|---|---|
| **All (non-abstention)** | 100 | 94.0 | 98.0 | 98.0 | 99.0 | 0.936 | 93.0 | 82.0 |
| knowledge-update | 15 | 100.0 | 100.0 | 100.0 | 100.0 | 0.985 | 93.3 | 93.3 |
| multi-session | 26 | 96.2 | 96.2 | 96.2 | 96.2 | 0.891 | 92.3 | 53.8 |
| single-session-assistant | 12 | 100.0 | 100.0 | 100.0 | 100.0 | 1.000 | 100.0 | 100.0 |
| single-session-preference | 6 | 50.0 | 83.3 | 83.3 | 100.0 | 0.770 | 66.7 | 50.0 |
| single-session-user | 14 | 100.0 | 100.0 | 100.0 | 100.0 | 1.000 | 92.9 | 92.9 |
| temporal-reasoning | 27 | 92.6 | 100.0 | 100.0 | 100.0 | 0.928 | 96.3 | 96.3 |

On the tuned sample the questions still missed at R@5 are four temporal-reasoning questions with a relative date and two
preference questions (ANALYSIS.md section 8.8).

### Checks outside the two datasets

The datasets only see the order of results, and all their memories have the same type, importance and age. Two things
they cannot show were checked separately (details and numbers in ANALYSIS.md section 8.6):

* **The score as an absolute value.** The pi extension injects a saved memory when its score is 0.25 or more. The first
  version of the change let unrelated saved memories pass that limit (9 of 9 results for three unrelated questions; 0 of 9
  before). `1f20127` caps the rank part of a memory that contains none of the question's words, which restores 0 of 9, and a
  core test holds it. That cap costs one LongMemEval question at R@3 and R@5 on the tuned sample (95 to 94) and 0.2 points
  LoCoMo R@5.
* **A real store.** On a read-only copy of a store in daily use (55,251 memories, Korean and English), 40 saved memories
  were picked at random across ages and a question was written for each before any build was run. The memory came back
  first / in the first 3 / in the first 10 for 13 / 24 / 30 of 40 questions with 0.5.1 and with develop, and for 16 / 26 / 31
  with `1f20127`; for memories older than 30 days, 6 / 12 / 15 of 20 before and 8 / 13 / 16 after. 40 questions and one
  store are a small check: it shows no sign that older memories are buried, not a measured gain.
* Korean and mixed-language sanity set in `core/src/server/api.rs` (`ranking_sanity_tests`) and the Korean retrieval fixture
  (`benchmarks/memorybench/run-korean-retrieval.ts`, 3 questions): every expected memory is still first.

### What others publish for these datasets

For orientation only. **These figures are self-reported by each project, were not re-run by us, and were measured under
different conditions:** a different embedding model (most use all-MiniLM-L6-v2; memnest uses multilingual-e5-base), most
on the full 500 or 470 questions where ours are two samples of 100, and most index whole sessions where memnest indexes
every turn. Pages retrieved 2026-11-11.

LongMemEval-S, retrieval only, session Recall@5 unless noted:

| Source | Figure |
|---|---|
| [taOSmd benchmarks](https://github.com/jaylfc/taosmd/blob/25b35afb/docs/benchmarks.md) | taOSmd 97.0 (hybrid with query expansion); its "all-turns hybrid (harder test)" 93.2. The same page lists MemPalace 96.6, agentmemory 95.2 and SuperMemory 81.6 as other published systems |
| [mempal summary](https://github.com/ZhangHanDong/mempal/blob/main/benchmarks/longmemeval_s_summary.md) | sessions indexed: R@1 80.6, R@5 96.6, R@10 98.2; every turn indexed: R@1 83.6, R@5 94.6, R@10 96.4 |
| [Uteke results](https://github.com/codecoradev/uteke/blob/develop/benchmarks/longmemeval/RESULTS.md) | any answer session in the first 5: 98.2 on the full 500 (v0.20.0); all answer sessions in the first 5: 88.0 |
| memnest `1f20127` (this page; every turn indexed) | tuned sample: R@1 89, R@5 94, R@10 95; held-out sample: R@1 94, R@5 98, R@10 99; all answer sessions in the first 5: 80 and 88 |

LoCoMo, retrieval only, in the metric of [mnema's BENCHMARKS.md](https://docs.rs/crate/mnema/latest/source/BENCHMARKS.md):
the mean share of a question's evidence turns found in the first k results, over the 1,981 answerable questions of all five
categories, every turn stored as `{speaker}: {text}`. memnest's figure is computed the same way from the per-question
files (`evidence_fraction@k`, 1,982 questions with evidence); memnest's stored text also carries the session date, so the
ingestion is not identical.

| Source | R@5 | R@10 |
|---|---|---|
| mnema page: BM25 | 45.9 | 53.8 |
| mnema page: TF-IDF | 43.7 | 52.9 |
| mnema page: mnema default | 40.1 | 46.7 |
| mnema page: its best tuned dense + BM25 presets | 49.0 (keyword-favoured) | 59.9 (balanced) |
| memnest 0.5.1 (the published run above) | 54.0 | 61.2 |
| memnest `1f20127` (half of the conversations were used for tuning) | 60.6 | 69.9 |
| memnest `1f20127`, held-out conversations 5-9 only (985 questions) | 59.7 | 68.8 |

### Speed with the change

The change adds no search work: the same two candidate lists are fetched and only the arithmetic on them differs.
LoCoMo, serial requests, `n_results=10`, 1,986 requests per build, each build started on a copy of the kept data, one
after the other on a quiet machine (1-minute load 1.4 on average in every run):

| Build | client p50 | client p95 | server-reported p50 / p95 |
|---|---|---|---|
| 0.5.1 | 37.8 ms | 44.2 ms | 37 / 43 ms |
| develop `8c41b79` | 38.3 ms | 44.0 ms | 37 / 43 ms |
| `1f20127` | 37.3 ms | 43.0 ms | 36 / 42 ms |

Differences of one millisecond are within run-to-run noise. Ingest is not affected by the change and was not timed again.


Result files, all asked against the kept data: `2026-11-11-unreleased-1f20127-kept-locomo.json`,
`...-kept-longmemeval-sample100.json` (seed 1110) and `...-kept-longmemeval-heldout100.json` (seed 1111); the same three
for develop (`2026-11-11-unreleased-8c41b79-kept-*`) and for 0.5.1 (`2026-11-11-v0.5.1-kept-*`).

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
  see ANALYSIS.md section 4.1. The 0.5.1 tables include that effect: they are what the released 0.5.1 does. The section "After the keyword-query fix" shows the build that fixes it.
* The ranking constants of the unreleased build were chosen on half of LoCoMo and on the first LongMemEval sample. Its figures on that data
  are tuned figures; the held-out rows are the ones to quote. Its two LongMemEval samples differ by 4 points at R@5 (94 and 98), which is the
  size of the sampling error.

## Reproduce

```sh
python3 benchmarks/retrieval/test_metrics.py                                   # metric self-test
python3 benchmarks/retrieval/run.py --binary ~/.local/bin/memnest --dataset locomo                          # ~25 min
python3 benchmarks/retrieval/run.py --binary ~/.local/bin/memnest --dataset longmemeval --sample 100 --seed 1110   # the sample on this page; ~3.5 h
# the second (held-out) sample: same size, other seed, none of the first sample's questions
python3 benchmarks/retrieval/run.py --binary BIN --dataset longmemeval --sample 100 --seed 1111 \
    --exclude-ids-from benchmarks/retrieval/results/2026-11-10-v0.5.1-longmemeval-sample100.json
# compare builds on the same stored data: ingest once into a kept directory, then ask again with another binary on a copy
python3 benchmarks/retrieval/run.py --binary BIN --dataset locomo --data-dir /tmp/bench-kept/locomo
cp -a /tmp/bench-kept/locomo /tmp/bench-kept/locomo-new
python3 benchmarks/retrieval/run.py --binary NEW_BIN --dataset locomo --data-dir /tmp/bench-kept/locomo-new --kept-only
# full LongMemEval-S (about 246,600 memories; ~17 h extrapolated from the sample, not measured), can be sharded over instances:
python3 benchmarks/retrieval/run.py --binary BIN --dataset longmemeval --shard 0/2 --port 3981 --checkpoint /tmp/lme0.jsonl --out-dir /tmp/lme-shards --tag shard0 &
python3 benchmarks/retrieval/run.py --binary BIN --dataset longmemeval --shard 1/2 --port 3982 --checkpoint /tmp/lme1.jsonl --out-dir /tmp/lme-shards --tag shard1 &
python3 benchmarks/retrieval/merge.py --out benchmarks/retrieval/results/<date>-v<version>-longmemeval.json /tmp/lme-shards/*.json
```

Replace the binary path to test another build; the result file name carries the version reported by `/health`.
Give each parallel instance its own `--port` (3981-3989); a run needs the embedding model in `~/.memnest/models`
(hard-linked into the scratch data directory). Run timing passes on an otherwise idle machine and check `uptime`;
the result file records the machine load.
