"""Retrieval metrics over ranked lists of item ids. No LLM involved."""

import math

KS = (1, 3, 5, 10)


def dedupe_keep_order(items):
    seen = set()
    out = []
    for item in items:
        if item not in seen:
            seen.add(item)
            out.append(item)
    return out


def first_hit_rank(ranked, relevant):
    """1-based rank of the first relevant item, or None."""
    relevant = set(relevant)
    for position, item in enumerate(ranked, start=1):
        if item in relevant:
            return position
    return None


def any_hit_at_k(ranked, relevant, k):
    """1.0 if at least one relevant item is in the top k."""
    return 1.0 if set(ranked[:k]) & set(relevant) else 0.0


def all_hit_at_k(ranked, relevant, k):
    """1.0 if every relevant item is in the top k."""
    relevant = set(relevant)
    return 1.0 if relevant and relevant <= set(ranked[:k]) else 0.0


def fraction_at_k(ranked, relevant, k):
    """Share of relevant items found in the top k."""
    relevant = set(relevant)
    if not relevant:
        return 0.0
    return len(relevant & set(ranked[:k])) / len(relevant)


def reciprocal_rank(ranked, relevant):
    rank = first_hit_rank(ranked, relevant)
    return 0.0 if rank is None else 1.0 / rank


def ndcg_at_k(ranked, relevant, k):
    """Binary-gain NDCG@k. `ranked` must not contain duplicates."""
    relevant = set(relevant)
    if not relevant:
        return 0.0
    dcg = sum(
        1.0 / math.log2(position + 1)
        for position, item in enumerate(ranked[:k], start=1)
        if item in relevant
    )
    ideal_hits = min(len(relevant), k)
    idcg = sum(1.0 / math.log2(position + 1) for position in range(1, ideal_hits + 1))
    return dcg / idcg


def percentile(values, fraction):
    """Nearest-rank percentile; fraction in (0, 1]."""
    if not values:
        return None
    ordered = sorted(values)
    index = max(0, math.ceil(fraction * len(ordered)) - 1)
    return ordered[index]


def mean(values):
    values = list(values)
    return sum(values) / len(values) if values else None
