const ENGLISH_STOPWORDS: &[&str] = &[
    "a", "an", "and", "are", "as", "at", "be", "by", "do", "does", "for", "from", "how", "in",
    "is", "it", "of", "on", "or", "our", "should", "the", "this", "to", "was", "what", "when",
    "where", "which", "who", "why", "with",
];

/// Conversational words that say nothing about a topic. Only [`topic_keywords`]
/// drops them; `extract_keywords` and explicit search still match them. In the
/// 2026-11-10 measurement these words were the usual reason an unrelated memory
/// looked related to a prompt (docs/search-relevance.md).
#[rustfmt::skip]
const CONVERSATIONAL_FILLERS: &[&str] = &[
    "그러면", "그럼", "근데", "그런데", "그리고", "그래서", "그러니까", "그니까", "그래도", "그래", "그냥",
    "지금", "이제", "아까", "일단", "다시", "계속", "이미", "원래", "혹시", "진짜", "정말",
    "이거", "저거", "그거", "이건", "저건", "그건", "이게", "저게", "그게", "이걸", "저걸", "그걸",
    "이런", "저런", "그런", "이렇게", "저렇게", "그렇게", "어떻게", "어떤", "무슨", "뭔가",
    "아니", "아니면", "우리", "저기",
    "so", "now", "then", "ok", "okay", "we", "you", "me", "my", "did", "can", "could", "would",
    "will", "that", "there", "here", "just", "again", "about", "if", "but", "not", "no", "yes",
    "please",
];

fn is_english_stopword(token: &str) -> bool {
    ENGLISH_STOPWORDS
        .iter()
        .any(|word| token.eq_ignore_ascii_case(word))
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
        .filter(|t| t.chars().count() >= min_len)
        .map(strip_korean_particles)
        .filter(|t| t.chars().count() >= min_len)
        .filter(|t| !is_english_stopword(t))
        .map(|t| t.to_string())
        .collect()
}

/// Query keywords that can show a memory shares the query's topic: lowercased,
/// each once, without conversational fillers.
pub fn topic_keywords(query: &str) -> Vec<String> {
    let mut keywords: Vec<String> = Vec::new();
    for keyword in extract_keywords(query, 2) {
        // "port." and "다시." are the words "port" and "다시"; `extract_keywords`
        // keeps the punctuation, and explicit search goes on using that form.
        let keyword = keyword
            .trim_matches(|c: char| !c.is_alphanumeric())
            .to_lowercase();
        if keyword.chars().count() >= 2
            && !CONVERSATIONAL_FILLERS.contains(&keyword.as_str())
            && !keywords.contains(&keyword)
        {
            keywords.push(keyword);
        }
    }
    keywords
}

#[cfg(test)]
mod keyword_tests {
    use super::{extract_keywords, topic_keywords};

    #[test]
    fn topic_keywords_drop_fillers_and_repeats_in_both_languages() {
        assert_eq!(
            topic_keywords("그러면 지금 Redis 설정은 어떻게 하고 redis 포트는"),
            ["redis", "설정", "하고", "포트"]
        );
        assert!(topic_keywords("ok so what should we do now then").is_empty());
        assert!(topic_keywords("확실히 그럼 어떻게 지금").len() == 1);
        // Punctuation at the edge of a word, and a filler that loses its particle.
        assert_eq!(
            topic_keywords("what was the deploy port."),
            ["deploy", "port"]
        );
        assert!(topic_keywords("ok then do that again. 그럼 다시. 그래도").is_empty());
    }

    #[test]
    fn minimum_length_counts_characters_not_utf8_bytes() {
        assert_eq!(
            extract_keywords("끝난 다음 처리할 일", 2),
            ["끝난", "다음", "처리할"]
        );
    }

    #[test]
    fn english_question_words_are_not_search_evidence() {
        assert_eq!(
            extract_keywords("What is the payment processor account ID?", 2),
            ["payment", "processor", "account", "ID"]
        );
    }
}
