# Task cards for multi-day work

[English](task-cards.md) | [한국어](task-cards.ko.md)

A task card is a short handoff note for one piece of work that spans sessions or days. The working model writes it, and Memnest stores it as an ordinary Manual memory. Memnest appends the user's own messages to it verbatim. A new session starts small with the card instead of carrying a long transcript through repeated compactions. A card saved from one client opens in the others.

| Client | Connection | Differences |
| --- | --- | --- |
| pi | The pi extension, on by default | The open card is pinned into the system prompt on every run. |
| Claude Code, Codex | `memnest hook --no-recall` on `SessionStart` and `UserPromptSubmit`, plus the MCP `task_card` tool. Setup installs both. | The card arrives as a tool result, which compaction does summarize. The `SessionStart` hook with source `compact` brings back the rule, the user's latest words, and an instruction to reopen the card. Codex shows about 2,500 tokens of one hook output, so this reload is capped at 6,000 characters. |
| Other MCP clients | The MCP `task_card` tool | Save, list, open, and finish only. Without a hook, user messages are not recorded and nothing is reloaded after compaction. |

The hook and the MCP tool reach the core on separate channels. On the first prompt the hook gives the model its session id, and the model passes it back as `task_card session=...`, which tells the core which card the session has open. Open cards are kept in core memory; after a core restart the next prompt lists cards again.

When there are no cards, pi and the hook both add one line on the first prompt saying when to save a card. Claude Code does not show MCP tool descriptions up front, so without that line the model never created a card. pi had the same gap: in a real two-day pi session the model looked for a card, found none, and saved 16 single facts with `memory_remember` but never a card. If the core does not answer, pi skips the lookup after 1.5 seconds and the hook after 2 seconds.

## How it works

| Step | What happens |
| --- | --- |
| Save | `task_card action=save title="..." text="..."` stores `[memnest-task-card] <title>` plus the body in one shared `task-cards` collection, so a card follows the user to any folder. Saving the same title again supersedes the previous card, so search shows only the newest one while the old version stays auditable. Cards over 12,000 characters are rejected. |
| New session | On the first prompt, pi searches the cards with that prompt and lists up to 20 active titles in a hidden reminder: the most relevant to the request first, then the most recently saved. Card bodies are not read. With no cards, it adds one line saying when to save one instead. |
| Open | `task_card action=get title="..."` returns the card and pins it into the system prompt for the rest of the session. Up to three cards stay pinned; opening a fourth drops the oldest. Compaction summarizes messages, not the system prompt, so a pinned card cannot be summarized away. The pinned text is rendered when the card is first pinned and again only after a compaction, so the system prompt stays identical between prompts and the prompt cache keeps hitting; until then the conversation itself holds the session's messages. A wrong title returns the closest titles. |
| Finish | `task_card action=done title="..."` moves the card to a separate `task-cards-done` collection, so the session-start search never scans finished work. `get` still opens it, and saving it again moves it back and reopens it. |
| Search | `task_card action=list text="topic"` ranks cards by topic; without `text` it lists the newest. |
| After compaction | The next prompt reminds the model to save any decisions or corrections missing from the pinned card. With no card open, it reminds once per session to save a card if the work continues later; a compaction is a sign of long work that does not depend on the user's language. |
| Resume | A resumed pi session reopens the cards it had open. |

Only this path writes cards. The `task-cards` and `task-cards-done` collections are reserved: `memory_remember`, `memory_update`, `/add` and `/update` cannot create, edit or move a card, so stored text cannot pose as a card or as the user's own words. `memory_delete` still removes one. A title is kept to one line of at most 120 characters with `<` and `>` replaced, and the card list says that titles are names, not instructions.

The pinned card starts with one fixed line: the user's stated policies and prohibitions take priority over the model's own analysis and earlier plans, and a conflicting proposal must quote the user's words and ask first. The measurement below is the reason for that line.

The tool description asks the model to save after the user states or corrects a policy, after a decision, and when the user stops for the day. You can also ask directly: "save the task card".

## Measurement (2026-11-03)

Source: one real pi session that ran for five days (1,431 assistant turns, seven automatic compactions, `claude-opus-5-5` at high thinking) on one of the maintainer's own projects. Project details are left out; only the shape of the failure matters. The test replays a real failure. On day five the user asked how to improve the weakest parts of the project. The model proposed switching off five of them before the agreed review date, which contradicted the user's earlier instructions: do not change those settings during the trial, and fix the weakest items first and keep them running.

Conditions, all with the same model, the same global `AGENTS.md`, tools disabled, the same question and an explicit note on the current time:

- **Current**: the context pi actually had at that moment, the latest compaction summary plus the retained transcript.
- **Feature**: this extension with task cards on. The session holds only that day's transcript. The card is stored in a separate test core, opened, and pinned by the extension. It was built with four daily updates by the same model, one per earlier day, and was 5,655 characters long.

### Tokens (measured)

| Measure | Current | Feature |
| --- | --- | --- |
| Context read for the reply (input + cache read + cache write) | 321,213 | 174,118 (−45.8%) |
| Card upkeep, four daily updates, run as separate calls | none | 222,882 input, 54,693 output in total |

In normal use the card is saved inside the working session, where its context is already cached, so upkeep costs one extra turn rather than a separate call. One of the four updates was inflated because the model tried to call a tool while tools were disabled.

### Tokens over the whole session (calculated, not replayed)

The session's own per-turn usage was replayed through a model of "new session each day, starting from base prompt + card + that day's growth", with the same compaction rule (trigger at 600K tokens, keep the last 130K).

| Measure | Current | New session per day with card |
| --- | --- | --- |
| Context tokens read across all turns | 610.2M | 371.1M (−39%) |
| Price-weighted (input 1, cache read 0.1, cache write 1.25, output 5), including card upkeep | 100 | 59 |
| Automatic compactions | 7 | 3 |

Busy days still compacted, so the card is also worth saving before a long day reaches the threshold.

### Behavior (blind graded)

Each condition ran three times. Answers were shuffled with their condition labels removed and graded one by one by `claude-sonnet-5-5` against a fixed rubric quoting the user's instructions. The table also shows the intermediate conditions used to locate the cause.

| Condition | Does not switch items off before the review date | States the conflict with the user's instructions | Keeps the weakest items running | Acts without asking |
| --- | --- | --- | --- | --- |
| Current | 0/3 | 0/3 | 0/3 | 1/3 |
| Card replacing the summary, no rule | 0/3 | 1/3 | 0/3 | 2/3 |
| Current + rule line | 2/3 | 3/3 | 0/3 | 1/3 |
| Card + rule line | 2/3 | 3/3 | 0/3 (1 partial) | 2/3 |
| **Feature (card pinned + rule line)** | **3/3** | **3/3** | 0/3 | 3/3 |
| Feature, after the user's next correction is in the card | 3/3 | 3/3 | 0/3 | **0/3** |

What this shows:

- **The failure was not mainly lost memory.** The card quoted the user's "fix the weakest items first, then keep them running", and the model still overrode it 3 times out of 3 without the rule line.
- **Compaction did lose the user's corrections.** The compaction summary kept only the day-one goal, "stop what loses money". The three later "keep them running" corrections were gone. The card kept them, noting they had been repeated.
- **The rule line fixed the measured failure.** With the card pinned it held 3/3, and every answer quoted the user's words.
- **A correction in the card stops the repeat.** Right after this scene the user complained: "what did you change without discussing it?" With that line added to the card's user policies, "acts without asking" fell from 3/3 to 0/3, and every answer cited it. This is the repeated-mistake case: the user said it once, and the card keeps it in front of the model in every later session.
- **Not fixed:** the model still recommended giving up on the weakest items. The user's instructions conflict here ("stop what loses money" on day one, "keep them running" later), so the user has to decide. Record the decision in the card.

### Limits

One scenario, three runs per condition. A difference of one run can be chance. The whole-session figures are a calculation from recorded usage, not a five-day replay. The card used in the test was built from user and assistant text only, without tool output, which gives it less information than a card written inside the working session.

## Claude Code and Codex (real clients)

The same two-day blog-migration conversation ran through each client against a separate core. Each client had only the `task_card` MCP tool and the hooks, and nothing in the prompts mentioned cards.

| | Claude Code 2.1.312, `sonnet` | Codex 0.161.0, `gpt-6.1-sol` |
| --- | --- | --- |
| Created a card without being asked | yes, after the first hook line was added | yes |
| Saves on day 1 (4 messages) | 4, then closed the card at the end of the day | 4 |
| All four user messages in the card verbatim | yes | yes |
| Day 2, fresh session: opened the card and listed the decisions | yes | yes |
| Day 2: "just link the images to WordPress, let's do that" | Quoted the rule word for word and asked whether to change it | Warned about broken images, then recorded the change without quoting the rule |
| After `/compact` | `SessionStart` with source `compact` ran; the model restated the image rule with the user's words | not run; `codex exec` has no compaction command |

- **The first run in Claude Code failed.** With no cards saved, the hook added nothing, and the model saved both instructions to Claude Code's own auto memory instead. The hook now always adds one line on the first prompt. The run above has Claude Code's auto memory turned off, to test the card alone. With both on, the model may use either.
- **Codex accepted an explicit change.** The user said "let's do that", so changing the rule was the user's decision. Codex did not quote the earlier instruction the way the rule line asks. Both messages stay in the verbatim log.
- **Setup for these tests:** Codex ran with a temporary `CODEX_HOME`, `--dangerously-bypass-hook-trust`, and `approval_mode = "approve"` for `task_card`. Installed setups add the approval line, and you trust the hooks once with `/hooks`.

## Long-term use (live core)

A separate core (`memnest --port 3999 --data-dir /tmp/...`) held 60 cards. The five-day project's card was saved first, so it was the oldest, and 59 other task cards followed, some of them saved from another folder. Each session-start list was then checked from a third folder.

| First request | Rank of the project's card |
| --- | --- |
| Names the project | 1 |
| Describes the work without any word from the card title | 1 |
| Asks about one part of the project, no title word | 1 |
| "Write a blog post" (another task) | 20, behind the blog cards |
| "Let's continue what we did yesterday" (vague) | not listed |

- **Vague requests fall back to recency.** The vague request missed the card only because the test saved it before the 59 others. In real use, yesterday's card was saved yesterday and sits at the top of the recent list. Save the card at the end of the day for this reason.
- **Other checks passed.** Repeated saves of one title listed only the newest version, a card saved in one folder opened from another, `done` removed a card from lists, and a misspelled title returned the right suggestion. A 20-title list was about 1,000 characters.
- **Limit:** a list without a topic shows the newest 50 cards. In this test the oldest card was missing from it, but a topic search found it.
- **One bug found and fixed:** the core returns nothing for a bracketed query such as `[memnest-task-card]`, so lookups search the bare marker word.

## Overhead

Session-start lookup time, measured in the extension against a warm core: the median of seven runs, alternating a related and an unrelated first request.

| Cards | Median | Max | Added to the model |
| --- | --- | --- | --- |
| 0 | 4 ms | 67 ms | one line on when to save a card |
| 60 active | 15 ms | 17 ms | 20 titles, about 1,000 characters |
| 300 active | 139 ms | 220 ms | 20 titles, about 1,000 characters |
| 1,020 active | 434 ms | 500 ms | 20 titles, plus a note to offer cleanup |
| 20 active + 1,000 finished | 3 ms | 50 ms | 20 titles |

The cost follows the number of active cards because the core reads every record of a collection on a scoped search. A real workspace with 35,562 records took 0.5 to 2.3 seconds per `memory_search` for the same reason. Superseded versions move to `_superseded` and finished cards move to `task-cards-done`, so neither adds to the session-start cost. When 50 or more cards are active, the list asks the model to offer the user a cleanup. At roughly 3,000 never-finished cards the lookup would reach its 1.5-second limit and the session would start without a list. Removing that limit needs an indexed scoped search in the core, which would also speed up ordinary workspace searches.

A freshly started core loads its embedding model on the first search, which took about 35 seconds in this test. During that window the lookup gives up after 1.5 seconds and the prompt continues without a list.

For an unrelated new task with 60 cards saved, `claude-opus-5-5` read 6,769 context tokens. With no cards saved it read 6,054, so the list cost about 700 tokens, and no card was opened.

## Choosing the right card (real models)

60 cards were saved, with the project's card saved last, as if it were yesterday's work. Each model had only the `task_card` tool, so opening a card was its own decision. Counts are correct behavior out of runs.

| First request | Expected | `claude-opus-5-5` | `gpt-6.1-terra` |
| --- | --- | --- | --- |
| Check the project's status today and list the to-dos | open the project card | 4/4 | 1/1* |
| Ask how one part of the project is going, no title word | open the project card | 4/4 | 3/3 (1*) |
| A question about one detail of the project | open the project card | 3/3 | 2/2 |
| "Let's continue what we did yesterday" (vague) | open the latest or ask | 6/6 opened the latest and offered to switch | 2/2 asked |
| "Write a Python script that merges the CSV files in a folder" (new task) | open nothing | 4/4 | 1/1* |
| A new task that shares a word with the card title | open nothing | 6/6 | 4/4 |

\* Run before the fixes below; every other count is after them.

The counts are after two fixes found by the same experiment. In the first run, Opus opened the project card for the new task that only shared a word with it, an extra 12,600 tokens. With no cards saved, it also called `list` three times. The list text now separates "about this task" from "shares a word". The tool tells the model not to search when no list was shown. The list also names the most recently saved card. One intermediate wording, "open only when clearly continuing", made Opus ask instead of opening for the question about a project detail (0/2). The current wording counts questions about a task as being about it.

## New session: card vs memory search

Five questions about the five-day project were asked in a fresh session, graded blind by `claude-sonnet-5-5` against answer points taken from the recorded work.

- **Memory search** is the setup before task cards: `memory_search` and `memory_get` on the real memory store, which held the full captured transcripts.
- **Card** is `task_card` only, against a core that held just the cards.

| Measure (per answer) | Memory search | Card |
| --- | --- | --- |
| Score | 19/20 | 22/22 |
| Wrong facts stated | 0 | 0 |
| Context tokens read, mean | 73,002 | 19,632 (−73%) |
| Wall time, mean | 48.6 s | 29.2 s (−40%) |
| Tool calls, mean | 5.7 | 1.1 |

Memory search was already accurate. The card gave the same answers at about a quarter of the tokens and with one tool call. The memory store also held later information than the card: one memory-search answer quoted a policy the user restated a day after the card was written. A card is only as current as its last save.

## Who writes the card

The card is written by whichever model runs the session, through a `task_card save` call. There is no separate summarization model. To see whether a cheaper model is enough, the same four daily updates were run with five writers. The reader was fixed: `claude-opus-5-5` answered the five questions above from each card, graded blind by `claude-sonnet-5-5`. A separate grader checked 12 key points from the recorded work, including the user's corrections that caused the original failure.

| Card writer | Key points kept | Answers from the card | Time to build four updates |
| --- | --- | --- | --- |
| `claude-opus-5-5` | 12/12 | 22/22 (two runs) | not timed |
| `claude-sonnet-5-5` | 12/12 | 10/10 | 301 s |
| `claude-haiku-5-5` | 10/12; reversed "keep them running" into "look elsewhere" | 9/10 | 559 s |
| DeepSeek web (free) | 10/12; dropped "keep them running" | 8/10 | 134 s |
| `gpt-6.1-terra` | 9/12; dropped "fix the weakest items first" | 8/10, one wrong fact | 202 s |
| `gpt-6.1-sol` | 9/12; dropped "keep them running" | 8/10 | 358 s |

The other writers lost exactly the corrections that mattered most in the replayed failure, and the larger GPT model did no better than the smaller one. Sonnet matched Opus. Each writer built one card, so a single dropped point can be chance.

### Fix: keep the user's own words verbatim

The lost points were things the user said, and the user's messages are small: 42 messages over four days came to 2,907 characters. The extension therefore appends every user message to the open card itself, outside the model's control. A save replaces only the model's body; the user's words carry over, and a log section copied into the save text is discarded. The same four weaker cards were re-checked with the verbatim log attached.

| Card writer | Key points, summary only → with verbatim log | Answers, summary only → with verbatim log |
| --- | --- | --- |
| `gpt-6.1-sol` | 9/12 → 11/12 | 8/10 → 9/10 |
| DeepSeek web | 10/12 → 11/12 | 8/10 → 9/10 |
| `gpt-6.1-terra` | 9/12 → 11/12 | 8/10 → 8/10 |
| `claude-haiku-5-5` | 10/12, one reversed → 11/12, corrected by the log | 9/10 → 9/10 |

Every user instruction survived with every writer. The one point still missing was a fact the model had found in documentation, a list of restrictions from a platform's documentation, and that still depends on the writer. Answers read about 2,500 more tokens.

Writing in the session's own model is also the cheap path: the context is already in its cache, so a save costs one extra reply. A separate writer would have to be sent the same context again.

## End-to-end conversation (real models)

Two real sessions per model ran over the RPC interface, with only `task_card` available and nothing telling the model to save.

1. **Day 1:** start a blog migration; "never leave images as external links, re-upload all of them, this must hold"; reduce eight categories to four; stop for the day.
2. **Day 2:** a fresh session asks "continue yesterday's work, what did we decide?", then proposes the opposite of the image rule.

| | `gpt-6.1-terra` | `claude-opus-5-5` |
| --- | --- | --- |
| Created a card without being asked | yes | yes |
| Saves on day 1 (4 messages) | 2 | 4 |
| All four user messages in the card verbatim | yes | yes |
| Day 2 opened the right card and listed the decisions | yes | yes |
| Day 2 quoted the image rule and asked before dropping it | yes | yes |
| Context tokens, day 1 / day 2 | 38,805 / 12,436 | 72,669 / 23,048 |

An earlier wording asked the model to save after every policy statement, and both models saved on all four messages. Because the extension now keeps the user's words itself, the tool asks for saves only when work progressed, a decision changed the plan, or the day ends. GPT dropped to two saves, while Opus still saved after each decision.

## Is it ever worse than no card?

The replayed failure was graded again, blind, three runs per condition, with the shipped card. That card carries the user's verbatim words and one added rule line: the card's next steps are a plan, not approval, so ask before changing, submitting, publishing, or deleting anything not approved in the conversation.

| Criterion | No card | Card before the fix | Card with the new line | Card with the new line and the user's verbatim words |
| --- | --- | --- | --- | --- |
| Does not switch items off before the review date | 0/3 | 3/3 | 3/3 | 3/3 |
| States the conflict with the user's instructions | 0/3 | 3/3 | 3/3 | 3/3 |
| Keeps the weakest items running | 0/3 | 0/3 | 0/3 | 2/3 partial |
| Acts without asking (lower is better) | 1/3 | 3/3 | 0/3 | 1/3 |
| Context tokens for the reply | 321,213 | 174,118 | 174,431 | 177,517 |

Before the fix, the card made one thing worse: the model treated the card's next steps as approval and said it would change and resubmit items more often. With the added line, the shipped card is at least as good as no card on every criterion here. With the verbatim words, the model for the first time tried to keep the weakest items running, partially, which was the user's own instruction.

Where a card adds cost without benefit:

- **A single long session with no new session.** The pinned card adds its size to every request (cached), and the transcript still grows. The benefit there is that user instructions survive compaction, not fewer tokens.
- **Unrelated short tasks.** The list costs about 700 tokens once per session, and nothing is opened.

## No cards yet, and the prompt cache (pi, 2026-11-05)

Two problems found in real pi use. Measured with `claude-opus-5-5` (high thinking) over the RPC interface against a copy of the real memory store, with the memory tools, `task_card`, read-only file tools, and the user's global `AGENTS.md`.

**No card was ever created.** In a real two-day session the model looked for a card, got "no task cards are saved; treat this as a new task", and kept saving single facts with `memory_remember`. The replay sends that session's first two user messages: today's to-dos a day after yesterday's work, then a complaint that decisions keep being reversed.

| Scenario | No line | First wording | Current wording |
| --- | --- | --- | --- |
| Resuming yesterday's work (the real two-day session), Korean | 0/3 saved a card; each called `list`, then used `memory_remember` | 5/5 | 2/2 |
| The same two messages in English | not run | 2/6 | 4/4 |
| A new multi-day task stated outright ("this will take days"), Korean | 2/2 | 2/2 | not run |
| One-off questions (a coding script, a status check, the service port, the cause of an earlier failure): no card is correct | not run | 6/6 correct | 5/5 correct |

The explicit case already worked. The line matters when the length of the work is only implied, which is how the real failure happened. The first wording ("work that continues from an earlier session") worked in Korean but not in English. The current wording names concrete signals (asks for today's next steps, refers to yesterday, continues a project) and asks for the save in the same run; every run that saved did so on the first message, and no one-off question got a card.

**Languages and the rest of the lifecycle** (first wording; day 2 does not depend on the no-card line). Day 1 starts a blog migration with an image rule and stops for the day; day 2 is a fresh session that asks to continue, then says the work is completely finished. "Plain setup" means English with no `AGENTS.md` and no skills.

| Step | Korean | English | English, plain setup | Japanese |
| --- | --- | --- | --- | --- |
| Day 1: card saved | 2/2 | 2/2 | 2/2 | 1/1 |
| Day 2: card opened with `get` | 2/2 | 2/2 | 2/2 | 1/1 |
| Day 2: `done` after "it is finished" | 2/2 | 2/2 | 2/2 | 1/1 |

In one plain-setup run the model saved after `done`, which reopens a card, and then marked it done again.

**An open card broke the prompt cache on every prompt.** The pinned text included the verbatim user-message log, which grows with each prompt, so the system prompt changed every time and the whole cached conversation was written again. In a five-message session with a card open, the first call of every later prompt read only 4,109 cached tokens and rewrote the rest (16,916 to 25,399 tokens per prompt), whether or not the card had been saved. The pin is now rendered when the card is first pinned and again only after a compaction. In the same session after the change, the first pin rewrote the cache once; every later prompt read 22,789 to 25,261 tokens from the cache and wrote 214 to 1,190.

| Five-message session, card open | Before | After |
| --- | --- | --- |
| Cache write tokens | 103,777 | 28,412 (−73%) |
| Price-weighted total (input 1, cache read 0.1, cache write 1.25, output 5) | 204,211 | 109,909 (−46%) |

The saving grows with the context: before the change, every prompt of a long session rewrote the whole context, up to the 600K compaction threshold.

Limits: one to three runs per condition; shell and file-writing tools were off for safety. Creating a card still costs one cache rewrite when it is first pinned, plus about 1,000 to 2,000 output tokens per save.

## Messages recorded into the wrong card (pi, 2026-11-06)

Three problems found in a week of real pi use, after the changes above.

- **A card opened by mistake kept the request.** A request for a new task opened an unrelated card first; the model closed it and saved a new card. Opening had already written the request into the wrong card, and closing cleared it, so the new card had no user words at all.
- **Closing came too late.** The user's message was written into the open card when the prompt started, before the model could decide the message was about other work. Across 38 sessions the model closed a card once, and two cards collected messages from later, unrelated tasks.
- **No new card when other cards existed.** The line on when to create a card was shown only when no cards were saved. With any card in the list, a request that resumed another multi-day task got no card.

Changes: the user's message is written when the run settles (`agent_settled`), into the card still open; opening and closing keep the triggering request for the next save instead of writing or dropping it. The pinned rule line asks the model to close a card before answering a message about other work, and the card list says to save a new card for resumed or multi-day work that has none. Hosts without `agent_settled` write earlier messages at the next prompt. The end-of-run write is awaited so a one-shot `pi -p` keeps it, and capped at 1.5 seconds so a stalled core does not hold pi busy.

Measured with `claude-sonnet-5-5` through `pi -p`, against a fake core seeded with two unrelated cards:

| Scenario | Before | After |
| --- | --- | --- |
| Resuming a multi-day task that has no card ("yesterday's work; today X, tomorrow Y") | 0/3 saved a card | 3/3 saved, with the request in its log |
| A new multi-day task while unrelated cards exist | not run | 1/1 new card holds the request; other cards untouched |
| Open a card, then "separately, a one-line CSV merge": the second message stays out | unit test: recorded | 4/4 closed; card holds only the first message |
| One-off questions, one sharing a word with a card: no card | not run | 3/3 no card, no `get` |

Unit tests reproduce the first two problems against the old build (wrong card kept the request, the saved card lacked it, a closed card kept the switching message) and pass on the new one.

## memory_remember with no card open (pi, 2026-11-07)

The list line above did not settle the third problem. In a later real session (`claude-opus-5-5`, five other cards listed, the line present) the user resumed a multi-day project; the model searched the cards by the project's name, got only other tasks back, saved two facts with `memory_remember`, and never saved a card. Rewording the list line, and adding a line under a topic search's results, changed nothing in replays (0 of 5 runs).

What the failures share is structural: multi-day work went to `memory_remember` while no card was open (16 times in the 2026-11-04 session, twice here). The extension now appends one line to the first successful `memory_remember` result of a session with no card open: if the fact belongs to work that started earlier or will go on later, save a card too; single facts from one-off tasks need none. A session that has opened or saved a card never gets it.

Measured with `claude-opus-5-5` over the RPC interface against a copy of the real memory store, with the real cards and the user's `AGENTS.md`:

| Scenario | Without the note | With the note |
| --- | --- | --- |
| The real session's two messages, then the user states a decision and asks to keep it; the model calls `memory_remember` | 0/1 saved a card | 3/3 saved a card right after `memory_remember` |
| One-off fact to remember (a GitHub ID; a commit-message preference in English): no card is correct | not run | 4/4 no card |

Runs where the model did not call `memory_remember` are left out: the note cannot appear there. Without that call, the replayed session still got no card in any build, so this note is a backstop for that case rather than a fix for the first prompt.

## An open card that is no longer the work (pi and hook, 2026-11-08)

A pi session opened a card for its first topic, moved on to other work, and was resumed after a reboot. The extension reopened the card, and for more than 30 user messages the model neither closed nor saved it, so every message of the other work was recorded into it and its body went stale. The pinned rule already says to close a card before answering about other work.

Now, when an open card has gone 8 user messages without being opened or saved, the next prompt gets one reminder: close it if the conversation moved on, or save it with the current state if it is still the task. It fires once per card until the card is saved again. In pi the count is rebuilt from the transcript on resume, so a resumed session that left a card unsaved for 8 or more messages is reminded on its first prompt. The hook counts per session in core memory, from when the session opened or saved the card.

Measured with `claude-opus-5-5` over RPC against a copy of the real memory store:

| Scenario | Without the reminder | With the reminder |
| --- | --- | --- |
| The real session, cut just before a message that was recorded into the wrong card, resumed with that message | 0/2 closed the card | 2/2 closed it |
| A fresh session opens a card, then drifts to other work (two scenarios) | 3/3 closed it by the third message | 2/2 closed it by the third message; the reminder never fired |
| 11 messages on the same task as the open card | 1/1 saved once | 1/1 saved twice; the reminder led to one extra save |

Cost: the reminder itself is about 100 tokens, once per stale card. In the replays the weighted token use was the same with and without it (436K vs 441K, 112K vs 116K). Where the card was still the task, the extra save cost about 11K weighted tokens (1.5K output tokens) in an 11-message session; that save is the one that keeps the card's body current.

`/memnest` now also lists the active cards with their last save in local time, and which cards this session records into:

```text
Task cards: 6 active, newest first
  Windows bluescreen (AdGuard WFP driver) (saved 2026-11-08 01:14)
  ...
Open in this session: none
To change: ask the assistant, e.g. "close the task card" or "mark the task card done".
```

## Rules stored before the card existed (2026-11-10)

A card only held what was said after it was created. In a real pi session the user had told the model, five days before the card was first saved, to stop proposing new changes while results were pending. That rule was stored as a memory in the work's project, but none of the card's 15 versions contained it, the default search scope did not reach that project, and the model made the same kind of proposal again until the user repeated the rule.

Now the answer to the first save of a new card lists memories already stored as the user's rules and preferences for that work (importance `preference` or kind `rule`), with their ids, and asks the model to copy the ones that still apply into the card. The work's project is the one that holds at least three of the positively scored search hits for the card's title and text; its rules are listed newest first, then rule hits from the search itself, 10 lines at most, one 200-character excerpt each. The list is labelled as reference data, and stored reminder tags are defused the same way as card text. Saving an existing card adds nothing.

Checked on a copy of the real store (54,000 memories), with the build under test on a scratch port:

| New card | Listed |
| --- | --- |
| The same work as the real session (its project holds 16 stored rules) | 10 rules from that project, newest first, including the two that the real session missed |
| An unrelated topic with no stored memories | 1 unrelated rule (a recent memory that the vector search returned) |
| Work whose project holds no rules | 1 rule from the shared `playbook` |

Limits: the list is shown once, in the tool result, and the model still decides what to copy; whether models copy the right rules has not been measured. An unrelated line can appear, as in the second row. Hosts that write cards through an older core do not get the list.

## Remaining limits and how they are handled

| Limit | Handling | Checked |
| --- | --- | --- |
| Two sessions record into one card at the same time | Each write re-reads the newest version, merges, and checks after writing; if another session overwrote it, it merges again (up to three tries). Writes within one session run one at a time. | Unit test: two extension instances each add two messages; all four are kept. Twenty rapid messages in one session: all kept, oldest trimmed past 8,000 characters. |
| The user moves on to other work while a card is open | `task_card action=close` unpins the card and stops recording. A message is written only when the run ends with the card still open, so closing during that run keeps the switching message out too. Opening an existing card records only the request that picked it up, not the session's earlier messages. | Real RPC session, both `gpt-6.1-terra` and `claude-opus-5-5`: after "that can wait, now something else entirely", each model closed the card and later messages stayed out. `claude-sonnet-5-5` through `pi -p`: 4/4 closed, and the switching message stayed out as well. The model still has to close; in real use it rarely did before the rule line asked for it. |
| Facts the model found itself, such as restrictions from documentation, still depend on the writer | Not removable by design. The full transcripts stay in memnest, so `memory_search` can recover them. | The 11th of 12 key points stayed missing for the weaker writers. |
