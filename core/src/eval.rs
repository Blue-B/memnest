//! Similarity scoring, plus the retrieval-quality metrics behind the offline
//! eval harness.
//!
//! `cosine` is the only item here that ships: `server::api` scores MMR
//! re-ranking with it. Everything else is `#[cfg(test)]`, because the metrics
//! are read exclusively by the in-crate `retrieval_eval` test in `server::api`
//! and would otherwise be dead weight in the release binary.
//!
//! Note: this `recall_at_k` uses the *full* gold set as the denominator ("did
//! we surface the relevant items?"). That differs from the approximate-NN
//! recall in `benches/hybrid_search.rs`, which keeps its own copy and compares
//! the top-k of an exact brute-force list against the ANN list.

#[cfg(test)]
use std::collections::HashSet;

#[cfg(test)]
fn top_k_set(list: &[String], k: usize) -> HashSet<&String> {
    list.iter().take(k).collect()
}

/// recall@k averaged over queries: for each query, the fraction of its gold ids
/// that appear in the top-k retrieved list, averaged across all queries.
/// Queries with an empty gold set are skipped.
#[cfg(test)]
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
#[cfg(test)]
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
#[cfg(test)]
pub fn precision_at_1(gold: &[Vec<String>], retrieved: &[Vec<String>]) -> f64 {
    let mut hits = 0.0;
    let mut counted = 0usize;
    for (g, r) in gold.iter().zip(retrieved) {
        if g.is_empty() {
            continue;
        }
        counted += 1;
        if let Some(top1) = r.first()
            && g.iter().any(|id| id == top1) {
                hits += 1.0;
            }
    }
    if counted == 0 {
        0.0
    } else {
        hits / counted as f64
    }
}

/// Cosine similarity between two equal-length vectors. Returns 0.0 for empty,
/// mismatched, or zero-norm inputs (so missing embeddings are treated as
/// maximally diverse rather than panicking).
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || a.len() != b.len() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    let sim = dot / (na.sqrt() * nb.sqrt());
    // Guard against NaN/inf from degenerate (e.g. NaN-bearing) embeddings so a
    // bad vector can't poison sort_by(partial_cmp) ordering downstream.
    if sim.is_finite() { sim } else { 0.0 }
}

/// Intra-list redundancy: the fraction of unordered pairs in a result list whose
/// cosine similarity exceeds `threshold`. 0.0 means a fully diverse top-k; 1.0
/// means every pair is a near-duplicate. This is the metric MMR is meant to
/// drive down on auto-logged stores where the same moment is recorded many
/// times. Lists with fewer than two items have redundancy 0.0 by definition.
#[cfg(test)]
pub fn intra_list_redundancy(embeddings: &[Vec<f32>], threshold: f32) -> f64 {
    let n = embeddings.len();
    if n < 2 {
        return 0.0;
    }
    let mut redundant = 0usize;
    let mut total = 0usize;
    for i in 0..n {
        for j in (i + 1)..n {
            total += 1;
            if cosine(&embeddings[i], &embeddings[j]) > threshold {
                redundant += 1;
            }
        }
    }
    redundant as f64 / total as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn cosine_basic() {
        assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-6);
        assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
        assert_eq!(cosine(&[], &[]), 0.0);
        assert_eq!(cosine(&[1.0], &[1.0, 2.0]), 0.0);
    }

    #[test]
    fn redundancy_detects_near_duplicates() {
        let dup = vec![1.0f32, 0.0];
        let other = vec![0.0f32, 1.0];
        // [dup, dup, other]: only the dup/dup pair is redundant -> 1 of 3 pairs.
        let r = intra_list_redundancy(&[dup.clone(), dup.clone(), other], 0.9);
        assert!((r - (1.0 / 3.0)).abs() < 1e-9);
        // all distinct -> 0 ; single item -> 0
        assert_eq!(intra_list_redundancy(&[vec![1.0, 0.0], vec![0.0, 1.0]], 0.9), 0.0);
        assert_eq!(intra_list_redundancy(&[vec![1.0, 0.0]], 0.9), 0.0);
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
