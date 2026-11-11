use anyhow::Result;

/// Weighted reciprocal-rank fusion of the vector and the keyword result lists.
///
/// A list contributes `k / (k + rank)` for its entry at `rank` (0 is first); the
/// keyword list counts `text_weight` times as much as the vector list. The sum is
/// divided by `1 + text_weight`, so 1.0 means first in both lists and the score
/// does not depend on how many candidates were requested.
pub fn rrf_fusion(
    vector_results: &[(String, f32)],
    text_results: &[(String, f32)],
    k: f32,
    text_weight: f32,
) -> Result<Vec<(String, f32)>> {
    let mut scores: std::collections::HashMap<String, f32> = std::collections::HashMap::new();
    let total_weight = 1.0 + text_weight;

    for (rank, (id, _score)) in vector_results.iter().enumerate() {
        *scores.entry(id.clone()).or_insert(0.0) += k / (k + rank as f32) / total_weight;
    }

    for (rank, (id, _score)) in text_results.iter().enumerate() {
        *scores.entry(id.clone()).or_insert(0.0) +=
            text_weight * k / (k + rank as f32) / total_weight;
    }

    let mut results: Vec<(String, f32)> = scores.into_iter().collect();
    // total_cmp rather than partial_cmp().unwrap(): the caller can absorb an
    // Err through unwrap_or_default, but a panic inside the comparator takes
    // the process down instead. Every score is a sum of k/(k + rank) terms and
    // cannot be NaN for k > 0, so this only costs a different comparator.
    results.sort_by(|a, b| b.1.total_cmp(&a.1));
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::rrf_fusion;

    fn list(ids: &[&str]) -> Vec<(String, f32)> {
        ids.iter().map(|id| (id.to_string(), 0.0)).collect()
    }

    #[test]
    fn fusion_is_scaled_to_one_and_the_vector_list_counts_more() {
        let vector = list(&["both", "vector-second", "vector-third"]);
        let text = list(&["both", "text-second"]);
        let fused = rrf_fusion(&vector, &text, 10.0, 0.5).unwrap();
        let score = |id: &str| fused.iter().find(|(found, _)| found == id).unwrap().1;

        assert_eq!(fused[0].0, "both");
        assert!((score("both") - 1.0).abs() < 1e-6);
        // Same rank, one list each: the vector list has twice the weight.
        assert!((score("vector-second") - 2.0 * score("text-second")).abs() < 1e-6);
        // A list's contribution falls with the rank: 10/10, 10/11, 10/12 of its weight.
        assert!((score("vector-third") - (10.0 / 12.0) / 1.5).abs() < 1e-6);
    }
}
