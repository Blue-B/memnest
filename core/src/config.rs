use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub data_dir: PathBuf,
    pub api_port: u16,
    pub api_host: String,
    pub embed_model: String,
    pub embed_dim: usize,
    pub distance_cutoff: f32,
    pub low_relevance_fallback: usize,
    pub recency_penalty_rate: f32,
    pub recency_penalty_cap: f32,
    pub keyword_max_bonus: f32,
    /// Rank constant of the reciprocal-rank fusion: a list's entry at rank r
    /// (0 first) contributes k / (k + r).
    pub rrf_k: f32,
    /// Weight of the keyword list in the fusion; the vector list has weight 1.
    pub rrf_text_weight: f32,
    /// Score of a memory that both the vector and the keyword search rank first.
    pub fusion_max_score: f32,
    /// Most the rank can add for a memory that contains none of the question's
    /// keywords. Clients treat 0.05 above a saved memory's base score as "this
    /// is about the question" (the pi extension injects at 0.25, a fresh manual
    /// knowledge memory starts at 0.20), so rank alone has to stay below it.
    pub no_keyword_score_cap: f32,
    pub mmr_lambda: f32,
    pub enable_lifecycle: bool,
}

impl Default for Config {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let data_dir = std::env::var("MEMNEST_DATA_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| default_data_dir(&home));
        Self {
            data_dir,
            api_port: 3111,
            api_host: "127.0.0.1".to_string(),
            embed_model: std::env::var("MEMNEST_EMBED_MODEL")
                .unwrap_or_else(|_| "intfloat/multilingual-e5-base".to_string()),
            embed_dim: std::env::var("MEMNEST_EMBED_DIM")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(768),
            distance_cutoff: 0.7,
            low_relevance_fallback: 3,
            recency_penalty_rate: 0.008,
            recency_penalty_cap: 0.30,
            // The fusion constants and the keyword bonus were measured together on
            // LoCoMo and LongMemEval-S, where only their ratio matters, and on a
            // copy of a real store, where their size against importance, type and
            // age matters too (benchmarks/retrieval/ANALYSIS.md section 8). The two
            // maxima add up to 0.18, what the former fusion (at most 0.033) and
            // keyword bonus (0.15) added up to.
            keyword_max_bonus: 0.08,
            rrf_k: 10.0,
            rrf_text_weight: 0.5,
            fusion_max_score: 0.10,
            no_keyword_score_cap: 0.049,
            mmr_lambda: 0.5,
            enable_lifecycle: true,
        }
    }
}

fn default_data_dir(home: &Path) -> PathBuf {
    let current = home.join(".memnest");
    let legacy = home.join(".factory").join("memories");
    if current.exists() || !legacy.exists() {
        current
    } else {
        legacy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_data_dir_preserves_legacy_store_until_migrated() {
        let home = tempfile::tempdir().unwrap();
        let legacy = home.path().join(".factory").join("memories");
        std::fs::create_dir_all(&legacy).unwrap();
        assert_eq!(default_data_dir(home.path()), legacy);

        let current = home.path().join(".memnest");
        std::fs::create_dir_all(&current).unwrap();
        assert_eq!(default_data_dir(home.path()), current);
    }
}
