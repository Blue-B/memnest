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
        .filter(|t| t.chars().count() >= min_len)
        .map(strip_korean_particles)
        .filter(|t| t.chars().count() >= min_len)
        .map(|t| t.to_string())
        .collect()
}

#[cfg(test)]
mod keyword_tests {
    use super::extract_keywords;

    #[test]
    fn minimum_length_counts_characters_not_utf8_bytes() {
        assert_eq!(
            extract_keywords("끝난 다음 처리할 일", 2),
            ["끝난", "다음", "처리할"]
        );
    }
}
