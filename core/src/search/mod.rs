use crate::models::*;

pub fn compute_decay_score(access_count: i64, days_old: f64, importance: &Importance) -> f64 {
    // Importance weight: more important memories are more salient AND decay
    // slower (longer half-life).
    //
    // The previous formula was `access_count * 0.5^(days_old / (30 / weight))`,
    // which had two bugs: (1) dividing the half-life by the weight made
    // *important* memories decay *faster* than logs — backwards; (2) multiplying
    // by access_count alone meant any never-accessed chunk scored exactly 0 and
    // was flagged stale regardless of importance or age, so the lifecycle
    // stale-count reported nearly the entire store.
    let weight = match importance {
        Importance::Knowledge => 2.0,
        Importance::Decision => 1.5,
        Importance::Preference => 1.3,
        Importance::Log => 1.0,
    };
    let half_life = 30.0 * weight; // days; important memories persist longer
    // Base salience from importance keeps a fresh-but-unaccessed important chunk
    // above the stale threshold; access count adds reinforcement over time.
    let salience = weight + access_count as f64;
    salience * 0.5_f64.powf(days_old / half_life)
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

#[cfg(test)]
mod decay_tests {
    use super::compute_decay_score;
    use crate::models::Importance;

    #[test]
    fn fresh_chunks_are_not_stale_even_without_access() {
        // A brand-new chunk (age 0, never accessed) must stay above the 0.5
        // stale threshold for every importance — the old formula scored it 0.
        for imp in [
            Importance::Log,
            Importance::Preference,
            Importance::Decision,
            Importance::Knowledge,
        ] {
            assert!(
                compute_decay_score(0, 0.0, &imp) >= 0.5,
                "{imp:?} fresh chunk wrongly flagged stale"
            );
        }
    }

    #[test]
    fn important_memories_decay_slower_than_logs() {
        let days = 90.0;
        let log = compute_decay_score(0, days, &Importance::Log);
        let knowledge = compute_decay_score(0, days, &Importance::Knowledge);
        assert!(
            knowledge > log,
            "knowledge ({knowledge}) should outlast log ({log}) at {days} days"
        );
        // At 90 days an unaccessed log should be stale; knowledge should survive.
        assert!(log < 0.5, "old log should be stale: {log}");
        assert!(knowledge >= 0.5, "old knowledge should persist: {knowledge}");
    }

    #[test]
    fn access_reinforces_salience() {
        let unaccessed = compute_decay_score(0, 30.0, &Importance::Log);
        let accessed = compute_decay_score(10, 30.0, &Importance::Log);
        assert!(accessed > unaccessed, "access should raise salience");
    }
}

pub fn extract_keywords(query: &str, min_len: usize) -> Vec<String> {
    let re = regex::Regex::new(r#"[\s,;:!?'"()\[\]{}]+"#).unwrap();
    let tokens: Vec<&str> = re.split(query).collect();
    tokens
        .into_iter()
        .map(|t| t.trim())
        .filter(|t| t.len() >= min_len)
        .map(strip_korean_particles)
        .filter(|t| t.len() >= min_len)
        .map(|t| t.to_string())
        .collect()
}
