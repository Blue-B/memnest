use anyhow::Result;

pub fn rrf_fusion(
    vector_results: &[(String, f32)],
    text_results: &[(String, f32)],
    k: f32,
) -> Result<Vec<(String, f32)>> {
    let mut scores: std::collections::HashMap<String, f32> = std::collections::HashMap::new();

    for (rank, (id, _score)) in vector_results.iter().enumerate() {
        let rrf_score = 1.0 / (k + rank as f32);
        *scores.entry(id.clone()).or_insert(0.0) += rrf_score;
    }

    for (rank, (id, _score)) in text_results.iter().enumerate() {
        let rrf_score = 1.0 / (k + rank as f32);
        *scores.entry(id.clone()).or_insert(0.0) += rrf_score;
    }

    let mut results: Vec<(String, f32)> = scores.into_iter().collect();
    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    Ok(results)
}
