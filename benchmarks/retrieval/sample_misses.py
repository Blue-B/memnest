#!/usr/bin/env python3
"""Print the fixed random sample of LoCoMo category 1-4 misses that ANALYSIS.md classifies by hand.

    python3 benchmarks/retrieval/sample_misses.py results/<date>-v0.5.1-locomo.json [--size 48] [--seed 1110]

Order of the misses is the order in the result file; the sample is random.Random(seed).sample(...).
"""

import argparse
import json
import os
import random
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import analyze_misses as am  # noqa: E402


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("results")
    parser.add_argument("--size", type=int, default=48)
    parser.add_argument("--seed", type=int, default=1110)
    args = parser.parse_args()
    with open(args.results, encoding="utf-8") as handle:
        result = json.load(handle)
    turns, speakers = am.load_turns()
    am.attach_questions(result["per_question"])
    rows = [
        r for r in result["per_question"]
        if r["relevant"] and r["category"] != "adversarial"
        and not set(r["ranked"]) & set(r["relevant"])
    ]
    print(f"{len(rows)} category 1-4 misses; sampling {args.size} with seed {args.seed}")
    for number, r in enumerate(random.Random(args.seed).sample(rows, args.size), start=1):
        _, best, overlap = am.classify(r, turns, speakers)
        print(f"#{number} [{r['category']}] {r['id']} Q: {r['question']} "
              f"(best rank in 50-probe={best}, returned={r['n_returned']}, shared words={overlap})")
        for key in r["relevant"][:3]:
            print(f"   EVIDENCE {key}: {turns[(r['project'], key)][:230]}")
        if r["ranked"]:
            top = r["ranked"][0]
            print(f"   TOP-1 {top}: {turns[(r['project'], top)][:150]}")


if __name__ == "__main__":
    main()
