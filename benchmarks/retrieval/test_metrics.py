"""Self-test for metrics.py: `python3 benchmarks/retrieval/test_metrics.py`."""

import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import metrics as m  # noqa: E402

ranked = ["a", "b", "c", "d"]

assert m.first_hit_rank(ranked, ["c"]) == 3
assert m.first_hit_rank(ranked, ["z"]) is None
assert m.reciprocal_rank(ranked, ["b", "d"]) == 0.5
assert m.reciprocal_rank(ranked, ["z"]) == 0.0

assert m.any_hit_at_k(ranked, ["c", "z"], 2) == 0.0
assert m.any_hit_at_k(ranked, ["c", "z"], 3) == 1.0
assert m.all_hit_at_k(ranked, ["c", "z"], 4) == 0.0
assert m.all_hit_at_k(ranked, ["a", "c"], 3) == 1.0
assert m.all_hit_at_k(ranked, [], 3) == 0.0
assert m.fraction_at_k(ranked, ["a", "z"], 1) == 0.5

# NDCG: one relevant item at rank 1 is perfect, at rank 2 is 1/log2(3).
assert m.ndcg_at_k(ranked, ["a"], 10) == 1.0
assert abs(m.ndcg_at_k(ranked, ["b"], 10) - 1 / math.log2(3)) < 1e-12
# Two relevant, found at ranks 1 and 3: dcg = 1 + 1/log2(4), idcg = 1 + 1/log2(3).
expected = (1 + 1 / 2) / (1 + 1 / math.log2(3))
assert abs(m.ndcg_at_k(ranked, ["a", "c"], 10) - expected) < 1e-12
assert m.ndcg_at_k(ranked, ["c"], 2) == 0.0

assert m.dedupe_keep_order(["x", "y", "x", "z", "y"]) == ["x", "y", "z"]
assert m.percentile([5, 1, 3, 2, 4], 0.5) == 3
assert m.percentile(list(range(1, 101)), 0.95) == 95
assert m.percentile([], 0.5) is None
assert m.mean([1, 2, 3]) == 2

print("metrics ok")
