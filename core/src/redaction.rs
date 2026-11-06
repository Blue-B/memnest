use once_cell::sync::Lazy;
use regex::Regex;

/// High-confidence secret shapes — these are nearly always credentials.
/// We redact these aggressively because false positives are rare.
static HIGH_CONFIDENCE_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        // OpenAI / Anthropic style API keys
        Regex::new(r#"\b(sk-[A-Za-z0-9][A-Za-z0-9_-]{16,})\b"#).unwrap(),
        // Slack tokens
        Regex::new(r#"\b(xox[baprs]-[A-Za-z0-9-]{10,})\b"#).unwrap(),
        // GitHub PATs / OAuth / App tokens, including fine-grained PATs
        Regex::new(r#"\b(gh[pousr]_[A-Za-z0-9_]{20,})\b"#).unwrap(),
        Regex::new(r#"\b(github_pat_[A-Za-z0-9_]{20,})\b"#).unwrap(),
        // AWS access keys
        Regex::new(r#"\b(AKIA[0-9A-Z]{16})\b"#).unwrap(),
        // PEM private keys
        Regex::new(r#"-----BEGIN (RSA |OPENSSH |EC |DSA |)PRIVATE KEY-----[\s\S]*?-----END (RSA |OPENSSH |EC |DSA |)PRIVATE KEY-----"#).unwrap(),
        // Google API keys
        Regex::new(r#"\b(AIza[0-9A-Za-z_-]{35})\b"#).unwrap(),
    ]
});

/// Medium-confidence — `password: hunterhunter` style. General memories are
/// always redacted; retrievable credentials belong in the secret vault.
static KV_PATTERNS: Lazy<Vec<Regex>> = Lazy::new(|| {
    vec![
        // password/api_key/token: VALUE or = VALUE, also prefixed env names
        // (DB_PASSWORD=, GITHUB_TOKEN=) and JSON-quoted keys ("password": "...")
        Regex::new(r#"(?i)\b((?:[A-Za-z0-9]+[_-])*(?:api[_-]?key|access[_-]?key|token|secret|password|passwd|pwd|authorization|bearer)(?:[_-][A-Za-z0-9]+)*)\b['\"]?\s*[:=]\s*['\"]?([^'\"\s,;]{8,})"#).unwrap(),
    ]
});

static BEARER_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?i)\bbearer\s+[A-Za-z0-9._~+/=-]{16,}"#).unwrap());

/// `scheme://user:password@host` keeps the user, masks the password.
static URL_PASSWORD_PATTERN: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?i)(\b[a-z][a-z0-9+.-]*://[^/\s:@]+:)[^@\s/]{3,}@"#).unwrap());

/// Redact credentials in general memory text.
///
/// Designed to be safe for code/log content: shell ENV assignments are NOT
/// touched here because users frequently paste configuration snippets they
/// later need to read back. If the line genuinely contains a high-entropy
/// secret it will still be caught by `HIGH_CONFIDENCE_PATTERNS`.
pub fn redact_text(input: &str) -> String {
    let mut output = input.to_string();
    for pattern in HIGH_CONFIDENCE_PATTERNS.iter() {
        output = pattern
            .replace_all(&output, "[REDACTED_SECRET]")
            .to_string();
    }
    for pattern in KV_PATTERNS.iter() {
        // Keep the key visible, mask the value only.
        output = pattern.replace_all(&output, "$1: [REDACTED]").to_string();
    }
    output = BEARER_PATTERN
        .replace_all(&output, "Bearer [REDACTED]")
        .to_string();
    URL_PASSWORD_PATTERN
        .replace_all(&output, "${1}[REDACTED]@")
        .to_string()
}

/// Return true when the input contains at least one high-confidence secret.
/// Used by the auto-classifier to nudge chunks into the secrets vault.
pub fn looks_like_secret(input: &str) -> bool {
    HIGH_CONFIDENCE_PATTERNS
        .iter()
        .any(|pattern| pattern.is_match(input))
}

#[cfg(test)]
mod tests {
    use super::{looks_like_secret, redact_text};

    #[test]
    fn redacts_common_secret_shapes() {
        let text = "OPENAI_API_KEY=sk-abcdefghijklmnopqrstuvwxyz\npassword: hunterhunter";
        let redacted = redact_text(text);
        // High-confidence pattern catches sk- prefix
        assert!(!redacted.contains("abcdefghijklmnopqrstuvwxyz"));
        // KV pattern catches password: VALUE
        assert!(!redacted.contains("hunterhunter"));
    }

    #[test]
    fn preserves_ordinary_env_lines_without_secrets() {
        // ENV assignments without high-entropy values should survive — users
        // routinely paste config they need to read back.
        let text = "DATABASE_URL=postgres://app@localhost/dev\nPORT=8080";
        let redacted = redact_text(text);
        assert!(redacted.contains("postgres://app@localhost/dev"));
        assert!(redacted.contains("PORT=8080"));
    }

    #[test]
    fn redacts_prefixed_quoted_bearer_and_url_credentials() {
        let text = "DB_PASSWORD=hunter2hunter2\nGITHUB_TOKEN=abcdefgh12345678\n\
                    {\"password\": \"quotedsecret1\"}\nAuthorization: Bearer abcdefghijklmnop1234\n\
                    postgres://app:dbpass123@localhost/dev\ngithub_pat_11ABCDEFG0123456789abcdef";
        let redacted = redact_text(text);
        for secret in [
            "hunter2hunter2",
            "abcdefgh12345678",
            "quotedsecret1",
            "abcdefghijklmnop1234",
            "dbpass123",
            "11ABCDEFG0123456789abcdef",
        ] {
            assert!(!redacted.contains(secret), "{secret} leaked: {redacted}");
        }
        assert!(redacted.contains("postgres://app:[REDACTED]@localhost/dev"));
        assert!(redact_text("max_tokens: 4096").contains("4096"));
    }

    #[test]
    fn identifies_pat_shapes() {
        assert!(looks_like_secret(
            "ghp_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA1234"
        ));
        assert!(looks_like_secret("sk-abcdefghijklmnopqrstuvwx"));
        assert!(!looks_like_secret("just a normal sentence"));
    }
}
