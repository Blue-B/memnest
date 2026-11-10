#!/usr/bin/env python3
"""Markdown tables for ANALYSIS.md from a variants.py result file (paired comparison).

    python3 benchmarks/retrieval/variant_tables.py results/<date>-v0.5.1-locomo-variants.json
"""

import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import build  # noqa: E402
import datasets  # noqa: E402
import metrics  # noqa: E402

CAT14 = {"single-hop", "multi-hop", "temporal", "open-domain"}


def recall(row, variant, k):
    return metrics.any_hit_at_k(row["variants"][variant][:k], row["relevant"], k)


def table(rows, variants, title):
    print(f"\n**{title}** (n = {len(rows)})\n")
    print("| Variant | R@1 | R@3 | R@5 | R@10 | R@10 wins / losses vs base |")
    print("|---|---:|---:|---:|---:|---|")
    for name in variants:
        cells = []
        for k in (1, 3, 5, 10):
            limit = {"request_n3": 3, "request_n5": 5}.get(name, 10)
            if k <= limit:
                cells.append(f"{100 * sum(recall(r, name, k) for r in rows) / len(rows):.1f}")
            else:
                cells.append("-")
        wins = sum(1 for r in rows if recall(r, name, 10) > recall(r, "base_n10", 10))
        losses = sum(1 for r in rows if recall(r, name, 10) < recall(r, "base_n10", 10))
        shown = "-" if name in ("base_n10", "request_n3", "request_n5") else f"{wins} / {losses}"
        print(f"| {name} | {' | '.join(cells)} | {shown} |")


def main():
    with open(sys.argv[1], encoding="utf-8") as handle:
        data = json.load(handle)
    text = {}
    source, _ = datasets.load("locomo")
    for group in build.locomo_groups(source, True):
        for q in group["questions"]:
            text[q["id"]] = q["question"]
    rows = data["per_question"]
    variants = list(rows[0]["variants"])
    cat14 = [r for r in rows if r["category"] in CAT14]
    table(cat14, variants, "Categories 1-4")
    table([r for r in cat14 if "'" in text[r["id"]]], variants, "Categories 1-4, question contains an apostrophe")
    table([r for r in cat14 if "'" not in text[r["id"]]], variants, "Categories 1-4, no apostrophe")
    table(rows, variants, "All five categories")
    for category in sorted({r["category"] for r in rows}):
        table([r for r in rows if r["category"] == category],
              ["base_n10", "query_punctuation_stripped", "query_server_keywords_only",
               "rrf_base_plus_server_keywords", "rrf_base_plus_without_names"] if "query_server_keywords_only" in variants
              else ["base_n10", "query_punctuation_stripped", "rrf_base_plus_without_names"], f"Category {category}")
    short = sum(1 for r in rows if len(r["variants"]["base_n10"]) < 10)
    print(f"\nrequests returning fewer than 10 results: base {short}; "
          f"query_as_stored_form {sum(1 for r in rows if len(r['variants']['query_as_stored_form']) < 10)}; "
          f"punctuation stripped {sum(1 for r in rows if len(r['variants']['query_punctuation_stripped']) < 10)}")


if __name__ == "__main__":
    main()
