# Why retrieval misses: analysis and measured emulations (memnest 0.5.1)

Scope: why questions are missed in `results/2026-11-10-v0.5.1-locomo.json` (full LoCoMo, 1,982 questions with
evidence) and `results/2026-11-10-v0.5.1-longmemeval-sample100.json` (100-question sample), and what small
changes to memnest's search would help. The headline tables are in [docs/benchmarks.md](../../docs/benchmarks.md).
**No product code was changed.** Every number below was produced by scripts in this directory
(`analyze_misses.py`, `sample_misses.py`, `variants.py`, `variant_tables.py`, `probe_query_syntax.py`) on binary 0.5.1.

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
