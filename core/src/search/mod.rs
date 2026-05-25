use crate::models::*;

pub fn compute_decay_score(access_count: i64, days_old: f64, importance: &Importance) -> f64 {
    let boost = match importance {
        Importance::Knowledge => 2.0,
        Importance::Decision => 1.5,
        Importance::Preference => 1.3,
        Importance::Log => 1.0,
    };
    let half_life = 30.0 / boost;
    access_count as f64 * (0.5_f64.powf(days_old / half_life))
}

pub fn compute_composite_score(
    distance: f32,
    days_old: f64,
    chunk_type: &ChunkType,
    importance: &Importance,
    keyword_match_ratio: f32,
) -> f32 {
    let recency_penalty = (days_old as f32 * 0.008).min(0.30);
    let type_bonus = match chunk_type {
        ChunkType::Manual => -0.1,
        ChunkType::Filtered => 0.0,
        ChunkType::AutoLog => 0.05,
        ChunkType::Consolidated => 0.0,
    };
    let importance_bonus = match importance {
        Importance::Knowledge => -0.1,
        Importance::Decision => -0.05,
        Importance::Preference => -0.08,
        Importance::Log => 0.0,
    };
    let kw_bonus = keyword_match_ratio * 0.15;

    distance + recency_penalty + type_bonus + importance_bonus - kw_bonus
}

pub fn strip_korean_particles(token: &str) -> String {
    let particles = [
        "에서", "부터", "까지", "으로", "처럼", "이나", "은", "는", "이", "가", "에", "의", "로",
        "을", "를", "와", "과", "도", "만",
    ];
    let mut result = token.to_string();
    for p in particles {
        if result.ends_with(p) {
            result.truncate(result.len() - p.len());
            break;
        }
    }
    if result.is_empty() {
        token.to_string()
    } else {
        result
    }
}

pub fn extract_keywords(query: &str, min_len: usize) -> Vec<String> {
    let re = regex::Regex::new(r#"[\s,;:!?'"()\[\]{}]+"#).unwrap();
    let tokens: Vec<&str> = re.split(query).collect();
    tokens
        .into_iter()
        .map(|t| t.trim())
        .filter(|t| t.len() >= min_len)
        .map(|t| strip_korean_particles(t))
        .filter(|t| t.len() >= min_len)
        .map(|t| t.to_string())
        .collect()
}
