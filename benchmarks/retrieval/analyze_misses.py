#!/usr/bin/env python3
"""Classify LoCoMo retrieval misses by cause (heuristic labels + printed examples).

    python3 benchmarks/retrieval/analyze_misses.py results/<date>-v0.5.1-locomo.json [--examples 3]

A miss is a scored question whose evidence turns are all absent from the top 10.
Labels are not exclusive; `primary` is the first matching label in PRIORITY order.
"""

import argparse
import collections
import json
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import build  # noqa: E402
import datasets  # noqa: E402

STOP = {
    'a', 'an', 'the', 'is', 'are', 'was', 'were', 'be', 'been', 'am', 'do', 'does', 'did',
    'have', 'has', 'had', 'of', 'in', 'on', 'at', 'to', 'for', 'from', 'with', 'by', 'and',
    'or', 'but', 'not', 'no', 'what', 'when', 'where', 'who', 'whom', 'whose', 'which', 'why',
    'how', 'that', 'this', 'these', 'those', 'it', 'its', 'his', 'her', 'their', 'they',
    'them', 'he', 'she', 'we', 'you', 'i', 'me', 'my', 'our', 'your', 'about', 'after',
    'before', 'as', 'if', 'so', 'than', 'then', 'there', 'into', 'out', 'up', 'down', 'over',
    'under', 'again', 'also', 'would', 'could', 'should', 'will', 'can', 'may', 'might', 's',
    't', 'd', 'll', 'just',
}
TEMPORAL_TEXT = re.compile(
    r"\b(yesterday|today|tomorrow|last (week|month|year|night|weekend|friday|saturday|sunday|monday|tuesday|wednesday|thursday)|"
    r"next (week|month|year)|this (week|month|year|weekend)|ago|recently|earlier|tonight|the other day)\b",
    re.I,
)


def words(text):
    return [w for w in re.findall(r"[a-z0-9']+", text.lower()) if w not in STOP and len(w) > 1]


def stem(word):
    for suffix in ("ing", "ed", "es", "s"):
        if word.endswith(suffix) and len(word) - len(suffix) >= 3:
            return word[: -len(suffix)]
    return word


def load_turns():
    data, _ = datasets.load("locomo")
    turns = {}
    speakers = {}
    for group in build.locomo_groups(data, False):
        for turn in group["turns"]:
            turns[(group["project"], turn["key"])] = turn["text"]
    for index, sample in enumerate(data):
        conv = sample["conversation"]
        speakers[f"locomo-{index}-{sample['sample_id']}"] = {
            conv["speaker_a"].lower(),
            conv["speaker_b"].lower(),
        }
    return turns, speakers


def classify(record, turns, speakers):
    project = record["project"]
    relevant = record["relevant"]
    texts = [turns[(project, key)] for key in relevant]
    labels = []
    depth = record["ranked_depth50"]
    ranks = [depth.index(k) + 1 for k in relevant if k in depth]
    best = min(ranks) if ranks else None
    if best is None:
        labels.append("not-in-top-50")
    elif best <= 20:
        labels.append("rank-11-20")
    else:
        labels.append("rank-21-50")
    if len(relevant) > 1:
        labels.append("multi-evidence")
    if record["n_returned"] < 10:
        labels.append("fewer-than-10-returned")
    if record["category"] == "temporal":
        labels.append("temporal-question")
    if any(TEMPORAL_TEXT.search(t) for t in texts) and record["category"] in ("temporal", "multi-hop"):
        labels.append("evidence-uses-relative-date")
    if record["category"] == "open-domain":
        labels.append("open-domain-knowledge")
    if record["category"] == "adversarial":
        labels.append("adversarial-wrong-speaker-premise")
    # strip the "<speaker> said: " label, keep the spoken text
    spoken = [re.sub(r"^.*? said: ", "", t, count=1) for t in texts]
    if all(len(s) < 60 for s in spoken):
        labels.append("short-evidence-turn")
    names = speakers.get(project, set())
    q_words = {stem(w) for w in words(record["question"]) if w not in names}
    ev_words = {stem(w) for s in spoken for w in words(s)}
    overlap = q_words & ev_words
    if q_words and not overlap:
        labels.append("no-lexical-overlap")
    elif q_words and len(overlap) / len(q_words) < 0.25:
        labels.append("low-lexical-overlap")
    if len(q_words) <= 2:
        labels.append("very-short-question")
    return labels, best, sorted(overlap)


PRIORITY = [
    "adversarial-wrong-speaker-premise",
    "open-domain-knowledge",
    "evidence-uses-relative-date",
    "temporal-question",
    "multi-evidence",
    "no-lexical-overlap",
    "short-evidence-turn",
    "low-lexical-overlap",
    "rank-11-20",
    "rank-21-50",
    "not-in-top-50",
]


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("results")
    parser.add_argument("--examples", type=int, default=2)
    parser.add_argument("--json-out", default="")
    args = parser.parse_args()
    with open(args.results, encoding="utf-8") as handle:
        result = json.load(handle)
    turns, speakers = load_turns()
    rows = [r for r in result["per_question"] if r["relevant"]]
    misses = [r for r in rows if not (set(r["ranked"]) & set(r["relevant"]))]
    print(f"scored={len(rows)} misses(top10)={len(misses)} ({len(misses)/len(rows):.1%})")
    label_counts = collections.Counter()
    primary_counts = collections.Counter()
    by_primary = collections.defaultdict(list)
    out = []
    for r in misses:
        labels, best, overlap = classify(r, turns, speakers)
        label_counts.update(labels)
        primary = next((p for p in PRIORITY if p in labels), "other")
        primary_counts[primary] += 1
        by_primary[primary].append((r, labels, best))
        out.append({"id": r["id"], "category": r["category"], "labels": labels,
                    "primary": primary, "best_rank_in_top50": best})
    print("\nlabel frequency (non-exclusive):")
    for label, count in label_counts.most_common():
        print(f"  {count:4d}  {label}")
    print("\nprimary cause (first match in priority order):")
    for label, count in primary_counts.most_common():
        print(f"  {count:4d}  {label}")
    near = sum(1 for o in out if o["best_rank_in_top50"] and o["best_rank_in_top50"] <= 20)
    print(f"\nmisses whose best evidence is at rank 11-20: {near}; 11-50: "
          f"{sum(1 for o in out if o['best_rank_in_top50'])}; not in top 50: "
          f"{sum(1 for o in out if not o['best_rank_in_top50'])}")
    print("\nexamples:")
    for primary, items in by_primary.items():
        print(f"\n## {primary} ({len(items)})")
        for r, _labels, best in items[: args.examples]:
            print(f"- [{r['category']}] {r['question']!r} -> evidence {r['relevant']} best_rank={best}")
            for key in r["relevant"][:2]:
                print(f"    evidence {key}: {turns[(r['project'], key)][:200]!r}")
            top = r["ranked"][0] if r["ranked"] else None
            if top:
                print(f"    top1 {top}: {turns[(r['project'], top)][:160]!r}")
    if args.json_out:
        with open(args.json_out, "w", encoding="utf-8") as handle:
            json.dump(out, handle, indent=1)


if __name__ == "__main__":
    main()
