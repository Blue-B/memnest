#!/usr/bin/env python3
"""Print the Markdown tables used in docs/benchmarks.md from result JSON files.

    python3 benchmarks/retrieval/tables.py locomo results/<file>.json
    python3 benchmarks/retrieval/tables.py longmemeval results/<file>.json
"""

import json
import sys


def pct(value):
    return "-" if value is None else f"{100 * value:.1f}"


def locomo(result):
    m = result["metrics"]
    head = "| Category | n | R@1 | R@3 | R@5 | R@10 | all-evidence R@10 | MRR@10 |"
    print(head)
    print("|" + "---|" * 8)
    rows = [("**Categories 1-4**", m["overall_categories_1_to_4"])]
    for name in ("single-hop", "multi-hop", "temporal", "open-domain"):
        rows.append((name, m["per_category"][name]))
    rows.append(("adversarial (cat. 5)", m["per_category"]["adversarial"]))
    rows.append(("all 5 categories", m["overall_all_categories"]))
    for name, b in rows:
        print(
            f"| {name} | {b['n']} | {pct(b['any_recall@1'])} | {pct(b['any_recall@3'])} | "
            f"{pct(b['any_recall@5'])} | {pct(b['any_recall@10'])} | "
            f"{pct(b['all_recall@10'])} | {b['mrr@10']:.3f} |"
        )


def lme(result):
    m = result["metrics"]
    print("| Question type | n | session R@1 | R@3 | R@5 | R@10 | NDCG@10 | turn R@10 (any) | turn R@10 (all) |")
    print("|" + "---|" * 9)
    rows = [("**All (non-abstention)**", m["overall"])] + list(m["per_question_type"].items())
    for name, b in rows:
        print(
            f"| {name} | {b['n']} | {pct(b['session_any_recall@1'])} | {pct(b['session_any_recall@3'])} | "
            f"{pct(b['session_any_recall@5'])} | {pct(b['session_any_recall@10'])} | "
            f"{b['session_ndcg@10']:.3f} | {pct(b['turn_any_recall@10'])} | {pct(b['turn_all_recall@10'])} |"
        )
    print()
    print("| Question type | n | session all-answer-sessions R@5 | R@10 |")
    print("|---|---|---|---|")
    for name, b in rows:
        print(f"| {name} | {b['n']} | {pct(b['session_all_recall@5'])} | {pct(b['session_all_recall@10'])} |")


if __name__ == "__main__":
    with open(sys.argv[2], encoding="utf-8") as handle:
        data = json.load(handle)
    {"locomo": locomo, "longmemeval": lme}[sys.argv[1]](data)
