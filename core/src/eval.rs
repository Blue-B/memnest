//! Pure retrieval-quality metrics for the offline eval harness.
//!
//! These functions are intentionally dependency-free so they can be reused by
//! the in-crate `retrieval_eval` test (see `server::api`) and by benches. They
//! take a per-query `gold` (relevant) id set and the ranked `retrieved` id list
//! per query, and compute standard IR metrics.
//!
//! Note: this `recall_at_k` uses the *full* gold set as the denominator ("did
//! we surface the relevant items?"). That differs from the approximate-NN
//! recall in `benches/hybrid_search.rs`, which compares the top-k of an exact
//! brute-force list against the top-k of the ANN list — a different question.

use std::collections::HashSet;

fn top_k_set(list: &[String], k: usize) -> HashSet<&String> {
    list.iter().take(k).collect()
}

/// recall@k averaged over queries: for each query, the fraction of its gold ids
/// that appear in the top-k retrieved list, averaged across all queries.
/// Queries with an empty gold set are skipped.
pub fn recall_at_k(gold: &[Vec<String>], retrieved: &[Vec<String>], k: usize) -> f64 {
    let mut total = 0.0;
    let mut counted = 0usize;
    for (g, r) in gold.iter().zip(retrieved) {
        if g.is_empty() {
            continue;
        }
        let top = top_k_set(r, k);
        let hits = g.iter().filter(|id| top.contains(id)).count() as f64;
        total += hits / g.len() as f64;
        counted += 1;
    }
    if counted == 0 {
        0.0
    } else {
        total / counted as f64
    }
}

/// Mean reciprocal rank over the top-k: average of `1/rank` of the first gold id
/// found in each query's retrieved list (0 when no gold id is within top-k).
pub fn mrr_at_k(gold: &[Vec<String>], retrieved: &[Vec<String>], k: usize) -> f64 {
    let mut total = 0.0;
    let mut counted = 0usize;
    for (g, r) in gold.iter().zip(retrieved) {
        if g.is_empty() {
            continue;
        }
        counted += 1;
        let gset: HashSet<&String> = g.iter().collect();
        for (rank, id) in r.iter().take(k).enumerate() {
            if gset.contains(id) {
                total += 1.0 / (rank as f64 + 1.0);
                break;
            }
        }
    }
    if counted == 0 {
        0.0
    } else {
        total / counted as f64
    }
}

/// precision@1: fraction of queries whose top-1 retrieved id is in the gold set.
pub fn precision_at_1(gold: &[Vec<String>], retrieved: &[Vec<String>]) -> f64 {
    let mut hits = 0.0;
    let mut counted = 0usize;
    for (g, r) in gold.iter().zip(retrieved) {
        if g.is_empty() {
            continue;
        }
        counted += 1;
        if let Some(top1) = r.first() {
            if g.iter().any(|id| id == top1) {
                hits += 1.0;
            }
        }
    }
    if counted == 0 {
        0.0
    } else {
        hits / counted as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn recall_perfect_and_partial() {
        let gold = vec![v(&["a"]), v(&["b", "c"])];
        let retrieved = vec![v(&["a", "x", "y"]), v(&["b", "z", "q"])];
        // q1: 1/1 = 1.0 ; q2: 1 of 2 in top-3 = 0.5 ; avg = 0.75
        assert!((recall_at_k(&gold, &retrieved, 3) - 0.75).abs() < 1e-9);
    }

    #[test]
    fn recall_respects_k_cutoff() {
        let gold = vec![v(&["a"])];
        let retrieved = vec![v(&["x", "y", "a"])];
        assert_eq!(recall_at_k(&gold, &retrieved, 2), 0.0);
        assert_eq!(recall_at_k(&gold, &retrieved, 3), 1.0);
    }

    #[test]
    fn mrr_uses_first_gold_rank() {
        let gold = vec![v(&["a"]), v(&["b"])];
        let retrieved = vec![v(&["x", "a"]), v(&["b", "y"])];
        // q1: rank2 -> 0.5 ; q2: rank1 -> 1.0 ; avg 0.75
        assert!((mrr_at_k(&gold, &retrieved, 5) - 0.75).abs() < 1e-9);
    }

    #[test]
    fn precision_at_1_basic() {
        let gold = vec![v(&["a"]), v(&["b"])];
        let retrieved = vec![v(&["a", "z"]), v(&["x", "b"])];
        assert_eq!(precision_at_1(&gold, &retrieved), 0.5);
    }

    #[test]
    fn empty_gold_skipped() {
        let gold = vec![v(&[]), v(&["a"])];
        let retrieved = vec![v(&["x"]), v(&["a"])];
        assert_eq!(recall_at_k(&gold, &retrieved, 3), 1.0);
    }
}
