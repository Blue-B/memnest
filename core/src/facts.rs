use crate::models::{Fact, FactHistory};
use crate::redaction::redact_text;

pub fn fact_id(subject: &str, predicate: &str) -> String {
    format!("{}::{}", subject.trim(), predicate.trim()).to_lowercase()
}

pub fn make_fact(
    subject: &str,
    predicate: &str,
    object: &str,
    source_session: Option<&str>,
) -> Option<Fact> {
    let subject = redact_text(subject).trim().to_string();
    let predicate = redact_text(predicate).trim().to_string();
    let object = redact_text(object).trim().to_string();

    if subject.is_empty()
        || predicate.is_empty()
        || object.is_empty()
        || subject.len() > 120
        || predicate.len() > 80
        || object.len() > 1_000
    {
        return None;
    }

    Some(Fact {
        id: fact_id(&subject, &predicate),
        subject,
        predicate,
        object,
        timestamp: chrono::Utc::now(),
        source_session: source_session.map(str::to_string),
        history: Vec::new(),
    })
}

pub fn merge_fact(existing: Option<Fact>, mut next: Fact) -> Fact {
    if let Some(existing) = existing {
        next.history = existing.history;
        if existing.object != next.object {
            next.history.push(FactHistory {
                object: existing.object,
                timestamp: existing.timestamp,
                source_session: existing.source_session,
            });
        }
    }
    next
}

pub fn extract_explicit_facts(text: &str, source_session: Option<&str>) -> Vec<Fact> {
    let mut facts = Vec::new();
    for line in text.lines() {
        let line = trim_list_marker(line.trim());
        if line.is_empty() {
            continue;
        }

        let (line, explicit) = strip_fact_prefix(line);
        let parsed = parse_double_colon_fact(line)
            .or_else(|| explicit.then(|| parse_pipe_fact(line)).flatten())
            .or_else(|| explicit.then(|| parse_dash_fact(line)).flatten());

        if let Some((subject, predicate, object)) = parsed {
            if let Some(fact) = make_fact(subject, predicate, object, source_session) {
                facts.push(fact);
            }
        }
    }
    facts
}

fn trim_list_marker(line: &str) -> &str {
    let line = line.trim_start_matches(['-', '*', '•', ' ']).trim();
    let Some((prefix, rest)) = line.split_once('.') else {
        return line;
    };
    if prefix.chars().all(|ch| ch.is_ascii_digit()) {
        rest.trim()
    } else {
        line
    }
}

fn strip_fact_prefix(line: &str) -> (&str, bool) {
    let lower = line.to_ascii_lowercase();
    for prefix in ["fact:", "facts:", "[fact]", "[facts]", "fact -", "fact:"] {
        if lower.starts_with(prefix) {
            return (line[prefix.len()..].trim(), true);
        }
    }
    (line, false)
}

fn parse_double_colon_fact(line: &str) -> Option<(&str, &str, &str)> {
    let (subject, rest) = line.split_once("::")?;
    let (predicate, object) = split_predicate_object(rest)?;
    Some((subject.trim(), predicate.trim(), object.trim()))
}

fn parse_pipe_fact(line: &str) -> Option<(&str, &str, &str)> {
    let parts = line.split('|').map(str::trim).collect::<Vec<_>>();
    if parts.len() == 3 {
        Some((parts[0], parts[1], parts[2]))
    } else {
        None
    }
}

fn parse_dash_fact(line: &str) -> Option<(&str, &str, &str)> {
    let (subject, rest) = line.split_once(" - ")?;
    let (predicate, object) = split_predicate_object(rest)?;
    Some((subject.trim(), predicate.trim(), object.trim()))
}

fn split_predicate_object(value: &str) -> Option<(&str, &str)> {
    value
        .split_once(':')
        .or_else(|| value.split_once('='))
        .or_else(|| value.split_once("=>"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_explicit_fact_lines() {
        let facts = extract_explicit_facts(
            "FACT: server::ip: 10.20.30.40\n- fact: deploy | port | 9999",
            Some("s1"),
        );
        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0].id, "server::ip");
        assert_eq!(facts[1].predicate, "port");
    }

    #[test]
    fn ignores_plain_colon_lines() {
        let facts = extract_explicit_facts("Summary: changed three files", None);
        assert!(facts.is_empty());
    }
}
