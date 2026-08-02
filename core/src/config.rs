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
    pub mmr_lambda: f32,
    pub mmr_diversity_penalty: f32,
    pub stale_fact_days: i64,
    pub enable_encryption: bool,
    pub enable_graph: bool,
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
            keyword_max_bonus: 0.15,
            mmr_lambda: 0.5,
            mmr_diversity_penalty: 0.15,
            stale_fact_days: 180,
            enable_encryption: true,
            enable_graph: true,
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
