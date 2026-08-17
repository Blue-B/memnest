//! `memnest hook`: prompt-time memory injection for any host.
//!
//! MCP defines tool calls the model chooses to make, not hooks into a host's
//! session events, so automatic injection has to ride on whatever prompt hook
//! the host offers. Those hooks all agree on one thing: run a command, feed it
//! the event on stdin, read stdout. This subcommand is that command, so a host
//! needs one line of configuration instead of a bespoke extension:
//!
//! ```text
//! "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "memnest hook" }] }]
//! ```
//!
//! It reads the payload, asks a running memnest service for a context pack, and
//! answers in the shape the host expects. It never opens the data directory or
//! loads the embedder, and it never fails loudly: a hook that errors out would
//! block the user's prompt, so every failure path prints nothing and exits 0.

use anyhow::{Result, anyhow};
use clap::ValueEnum;
use serde_json::{Value, json};
use std::time::Duration;

/// Prompts shorter than this are conversational filler ("ok", "go on") that
/// retrieval cannot help with. Counted in characters, not bytes, so the budget
/// means the same thing for Korean as for English.
const MIN_PROMPT_CHARS: usize = 12;

/// Keys that mark a payload as a Claude Code hook event rather than some other
/// host's JSON. `prompt` alone is too common to identify anything.
const CLAUDE_CODE_MARKERS: &[&str] = &["hook_event_name", "session_id", "transcript_path"];

/// Where to look for the user's prompt, most specific first.
const PROMPT_KEYS: &[&str] = &["prompt", "query", "text", "message", "input"];

/// Reply shape. Hosts disagree on whether stdout is parsed or pasted verbatim.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum HookFormat {
    /// Pick the shape from the payload: Claude Code's envelope when the payload
    /// looks like one of its hook events, plain text otherwise.
    Auto,
    /// `{"hookSpecificOutput":{"hookEventName":"UserPromptSubmit","additionalContext":"..."}}`.
    /// The nesting is required; Claude Code ignores a top-level `additionalContext`.
    ClaudeCode,
    /// The context pack on its own, for hosts that append stdout to the prompt.
    Text,
    /// `{"context":"..."}` for hosts that parse stdout but define no envelope.
    Json,
}

/// A payload boiled down to what the hook actually needs.
#[derive(Debug, PartialEq, Eq)]
pub struct Resolved {
    pub prompt: String,
    /// Never `Auto`: detection has already run.
    pub format: HookFormat,
}

/// Read the payload. Unknown shapes fall back to plain text rather than
/// failing, because an unrecognised host should still get memory.
pub fn resolve(raw: &str, requested: HookFormat) -> Resolved {
    let (prompt, detected) = match serde_json::from_str::<Value>(raw) {
        Ok(Value::Object(map)) => {
            let prompt = PROMPT_KEYS
                .iter()
                .find_map(|key| map.get(*key).and_then(Value::as_str))
                .unwrap_or_default()
                .to_string();
            let claude_code = map.contains_key("prompt")
                && CLAUDE_CODE_MARKERS.iter().any(|key| map.contains_key(*key));
            let format = if claude_code {
                HookFormat::ClaudeCode
            } else {
                HookFormat::Text
            };
            (prompt, format)
        }
        // Valid JSON that is not an object carries no field to read, and a
        // parse failure means the host piped the prompt itself.
        _ => (raw.to_string(), HookFormat::Text),
    };

    Resolved {
        prompt: prompt.trim().to_string(),
        format: match requested {
            HookFormat::Auto => detected,
            explicit => explicit,
        },
    }
}

/// Whether a prompt is worth a retrieval round trip. Mirrors the pi extension's
/// `isSubstantive`: slash commands are host syntax, not questions, and very
/// short turns carry no terms to match on.
pub fn should_search(prompt: &str) -> bool {
    let trimmed = prompt.trim();
    !trimmed.starts_with('/') && trimmed.chars().count() >= MIN_PROMPT_CHARS
}

/// Wrap the context pack for the host. An empty pack renders as empty output so
/// the host appends nothing at all.
pub fn render(format: HookFormat, context: &str) -> String {
    if context.trim().is_empty() {
        return String::new();
    }
    match format {
        HookFormat::ClaudeCode => json!({
            "hookSpecificOutput": {
                "hookEventName": "UserPromptSubmit",
                "additionalContext": context,
            }
        })
        .to_string(),
        HookFormat::Json => json!({ "context": context }).to_string(),
        // `Auto` cannot reach here: `resolve` has already replaced it.
        HookFormat::Text | HookFormat::Auto => context.to_string(),
    }
}

/// Build the `/context` URL from a base address, tolerating a trailing slash.
pub fn context_endpoint(base: &str) -> String {
    format!("{}/context", base.trim().trim_end_matches('/'))
}

/// Resolve the service address: flag, then environment, then the documented default.
pub fn resolve_url(flag: Option<&str>) -> String {
    flag.map(str::to_string)
        .or_else(|| std::env::var("MEMNEST_URL").ok())
        .map(|url| url.trim().to_string())
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| "http://127.0.0.1:3111".to_string())
}

/// Ask the running service for a context pack. Server-side defaults decide the
/// project scope and character budget, so the hook stays in step with `/context`.
async fn fetch_context(base_url: &str, prompt: &str, timeout_ms: u64) -> Result<String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(timeout_ms))
        .build()?;
    let mut request = client.post(context_endpoint(base_url)).json(&json!({
        "query": prompt,
        "adapter": "memnest-hook",
    }));
    if let Ok(token) = std::env::var("MEMNEST_TOKEN")
        && !token.trim().is_empty()
    {
        request = request.bearer_auth(token.trim());
    }

    let response = request.send().await?;
    if !response.status().is_success() {
        return Err(anyhow!("service returned {}", response.status()));
    }
    let body: Value = response.json().await?;
    Ok(body
        .get("prompt")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string())
}

/// Everything between stdin and stdout, so tests can drive the whole path.
/// Returns the exact bytes to print, empty when the host should get nothing.
pub async fn respond(
    raw: &str,
    base_url: &str,
    requested: HookFormat,
    timeout_ms: u64,
) -> (String, Option<String>) {
    let resolved = resolve(raw, requested);
    if !should_search(&resolved.prompt) {
        return (String::new(), None);
    }
    match fetch_context(base_url, &resolved.prompt, timeout_ms).await {
        Ok(context) => (render(resolved.format, &context), None),
        // A missing service is the normal case on a machine where memnest is
        // not running yet, so it is a note on stderr, never a failed prompt.
        Err(e) => (String::new(), Some(e.to_string())),
    }
}

/// Entry point for the `hook` subcommand.
pub async fn run(url: Option<&str>, format: HookFormat, timeout_ms: u64) {
    let raw = std::io::read_to_string(std::io::stdin()).unwrap_or_default();
    let (out, warning) = respond(&raw, &resolve_url(url), format, timeout_ms).await;
    if let Some(warning) = warning {
        eprintln!("memnest hook: no context injected ({warning})");
    }
    if !out.is_empty() {
        println!("{out}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_code_payload_is_detected_and_read() {
        let raw = r#"{"session_id":"x","hook_event_name":"UserPromptSubmit","prompt":"what did we decide about the deploy port"}"#;
        let resolved = resolve(raw, HookFormat::Auto);
        assert_eq!(resolved.format, HookFormat::ClaudeCode);
        assert_eq!(resolved.prompt, "what did we decide about the deploy port");
    }

    #[test]
    fn claude_code_output_nests_additional_context() {
        let rendered = render(HookFormat::ClaudeCode, "remembered: port 8320");
        let parsed: Value = serde_json::from_str(&rendered).unwrap();
        // Claude Code silently drops a top-level additionalContext, so the
        // nesting is the part worth pinning down.
        assert!(parsed.get("additionalContext").is_none());
        let inner = &parsed["hookSpecificOutput"];
        assert_eq!(inner["hookEventName"], "UserPromptSubmit");
        assert_eq!(inner["additionalContext"], "remembered: port 8320");
    }

    #[test]
    fn unknown_json_falls_back_to_text_and_scans_prompt_keys() {
        let resolved = resolve(r#"{"query":"how do we handle migrations"}"#, HookFormat::Auto);
        assert_eq!(resolved.format, HookFormat::Text);
        assert_eq!(resolved.prompt, "how do we handle migrations");

        // A bare `prompt` without host markers is still not Claude Code.
        let bare = resolve(r#"{"prompt":"how do we handle migrations"}"#, HookFormat::Auto);
        assert_eq!(bare.format, HookFormat::Text);

        // Later keys are only used when the earlier ones are absent.
        let message = resolve(r#"{"message":"restore from the backup directory"}"#, HookFormat::Auto);
        assert_eq!(message.prompt, "restore from the backup directory");
    }

    #[test]
    fn non_json_stdin_is_taken_as_the_prompt() {
        let resolved = resolve("  which port does deploy use  ", HookFormat::Auto);
        assert_eq!(resolved.format, HookFormat::Text);
        assert_eq!(resolved.prompt, "which port does deploy use");
    }

    #[test]
    fn trivial_prompts_are_skipped() {
        assert!(!should_search("ok"));
        assert!(!should_search("   go on   "));
        // Korean is counted in characters, so a real question qualifies even
        // though it is well under 12 bytes per character.
        assert!(should_search("배포 포트가 어떻게 되지"));
        assert!(should_search("what did we decide about the port"));
    }

    #[test]
    fn slash_commands_are_skipped() {
        assert!(!should_search("/memnest status please show me everything"));
        assert!(should_search("memnest status please show me everything"));
    }

    #[test]
    fn empty_context_renders_nothing() {
        for format in [HookFormat::ClaudeCode, HookFormat::Text, HookFormat::Json] {
            assert_eq!(render(format, ""), "");
            assert_eq!(render(format, "   \n "), "");
        }
    }

    #[test]
    fn explicit_format_overrides_detection() {
        let raw = r#"{"session_id":"x","hook_event_name":"UserPromptSubmit","prompt":"long enough prompt here"}"#;
        assert_eq!(resolve(raw, HookFormat::Text).format, HookFormat::Text);
        assert_eq!(resolve("plain text", HookFormat::ClaudeCode).format, HookFormat::ClaudeCode);
    }

    #[test]
    fn endpoint_tolerates_a_trailing_slash() {
        assert_eq!(
            context_endpoint("http://127.0.0.1:3111"),
            "http://127.0.0.1:3111/context"
        );
        assert_eq!(
            context_endpoint("http://127.0.0.1:3111/"),
            "http://127.0.0.1:3111/context"
        );
    }

    #[tokio::test]
    async fn a_missing_service_prints_nothing() {
        // Port 1 is never a memnest service, so this exercises the path that
        // must not block a user's prompt.
        let raw = r#"{"session_id":"x","hook_event_name":"UserPromptSubmit","prompt":"what did we decide about the deploy port"}"#;
        let (out, warning) = respond(raw, "http://127.0.0.1:1", HookFormat::Auto, 500).await;
        assert_eq!(out, "");
        assert!(warning.is_some(), "the failure should be reported on stderr");
    }

    #[tokio::test]
    async fn a_skipped_prompt_never_calls_the_service() {
        // An unreachable URL proves no request was attempted: a call would have
        // produced a warning.
        let (out, warning) = respond("/status", "http://127.0.0.1:1", HookFormat::Auto, 500).await;
        assert_eq!(out, "");
        assert!(warning.is_none());
    }
}
