# Why retrieval misses: analysis and measured emulations (memnest 0.5.1)

Scope: why questions are missed in `results/2026-11-10-v0.5.1-locomo.json` (full LoCoMo, 1,982 questions with
evidence) and `results/2026-11-10-v0.5.1-longmemeval-sample100.json` (100-question sample), and what small
changes to memnest's search would help. The headline tables are in [docs/benchmarks.md](../../docs/benchmarks.md).
**No product code was changed for sections 1-7.** Every number in them was produced by scripts in this directory
(`analyze_misses.py`, `sample_misses.py`, `variants.py`, `variant_tables.py`, `probe_query_syntax.py`) on binary 0.5.1.
Section 8 (2026-11-11) is the ranking ablation and the change that followed from it.

A "miss" is a question with none of its evidence turns in the top 10 results. LoCoMo categories 1-4 are the scored
question types; category 5 (adversarial) asks about things the speakers never said, so it is reported separately.
Of the 1,536 category 1-4 questions, **474 (30.9%) miss**; over all five categories 660 of 1,982 (33.3%).

## 1. Method

1. `analyze_misses.py` attaches non-exclusive heuristic features to every miss (evidence rank in a 50-result probe,
   number of evidence turns, number of results returned, word overlap between question and evidence, evidence length,
   relative-date wording). It is a triage aid; the cause shares below come from reading, not from these labels.
2. I read a **fixed random sample of 48 category 1-4 misses** (`sample_misses.py`, `random.Random(1110)`; the miss set is
   identical in both full LoCoMo runs) and assigned each one a primary cause by hand (section 2). 48 of 474 is small,
   so each share has roughly +/-7 points of sampling error. Section 3 gives population counts that do not depend on the sample.
3. For causes that look fixable I emulated the change through the existing HTTP API (rewritten query text, different `n_results`)
   on **four fixed conversations** (`conv-26, conv-30, conv-41, conv-42`; 755 scored questions, 582 in categories 1-4),
   ingested once, every variant asked against the same data (paired comparison). The variants are in `variants.py`; the complete
   tables are in `results/2026-11-10-v0.5.1-locomo-variants.json` (`variant_tables.py` prints them). Rerunning the whole experiment
   gave the same base figures. Changes the API cannot emulate are marked **not measured**.

## 2. Hand-classified sample (48 category 1-4 misses)

| Cause | n | Examples (conversation `#` question index) |
|---|---:|---|
| **B. Right place, wrong turn**: the top results are neighbours (same session, within 1-3 turns) of the evidence; the answer is in the previous or next turn | 8 | `conv-26#127` "What did Caroline make for a local church?": top-1 says "It was made for a local church", the evidence is the turn before ("I made this stained glass window"); `conv-43#105` "excited to see at Universal Studios?": top-1 is the question "have you been to Universal Studios?", evidence is the reply "super stoked for the Harry Potter stuff" |
| **E. Multi-turn answers** (lists, counts, "what things"): one turn cannot answer | 8 | `conv-42#34` "What book recommendations has Joanna given to Nate?" (3 turns); `conv-50#59` "How many Ferraris does Calvin own?"; `conv-48#84` "engineering projects" (3 sessions) |
| **C. Dates and relative time**: the question asks about a calendar date or "when"; the evidence says "yesterday / last week / the day after tomorrow", so the date must be computed from the session date | 6 | `conv-47#32` "Where was James at on July 12, 2022?" (evidence: "leaving the day after tomorrow"); `conv-48#52` "When did Jolene lose a lot of progress?"; `conv-47#11` "How was John feeling on April 10, 2022?" |
| **D. Paraphrase, no shared words** between question and evidence | 6 | `conv-41#105` "honor the memories of his beloved pet" vs "considering adopting a rescue dog"; `conv-44#87` "trail hike ... take" vs "The hike took us two hours" (top-1 is another "trail" turn); `conv-50#132` "his music in relation to capturing feelings" vs "a way for me to express myself ... my own form of therapy" |
| **F. Just outside the cut** (evidence at rank 11-50, nothing else wrong) | 6 | `conv-43#88` "How did John get introduced to basketball?" (rank 19); `conv-43#115` "describe the game season" (rank 21); `conv-26#3` "What did Caroline research?" (rank 4 with `n_results=50`, absent with `n_results=10`, see 4.4) |
| **I. Needs world knowledge or inference** (LoCoMo "open-domain") | 5 | `conv-42#73` "What state did Joanna visit in summer 2021?" (evidence: "near Fort Wayne"); `conv-43#40` "Has Tim been to North Carolina and/or Tennessee?" (evidence: "Smoky Mountains") |
| **A. Keyword (BM25) half of the search lost** (question has an apostrophe; exactly 3 results instead of 10) | 5 | `conv-26#71` "What book did Melanie read from Caroline's suggestion?"; `conv-44#109` "Audrey's dogs"; `conv-42#106` "Joanna's new screenplay"; `conv-26#107` "Melanie's reason" |
| **G. Evidence turn carries almost no text** (image-only turn, one-line reply) | 3 | `conv-26#1` "When did Melanie paint a sunrise?" (evidence turn is "take a look at this"; the content is in an image the dataset describes only by a caption that was not ingested); `conv-48#101` "I bought it a year ago in Paris." |
| **H. Wrong speaker**: the question names one person, the evidence is spoken by the other | 1 | `conv-48#179` "What did Jolene recently play that she described to Deb?" (said by Deborah) |
| Total | 48 | |

## 3. Population counts behind the sample (all 474 category 1-4 misses, full run)

| Fact | Count |
|---|---:|
| Questions that returned **exactly 3 results instead of 10** (apostrophe or colon in the question) | 238 of 1,536 (15.5%); 106 of the 474 misses |
| Recall@10 of those 238 questions vs the other 1,298 | 55.5% vs 71.6% |
| Misses with a top-10 turn within 1 turn of the evidence, same session | 121 (25.5%); within 2 turns: 238 (50.2%) |
| Misses whose evidence is at rank 11-50 of a 50-result probe | 199 (42.0%); not in the top 50: 275 |
| Misses with more than one evidence turn | 161 (34.0%) |

All-evidence recall@10 (every evidence turn present): single-hop 70.2%, multi-hop 14.5%, open-domain 20.7%, temporal 67.6%.
The multi-hop figure is the clearest structural limit of "one memory per turn, top-k turns back".

## 4. Measured emulations (4 conversations, paired; recall of any evidence turn)

Baseline on this subset, categories 1-4 (n = 582): R@1 38.8, R@3 55.8, R@5 60.7, R@10 67.0.

### 4.1 Proposal 1: stop losing the BM25 channel on punctuation

Cause: `TextIndex::search_projects` (`core/src/index/mod.rs`, `parser.parse_query(query)?`) hands the raw user text to
tantivy's query-language parser. `run_hybrid_search_scope` (`core/src/server/api.rs`) then calls
`text_search_projects(...).await.unwrap_or_default()`, so a parse error becomes "no keyword results" and nothing is logged.
With no keyword hits only the 3-slot vector-only fallback (`low_relevance_fallback = 3`) can fill the list, which is exactly
why those questions return 3 results. In the full LoCoMo run 330 of 1,986 requests returned 3 results; the same happens in
LongMemEval (17 of 100 requests, "I've", "What's"), where recall is unaffected at session level because those questions are long and the
vector half alone finds the session.

`probe_query_syntax.py` (`results/2026-11-10-v0.5.1-query-syntax-probe.json`) stores one memory containing a rare word and
searches with one-keyword queries (one keyword has no vector fallback, so a failure shows up as an empty result). The memory
is **not found** for: possessive `word's` and curly `word’s`, trailing `.`, leading `-`, `--`, `+`, `#`, `@`, `$`, a lone `(`
or `)`, `[word]`, `{word}`, `<word>`, a single `"`, `word~`, `word^2`, `word*`, `a=word`, and a trailing uppercase `AND` / `OR`.
It is found for `?`, `,`, `!`, `;`, `error: word`, `src/lib.rs:10 word`, `std::word`, `a/b word`, `a && word` and `"word"`.
(For two or more keywords the 3-slot fallback can rescue a result, so only single-keyword rows are a clean test. A `:` after
the first word of a longer question, as in `Caroline said: ...`, did break the keyword half: `query_as_stored_form` returned 3 results for 741 of 755 requests.)
For a coding-agent memory, flags, brackets, trailing periods and backticks are everyday input, not an English-possessive quirk.

Emulation (strip punctuation client-side, `syntax_safe` in `variants.py`):

| Subset | n | R@1 base | R@1 fixed | R@10 base | R@10 fixed | R@10 wins / losses |
|---|---:|---:|---:|---:|---:|---|
| Categories 1-4, question has an apostrophe | 104 | 34.6 | 38.5 | 53.8 | **63.5** | 18 / 8 |
| Categories 1-4, no apostrophe | 478 | 39.7 | 40.2 | 69.9 | 70.5 | 6 / 3 |
| Categories 1-4, all | 582 | 38.8 | 39.9 | 67.0 | **69.2** | 24 / 11 |
| All five categories | 755 | 38.1 | 38.9 | 65.4 | 68.3 | 34 / 12 |

Expected effect on LoCoMo categories 1-4: about +2 points R@10 (15.5% of questions affected, about +10 points on those).
Proposed change: build the keyword query with `QueryParser::parse_query_lenient`, or escape/strip syntax characters before parsing, and
`tracing::warn!` when the text channel errors instead of `unwrap_or_default()`. Small, no model or schema impact; needs a regression test
for `word's`, `-flag`, `(`, `"`.

### 4.2 Proposal 2: also search with the content words (second query, RRF)

Cause (D, F, part of the rest): the question is embedded and keyword-matched as typed, including "What did / How does / When did", which pull
towards generic question-like turns (`conv-26#3` "What did Caroline research?": top-1 is "Cool! What did it look like?"). The server already computes
`extract_keywords(query, 2)`. Emulation: a second request with the keyword string, fused with the server's own RRF constant (k = 60) client-side.
Two keyword lists were tried: the server's exact 35-word English stop list (`server_keywords`), and a broader list of 88 common function words (`content_words`).

| Variant (categories 1-4, n = 582) | R@1 | R@3 | R@5 | R@10 | R@10 wins / losses |
|---|---:|---:|---:|---:|---|
| base | 38.8 | 55.8 | 60.7 | 67.0 | - |
| RRF(base, server keywords) | 39.2 | 56.7 | 62.7 | **70.8** | 24 / 2 |
| RRF(base, punctuation fixed only) | 39.3 | 57.2 | 62.9 | 70.4 | 21 / 1 |
| RRF(base, broad content words) | **41.8** | 59.5 | 65.5 | **70.8** | 26 / 4 |
| broad content words alone | 44.0 | 58.9 | 64.3 | 70.4 | 38 / 18 |
| **no apostrophe only (n = 478):** base | 39.7 | 56.7 | 62.3 | 69.9 | - |
| no apostrophe: RRF(base, server keywords) | 39.7 | 56.7 | 62.6 | 70.7 | 5 / 1 |
| no apostrophe: RRF(base, broad content words) | **43.5** | 59.6 | 66.1 | **72.6** | 17 / 4 |

Reading: with the server's current stop list the second query adds almost nothing beyond fixing punctuation (R@10 70.4 to 70.8, R@1 +0);
most of that row's gain is proposal 1. The gain that is independent of the punctuation fix comes from a **broader stop-word list**
(no-apostrophe subset: R@1 +3.8, R@10 +2.7), i.e. removing words such as "did", "does", "have", "her", "about" from the keyword query.
Proposed change: extend `ENGLISH_STOPWORDS` (`core/src/search/mod.rs`), embed `keywords.join(" ")` as a second query vector in
`run_hybrid_search_scope` and add it as a third list to `crate::index::hybrid::rrf_fusion`. Cost: one extra query embedding per search (not measured).
English dialogue only: Korean and code queries need their own check, and a wider stop list could remove words that matter in code (`will`, `can`, `have`).

### 4.3 Proposal 3: neighbour context for hits (upper bound, not free)

Cause (B): 25% of the misses have a returned turn within one turn of the evidence. The product has neighbour plumbing
(`link_following_capture`, `transcript_neighbors`, `memory_get before/after`), but a search result only links the following assistant turn of a user turn.
Upper bound if each returned hit also exposed its +-1 neighbours (computed from the full run, categories 1-4, no new request):

| k | plain top-k | top-k plus +-1 neighbours of each hit (up to 3k turns of text) |
|---:|---:|---:|
| 1 | 39.7 | 47.1 |
| 3 | 55.4 | 65.0 |
| 5 | 60.7 | 70.4 |
| 10 | 69.1 | 77.0 |

This is **not** a like-for-like win: top-3 with neighbours (up to 9 turns of text) reaches 65.0, plain top-10 (10 turns) reaches 69.1.
The value is at the first positions (R@1 +7.4) when the agent reads only one or two hits. Ranked below proposals 1 and 2; only this arithmetic was done.

### 4.4 Candidate pool and `n_results`

* `n_results=50` (candidate pool 1,000 instead of 200) changes the first-10 list for few questions: 5 wins and 5 losses on 582 category 1-4 questions,
  R@10 identical (67.0). A bigger pool does not help; ranking, not the pool, is the limit. It also shows that `n_results` is part of the ranking:
  `conv-26#3` is rank 4 with `n_results=50` and absent with `n_results=10`, because the candidate pool and the MMR selection depend on `n_results`.
* The product default is `n_results=3` (HTTP and MCP). On category 1-4 R@3 is 55.4 vs 69.1 at ten. A separate `n_results=3` request returns the same R@1/R@3
  as the first three of a 10-result request within 0.3 points (R@3 56.0 vs 55.8). Raising the default is a token-cost decision for the product owner, not a retrieval fix.

### 4.5 What did not help or could not be measured

* Removing speaker names from the query: R@10 on all five categories 65.4 to 68.7 (56 wins, 31 losses) but categories 1-4 only 67.0 to 67.2 (28 / 27). LoCoMo-specific (two names repeated in every turn); not proposed.
* Putting the session date into the memory text (rerun with `--no-date`, same four conversations, categories 1-4): R@10 65.5 without vs 67.0 with the date; temporal questions 76.9 vs 80.0. Small; the date does not solve relative-time questions.
* `low_relevance_fallback`, `distance_cutoff`, MMR `lambda`, RRF `k` are constants in `core/src/config.rs` / `api.rs` and not reachable from the HTTP API: **not measured**. Every LoCoMo request that did not lose the keyword half returned a full 10 results, so the fallback budget was not the binding limit on this data.
* Embedding-input length and chunking were **not examined**: LoCoMo turns average 164 characters, LongMemEval turns average about 990 characters and the longest reach 31,000, and each turn is embedded as one piece.

## 5. LongMemEval-S sample (100 questions): where the 6 session-level misses come from

Session R@10 is 94.0%; six questions have no answer session in the top 10 turns' sessions:

* 4 of 6 are temporal-reasoning questions with relative time ("four weeks ago", "10 days ago", "a week ago", "two weeks ago") whose answer
  turn does not share the words of the question (for example "What kitchen appliance did I buy 10 days ago?" vs a turn about BBQ sauce that mentions the smoker in passing).
  The question's own date (`question_date`) is not sent by this harness and the server cannot resolve relative dates anyway.
* 2 of 6 are single-session-preference questions ("What should I serve for dinner this weekend with my homegrown ingredients?" with the evidence "harvested some cherry tomatoes"; a high-school-reunion question): the question never names what the answer depends on.
* Five of these six have the answer session at rank 11-50 of the 50-result probe; the sixth (the reunion question) returned only 3 results because of an apostrophe and is not in the top 50.
* Turn-level recall is lower than session-level recall (R@10 79% vs 94%; all `has_answer` turns present 54%), and multi-session questions need several sessions (all answer sessions in the top 10: 69%). Finding the session is easier than finding every supporting turn.

## 6. Ranked proposals

| Rank | Change | Where | Measured effect (LoCoMo, emulated client-side) | Risk / cost |
|---:|---|---|---|---|
| 1 | Make BM25 query parsing robust to punctuation; log text-channel errors | `TextIndex::search_projects` (`core/src/index/mod.rs`), `run_hybrid_search_scope` (`core/src/server/api.rs`) | cat 1-4 R@10 67.0 to 69.2 (subset), +9.7 on affected questions; 17% of LoCoMo and LongMemEval requests were affected | Very low; also fixes code-like queries |
| 2 | Second query from `extract_keywords` with a broader stop-word list, fused by RRF | `ENGLISH_STOPWORDS` in `core/src/search/mod.rs`, `run_hybrid_search_scope`, `rrf_fusion` | no-apostrophe cat 1-4: R@1 39.7 to 43.5, R@10 69.9 to 72.6; with the 35-word list only +0.8 R@10 | One more embedding per search; English-tuned, code/Korean unchecked |
| 3 | Return +-1 neighbour turns (or index adjacent turns as windows) | `link_following_capture`, `transcript_neighbors`, chunking in `core/src/watch.rs` | upper bound R@1 39.7 to 47.1; at equal text budget plain top-10 is better | More text per result; windowing needs re-index |
| 4 | Reconsider default `n_results` (3) | `default_n` in `api.rs`, MCP schema | R@3 55.4 / R@5 60.7 / R@10 69.1 | Token cost; product decision |

Status (2026-11-10): proposal 1 is implemented (`TextIndex::search_projects` retries a rejected query as plain words) and was measured with the real build: LoCoMo category 1-4 R@10 69.1 to 70.7 (45 wins, 21 losses), R@3 55.4 to 54.7; on the 17 affected LongMemEval sample questions session R@1 fell from 16 to 11 while R@3 to R@10 stayed at 16. See docs/benchmarks.md. Proposals 2-4 are not implemented.

Not addressable by retrieval alone: multi-turn answers (all-evidence R@10 14.5% on multi-hop), calendar arithmetic from relative dates, and world-knowledge questions.
These need the reader (an LLM or agent) to combine turns. Proposals 1-3 are measured by client-side emulation only; the same harness can measure the real implementation
(`python3 benchmarks/retrieval/run.py --binary <new build> --dataset locomo`).

## 7. Reproduce

```sh
python3 benchmarks/retrieval/analyze_misses.py benchmarks/retrieval/results/2026-11-10-v0.5.1-locomo.json --examples 3
python3 benchmarks/retrieval/sample_misses.py benchmarks/retrieval/results/2026-11-10-v0.5.1-locomo.json      # the 48 hand-classified misses
python3 benchmarks/retrieval/variants.py --binary ~/.local/bin/memnest                # section 4, ~15 min on an idle machine
python3 benchmarks/retrieval/variant_tables.py benchmarks/retrieval/results/2026-11-10-v0.5.1-locomo-variants.json
python3 benchmarks/retrieval/probe_query_syntax.py --binary ~/.local/bin/memnest --out benchmarks/retrieval/results/<date>-v0.5.1-query-syntax-probe.json
```

## 8. Ranking ablation and the change that was implemented (2026-11-11)

Sections 1-7 describe 0.5.1 and were written before any ranking code changed. This section measures the ranking itself.
The change is commits `42e7a66` and `1f20127`; the headline tables are in [docs/benchmarks.md](../../docs/benchmarks.md).

### 8.1 Method

* **Kept data.** Ingest is the slow part, so the data was ingested once with the released 0.5.1 binary into scratch data
  directories that stay on disk (`run.py --data-dir`), and every variant asked the same questions against copies of them
  (`--kept-only`). One variant takes about a minute for LoCoMo and seconds for LongMemEval instead of hours.
* **Tuning data and held-out data.** Everything in 8.2-8.5 uses the tuning data only: LoCoMo conversations 0-4 (760 scored
  category 1-4 questions) and the LongMemEval-S sample of docs/benchmarks.md (seed 1110, 100 questions). LoCoMo conversations
  5-9 and a second LongMemEval-S sample (seed 1111, 100 questions, none of them in the first sample) were not looked at
  until the change was committed (8.7 says exactly what was run on them and when).
* **Ablation build.** The develop commit `8c41b79` plus a patch that reads the ranking constants from a file in the scratch
  data directory, so one running instance serves every variant. That patch is not part of the repository and is not in any
  released or committed build. With no constants file the build ranks exactly like `8c41b79`.
* **Offline replica.** The same build can write, per question, both candidate lists with ranks and scores. A script re-ranks
  those lists with the server's formula. Against the real server it produced the identical 10-result list for 755 of 759
  LoCoMo questions (the 4 differences are exact score ties). It was used to search combinations (8.4); every number in
  the table of 8.2 and every headline number comes from a real binary answering `/search`.

All LoCoMo figures are any-evidence recall in percent, categories 1-4. LongMemEval figures are session recall in percent.
"wins/losses" are paired on the same questions against the first row: questions that gained a hit at that k / questions that lost one.

### 8.2 One factor at a time (tuning data, real binary)

| Variant (one factor changed) | LoCoMo R@1 | R@5 | R@10 | wins/losses @1 | @5 | LongMemEval session R@1 | R@5 | R@10 | wins/losses @1 | @5 |
|---|---:|---:|---:|---|---|---:|---:|---:|---|---|
| develop `8c41b79` (hybrid, RRF k=60, keyword bonus 0.15, MMR 0.5) | 39.1 | 62.2 | 70.7 | 0/0 | 0/0 | 78.0 | 91.0 | 94.0 | 0/0 | 0/0 |
| vector list only (no keyword list, no keyword bonus) | 40.7 | 67.9 | 76.1 | 92/80 | 112/69 | 88.0 | 94.0 | 95.0 | 13/3 | 4/1 |
| vector list only, MMR off | 40.7 | 67.4 | 75.7 | 92/80 | 112/73 | 88.0 | 94.0 | 95.0 | 13/3 | 4/1 |
| keyword (BM25) list only (no bonus, no vector-only fallback) | 33.9 | 56.1 | 63.2 | 39/78 | 28/75 | 85.0 | 90.0 | 94.0 | 9/2 | 2/3 |
| keyword list only, MMR off | 33.9 | 56.4 | 63.0 | 39/78 | 27/71 | 85.0 | 90.0 | 93.0 | 9/2 | 2/3 |
| RRF without the keyword bonus | 39.5 | 65.7 | 71.7 | 54/51 | 59/33 | 89.0 | 94.0 | 95.0 | 12/1 | 3/0 |
| RRF without the keyword bonus, MMR off | 39.5 | 65.7 | 71.3 | 54/51 | 59/33 | 89.0 | 94.0 | 95.0 | 12/1 | 3/0 |
| keyword bonus 0.30 | 39.1 | 60.9 | 69.6 | 1/1 | 2/12 | 78.0 | 90.0 | 94.0 | 0/0 | 0/1 |
| keyword bonus 0.05 | 40.4 | 64.6 | 73.6 | 13/3 | 23/5 | 83.0 | 94.0 | 96.0 | 6/1 | 3/0 |
| keyword bonus 0.02 | 41.4 | 66.1 | 73.0 | 26/8 | 39/10 | 87.0 | 94.0 | 95.0 | 10/1 | 3/0 |
| keyword bonus 0.01 | 41.7 | 65.7 | 72.6 | 31/11 | 46/20 | 88.0 | 94.0 | 95.0 | 11/1 | 3/0 |
| RRF k=10 | 43.2 | 67.4 | 76.7 | 43/12 | 51/12 | 88.0 | 93.0 | 94.0 | 12/2 | 3/1 |
| RRF k=20 | 41.7 | 65.9 | 74.7 | 24/4 | 35/7 | 86.0 | 93.0 | 95.0 | 9/1 | 3/1 |
| RRF k=120 | 38.9 | 60.7 | 68.2 | 1/2 | 5/17 | 78.0 | 90.0 | 93.0 | 0/0 | 0/1 |
| keyword list weight 0.5 | 39.7 | 63.2 | 71.2 | 6/1 | 9/2 | 78.0 | 91.0 | 94.0 | 0/0 | 0/0 |
| keyword list weight 0.25 | 40.3 | 63.6 | 72.1 | 11/2 | 15/5 | 78.0 | 91.0 | 94.0 | 0/0 | 0/0 |
| vector list weight 0.5 | 38.4 | 61.3 | 68.2 | 3/8 | 3/10 | 78.0 | 90.0 | 95.0 | 0/0 | 0/1 |
| candidate pool 5 x n_results (50) | 39.1 | 63.2 | 70.9 | 1/1 | 10/3 | 79.0 | 92.0 | 95.0 | 1/0 | 1/0 |
| candidate pool 10 x n_results (100) | 38.9 | 62.2 | 70.8 | 0/1 | 4/4 | 79.0 | 92.0 | 95.0 | 1/0 | 1/0 |
| candidate pool 50 x n_results (500) | 39.2 | 62.0 | 70.8 | 2/1 | 2/4 | 78.0 | 91.0 | 95.0 | 0/0 | 0/0 |
| MMR off | 39.2 | 61.3 | 69.5 | 1/0 | 4/11 | 78.0 | 92.0 | 94.0 | 0/0 | 1/0 |
| MMR lambda 0.3 | 39.1 | 61.2 | 69.3 | 0/0 | 7/15 | 78.0 | 91.0 | 96.0 | 0/0 | 0/0 |
| MMR lambda 0.7 | 39.1 | 61.8 | 70.5 | 1/1 | 2/5 | 78.0 | 92.0 | 94.0 | 0/0 | 1/0 |
| MMR lambda 0.9 | 39.1 | 61.8 | 69.9 | 1/1 | 5/8 | 78.0 | 92.0 | 95.0 | 0/0 | 1/0 |
| relevance order, skip near-duplicates (cosine >= 0.97) | 39.2 | 61.3 | 69.6 | 1/0 | 4/11 | 78.0 | 92.0 | 94.0 | 0/0 | 1/0 |
| relevance order, skip near-duplicates (cosine >= 0.93) | 39.1 | 58.6 | 65.7 | 1/1 | 11/39 | 78.0 | 92.0 | 94.0 | 0/0 | 1/0 |
| distance cutoff 0.15 | 38.9 | 62.2 | 70.4 | 0/1 | 0/0 | 78.0 | 91.0 | 94.0 | 0/0 | 0/0 |
| distance cutoff 1.0 | 39.1 | 62.2 | 70.7 | 1/1 | 0/0 | 78.0 | 91.0 | 94.0 | 0/0 | 0/0 |
| vector-only fallback slots 0 | 39.3 | 62.2 | 70.4 | 2/0 | 0/0 | 78.0 | 91.0 | 94.0 | 0/0 | 0/0 |
| vector-only fallback slots 10 | 39.1 | 62.2 | 70.7 | 0/0 | 0/0 | 78.0 | 91.0 | 94.0 | 0/0 | 0/0 |
| keyword query without the server's 35 stop words | 38.8 | 61.6 | 69.9 | 7/9 | 3/8 | 78.0 | 91.0 | 94.0 | 0/0 | 0/0 |
| keyword query without stop words and conversational fillers | 39.7 | 62.1 | 70.5 | 9/4 | 7/8 | 78.0 | 92.0 | 94.0 | 0/0 | 1/0 |
| keyword query without a broad 88-word stop list | 39.5 | 63.3 | 71.4 | 10/7 | 14/6 | 79.0 | 92.0 | 94.0 | 1/0 | 1/0 |
| keyword bonus counts topic words only (no fillers) | 40.9 | 64.2 | 72.0 | 19/5 | 20/5 | 80.0 | 93.0 | 95.0 | 3/1 | 2/0 |
| keyword bonus counts content words only (broad stop list) | 43.0 | 65.7 | 72.2 | 40/10 | 38/12 | 80.0 | 92.0 | 95.0 | 5/3 | 2/1 |
| keyword bonus needs whole-word matches | 39.2 | 61.6 | 70.9 | 27/26 | 18/23 | 78.0 | 93.0 | 95.0 | 2/2 | 3/1 |
| keyword bonus needs a match at the start of a word | 40.1 | 63.4 | 71.1 | 11/3 | 14/5 | 79.0 | 92.0 | 95.0 | 1/0 | 1/0 |
| English stemming in the text index (index rebuilt) | 39.9 | 63.3 | 70.9 | 10/4 | 14/6 | 78.0 | 92.0 | 94.0 | 0/0 | 1/0 |
| English stemming and broad stop list in the keyword query | 40.5 | 65.1 | 73.3 | 19/8 | 29/7 | 79.0 | 92.0 | 95.0 | 1/0 | 1/0 |

Reading:

* **The vector list alone beats the hybrid on both datasets**, and the keyword list alone is the weakest on LoCoMo. On LoCoMo
  the answer is in the top 10 of the vector list for 75.6% of questions, of the keyword list for 63.0%, and of at least one
  of them for 83.0%: the keyword list adds something, but equal weights let the weaker list pull the result down.
* **The keyword bonus was the largest single problem.** The fused score was at most 0.033 (two lists, `1/(60+rank)`), the
  bonus for the share of question words found in a memory reached 0.15. So that share decided the order almost alone, and the
  two retrievers only broke ties. Lowering the bonus or removing it helps on both datasets; raising it hurts.
* **RRF with k=60 is nearly flat** over a 200-candidate pool (rank 0 gives 0.0167, rank 10 gives 0.0143). k=10 separates the
  first ranks from the rest: LoCoMo R@10 +6.0 with nothing else changed.
* MMR (any lambda, off, or a near-duplicate threshold), the candidate pool, the distance cutoff and the number of vector-only
  fallback slots move each figure by at most about one point here, in both directions. The near-duplicate threshold 0.93 is
  clearly worse on LoCoMo: turns of one conversation sit closer than that to each other.
* Dropping stop words from the keyword query, counting only content words for the bonus, and English stemming each help
  LoCoMo a little while the bonus still dominates; 8.5 shows what is left of them after the main change.

### 8.3 The five LongMemEval questions that lost rank 1 after the keyword-query fix

For each one the answer session was first with 0.5.1 (vector results only, because the keyword half was lost) and second or
third with develop. What outranked it, in every case, was a long assistant turn (1,900 to 2,600 characters) from another
session that contains more of the question's words as substrings: the question about a NAS device lost to a turn about buying
a laptop that matched `whether`, `buy`, `device`, `you`, `think`; the bedtime question lost to two long turns matching
`time`, `go`, `bed`, `day`, `had`. Matching is by substring, so `go` is found in `going`, `Red` in `preferred`, `ve` in
`have`. The answer turns were short (140 to 360 characters), matched fewer words, and the 0.15 bonus outweighed their first
place in the vector list. With the change below all five are first again.

### 8.4 What was implemented (commits `42e7a66`, `1f20127`)

`core/src/index/hybrid.rs` (`rrf_fusion`), `core/src/config.rs`, and the score line in `hybrid_search_embedded` (`core/src/server/api.rs`):

| | before | after |
|---|---|---|
| Fusion | `1/(60+rank)` per list, equal weight, at most 0.033 | `10/(10+rank)` per list, keyword list weight 0.5, scaled to 0..1 (1 = first in both lists) |
| Final score | fused + 0.15 x keyword ratio + importance + type - age | 0.10 x fused + 0.08 x keyword ratio + importance + type - age |
| A memory containing none of the question's keywords | fused part at most 0.033 | fused part capped at 0.049 (8.6) |

The keyword ratio itself, the candidate pools, the distance cutoff, the fallback slots, MMR and the importance, type and age
terms are unchanged. The two maxima (0.10 and 0.08) add up to what fusion plus bonus could reach before (0.183). What changes
is that a memory ranked first by the retrievers can now outscore one that only repeats more of the question's words.

Choosing the constants: the replica was run over k in 5, 10, 15, 20, keyword-list weight in 0.3, 0.5, 0.75, 1.0 and
bonus/fused in 0.5, 0.75, 1.0, 1.5. Everything with k 5-15, weight 0.3-0.5 and bonus/fused 0.5-1.0 is within about one point on
LoCoMo (R@1 43.5-45.1, R@5 69.7-72.1, R@10 78.3-79.4) and within two questions on LongMemEval (R@1 86-91, R@5 95-96).
The values were taken from the middle of that region rather than from its best cell: k=10, weight 0.5, bonus/fused 0.8.

Result on the tuning data (real binaries, each started on a copy of the same kept data; LoCoMo conversations 0-4,
LongMemEval sample seed 1110). The 0.5.1 row differs a little from sections 1-7, where the questions were asked in the
process that had just stored the turns: after a restart the same build orders the results differently for 869 of 1,986
LoCoMo questions and its figures move by up to 0.3 points. The cause was not isolated; all rows here are after a restart:

| | LoCoMo R@1 | R@3 | R@5 | R@10 | LongMemEval session R@1 | R@3 | R@5 | R@10 | turn R@10 |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| released 0.5.1 | 39.2 | 56.6 | 61.8 | 68.8 | 83 | 89 | 91 | 94 | 79 |
| develop `8c41b79` | 39.1 | 56.2 | 62.2 | 70.7 | 78 | 89 | 91 | 94 | 79 |
| `42e7a66` (no cap) | 44.3 | 63.8 | 70.3 | 79.1 | 89 | 94 | 95 | 95 | 91 |
| `1f20127` (with the cap; the final build) | 44.3 | 63.7 | 70.1 | 79.1 | 89 | 93 | 94 | 95 | 92 |
| final build, wins/losses against develop | 56/16 | 79/22 | 79/19 | 78/14 | 12/1 | 4/0 | 3/0 | 1/0 | |
| final build, wins/losses against 0.5.1 | 56/17 | 82/28 | 85/22 | 90/12 | 7/1 | 4/0 | 3/0 | 1/0 | |

With the product default `n_results=3` instead of 10 (LoCoMo conversations 0-4, develop against the final build): R@1 39.1 to 44.3, R@3 56.4 to 63.2.

### 8.5 Measured and not adopted

Effect on top of `42e7a66` (replica, tuning data, before the cap of 8.6, which moves these figures by at most 0.2 points
on LoCoMo and one question on LongMemEval; LoCoMo R@1 / R@5 / R@10, then LongMemEval R@1 / R@5 / R@10):

| Addition | LoCoMo | LongMemEval | Why not |
|---|---|---|---|
| (`42e7a66`) | 44.3 / 70.2 / 79.1 | 89 / 95 / 95 | |
| English stemming in the text index | 46.0 / 71.1 / 79.6 | not measured in combination; alone 78 / 92 / 94 against 78 / 91 / 94 | Every existing text index would have to be rebuilt on upgrade, and the effect on Korean and on code identifiers is unmeasured. About one point |
| Keyword query without an 88-word stop list | 45.1 / 71.9 / 79.4 | 89 / 95 / 96 | English-only list; it contains words that matter in code (`will`, `can`, `have`). About one point |
| Bonus counts topic words only (no fillers) | 45.3 / 71.0 / 79.6 | 90 / 95 / 95 | Within noise |
| Bonus needs whole-word matches | 42.8 / 70.6 / 79.3 | 89 / 95 / 95 | Worse at rank 1 on LoCoMo (`paint` no longer matches `painted`) |
| MMR off / lambda 0.7 / lambda 0.3 | 44.3 / 69.6 / 78.8, 44.3 / 70.1 / 79.2, 44.3 / 71.8 / 79.6 | 89 / 95 / 95, 89 / 95 / 95, 89 / 95 / 96 | No consistent gain; MMR 0.5 was set for near-duplicate saved memories, which these datasets do not contain |
| Candidate pool 100 or 50 instead of 200 | 44.3 / 70.0 / 79.1, 44.3 / 69.8 / 79.1 | 89 / 95 / 95 | No effect |
| Vector-only fallback slots 0 / 10 instead of 3 | 44.1 / 68.6 / 75.6, 44.3 / 70.2 / 78.9 | 89 / 94 / 94, 89 / 95 / 95 | The 3 slots are needed; more do not help |
| No keyword bonus at all | 43.0 / 69.7 / 78.8 | 91 / 95 / 95 | The bonus is the only keyword signal for Korean words with particles (the text index stores `포트는` as one token and does not match `포트`) |

Not measured: a different embedding model (needs every memory embedded again: about 2.5 hours per 49,000 LongMemEval turns
on this machine) and a different tokenizer for Korean.

### 8.6 The score is also read as an absolute value: the cap, and a check on a real store

The datasets only see the order of results. Clients also compare the score with fixed numbers, and every memory in the datasets
has the same type, importance and age, so they say nothing about how relevance should weigh against those. Both were checked
separately after `42e7a66`.

**What reads the score.**

| Where | Rule | After the change |
|---|---|---|
| pi extension automatic context (`pi-extension/src/autocontext.ts`) | injects a saved memory when its score is 0.25 or more. A fresh saved memory starts at 0.20 (manual + knowledge), so this means "the relevance part is at least 0.05" | **Broke with `42e7a66`**, fixed in `1f20127` (below) |
| `earlier_rules` for a new task card (`core/src/server/task_cards.rs`) | keeps results with a score above 0 | 9 real cards: same number of kept results and same home project on develop and the final build |
| cross-project hint (`HINT_SCORE_MARGIN` 0.05, `core/src/server/operations.rs`) | an outside saved memory must beat the best on-topic scoped score by 0.05 | 5 phrasings of a real question whose answer is in another project: the hint names that project on develop and on the final build, 5 of 5 |
| MCP and HTTP output, pi tool output | print the score | no rule attached |

**The injection gate.** Before, the rank could add at most 0.033, so a memory containing none of the question's words could
never reach 0.25. With `42e7a66` the rank adds up to 0.10. Scratch check with 12 saved memories and 3 questions about
something else (lunch, CSS, Kubernetes): results at 0.25 or more were 0 of 9 on develop and 9 of 9 on `42e7a66`. In real use
that injects the nearest fresh saved memory into prompts it has nothing to do with. `1f20127` caps the rank part at 0.049 for
a memory in which none of the question's keywords occurs: 0 of 9 again, and a test in `core/src/server/api.rs`
(`a_memory_without_any_question_keyword_stays_below_the_injection_score`, Korean and English) fails if this is ever undone.
The right memory still passes: 12 of 13 related questions on develop and on the final build.

**How large relevance should be.** Two ways to add the cap were built: scale 0.07 / 0.056 ("small"), which keeps the order of
`42e7a66` on the datasets exactly, and scale 0.10 / 0.08 ("kept"), which costs LoCoMo R@5 0.2 points and one LongMemEval
question at R@3 and R@5 (its answer turns share no word with the question; they are first and second in the vector list and are
now held below turns that do share words). They were compared on a read-only snapshot of a real store in daily use (55,251
memories: 4,961 saved by hand, the rest transcript turns; Korean and English mixed), restored into a scratch directory, served
by scratch instances and deleted afterwards. The live service was not touched.

*Known-item search.* 40 saved memories were drawn at random, 10 per age group (under 7 days, 7-30, 30-90, over 90), at most
3 per project. For each one a question a user might ask was written before any build was run (28 Korean or mixed, 12 English,
not copying the memory's sentences). Each question was asked with `project="all"`, and for the 22 memories that live in the
default workspace scope also without a project. `n_results=10`. Number of questions whose memory came back at rank 1 / in the
first 3 / 5 / 10, and MRR:

| Build | all projects (n=40) | default scope (n=22) | older than 30 days, all projects (n=20) | older than 30 days, default scope (n=12) |
|---|---|---|---|---|
| released 0.5.1 | 13 / 24 / 28 / 30, 0.478 | 4 / 6 / 11 / 15, 0.296 | 6 / 12 / 15 / 15, 0.468 | 1 / 3 / 5 / 6, 0.198 |
| develop `8c41b79` | 13 / 24 / 28 / 30, 0.478 | 4 / 6 / 11 / 15, 0.296 | 6 / 12 / 15 / 15, 0.468 | 1 / 3 / 5 / 6, 0.198 |
| small scale | 13 / 24 / 29 / 31, 0.492 | 4 / 8 / 12 / 16, 0.339 | 7 / 11 / 14 / 16, 0.504 | 1 / 4 / 5 / 7, 0.247 |
| kept scale (`1f20127`) | 16 / 26 / 31 / 31, 0.528 | 5 / 10 / 13 / 17, 0.388 | 8 / 13 / 16 / 16, 0.524 | 2 / 4 / 6 / 8, 0.307 |

Against develop the kept scale moved the memory up for 17 question/scope pairs and down for 3 (each by one place); the small
scale 10 up, 4 down.

*Real prompts.* 20 specific user prompts from the transcripts were searched in their own workspace scope (first 3 results,
the prompt's own record removed). Where the first 3 differed from develop's, the two lists were judged without knowing which
build produced which. Kept scale: 13 identical, 2 better, 3 equal, 2 worse. Small scale: 12 identical, 3 better, 5 worse; its
losses were weakly related saved memories pushed above transcript turns, because relevance had become small against the bonus
a saved memory gets.

*Injection gate on the real copy* (the extension's request: saved memories only, 20 results, first 2 at 0.25 or more). 63 real
prompts: nothing injected by any build. The 40 questions above in the default scope: develop injected 4 memories, 1 of them the
one asked for; the final build injected 1, the one asked for.

So `1f20127` keeps 0.10 / 0.08. The limits of this check: one store, 40 questions, one judge. Differences of one to three
questions are not significant. What it does show is that there is no sign of older or less important memories being buried by
the new ranking, and that shrinking relevance further has a visible cost.

### 8.7 Held-out confirmation

What was run on the held-out data, in order:

1. LoCoMo conversations 5-9 with `42e7a66` (the full 10-conversation run, split afterwards).
2. After that run the injection problem of 8.6 was found. The cap and the scale were decided on the scratch check, the
   tuning data and the real-store copy, not on held-out data. The full LoCoMo run was then repeated with the final build
   `1f20127`. So LoCoMo's held-out half was measured twice; the two results differ only in all-evidence R@10 (62.8, then 62.6).
3. The second LongMemEval-S sample (seed 1111) was ingested while the work went on and asked once, after `1f20127` was
   committed: with the final build, with 0.5.1 and with develop, all on the same kept data.
4. For timing, the full LoCoMo run was repeated with 0.5.1, develop and the final build on a quiet machine. The final
   build returned the identical ranking for all 1,986 questions; develop for 1,980. The committed result files and the
   rows below are these runs.

LoCoMo conversations 5-9, categories 1-4 (n = 776), recall in percent:

| Build | R@1 | R@3 | R@5 | R@10 | all-evidence R@10 | R@50 (probe) | MRR@10 |
|---|---:|---:|---:|---:|---:|---:|---:|
| released 0.5.1 | 39.8 | 54.5 | 60.2 | 69.5 | 55.2 | 82.5 | 0.489 |
| develop `8c41b79` | 40.2 | 53.5 | 60.1 | 70.6 | 56.7 | 87.4 | 0.491 |
| `42e7a66` (first run) | 42.9 | 62.0 | 68.7 | 76.8 | 62.8 | 91.6 | 0.541 |
| `1f20127` (final build) | 42.9 | 62.0 | 68.7 | 76.8 | 62.6 | 91.6 | 0.541 |
| final build, wins/losses against 0.5.1 | 55/31 | 86/28 | 93/27 | 76/19 | | | |
| final build, wins/losses against develop | 52/31 | 86/20 | 88/21 | 66/18 | | | |

The gain is smaller here than on the tuned half: against 0.5.1, R@1 +3.1 (tuned half +5.1), R@10 +7.3 (+10.3); R@5 +8.5 (+8.3).

Second LongMemEval-S sample (n = 100), session recall in percent:

| Build | R@1 | R@3 | R@5 | R@10 | turn R@10 (any) |
|---|---:|---:|---:|---:|---:|
| released 0.5.1 | 83 | 93 | 96 | 98 | 84 |
| develop `8c41b79` | 83 | 93 | 96 | 98 | 83 |
| `1f20127` (final build) | 94 | 98 | 98 | 99 | 93 |
| final build, wins/losses against 0.5.1 | 11/0 | 5/0 | 2/0 | 1/0 | |

This sample is easier than the first one for every build (0.5.1 R@5 96 here, 91 there), which is what two samples of 100
look like: the approximate 95% interval of 98 of 100 is 93.0 to 99.4. Over both samples (200 questions) the final build has
session R@1 91.5, R@5 96.0, R@10 97.0; 0.5.1 has 83.0, 93.5, 96.0.

**Category 5 (adversarial) got worse at the first rank.** These questions are not in the scored set (they ask about things
the named speaker never said), but their labelled evidence turn is measured the same way. All 10 conversations, n = 446:
R@1 30.9 (0.5.1) and 32.5 (develop) to 26.9; R@3 46.6 and 48.9 to 46.4; R@5 52.7 and 56.3 to 55.4; R@10 58.1 and 63.0 to
66.6. Against develop that is 9 questions gained and 34 lost at rank 1, 30 gained and 14 lost in the first 10. The cause was
not investigated.

### 8.8 What ranking alone does not reach

* **LongMemEval session R@5.** In the tuning sample the answer session is among the first five sessions of the vector list for
  94 of 100 questions, of the keyword list for 90, and of either for 94. The questions still missed at R@5 are temporal-reasoning
  questions with a relative date ("two weeks ago", "a week ago", "10 days ago") and two preference questions; their best
  answer turn is at rank 11 to 39 in both lists, or shares no word with the question. Ordering sessions by all their turns
  among the first 50 results instead of the first 10 (computed offline on `42e7a66`, not a product change) leaves R@5 at 95-96
  and lifts R@10 from 95 to 97-98. Getting further needs session-level or neighbour-window indexing, or resolving relative
  dates from the question date, not another ranking constant.
* **LoCoMo.** With the final build the evidence is in the first 10 for 77.9% of category 1-4 questions and in the first 50 for
  91.1%. Multi-hop questions need several turns at once (all-evidence R@10 20.9%), and section 2's causes B, C, E and I
  (neighbouring turn, date arithmetic, multi-turn answers, world knowledge) are untouched by ranking.
