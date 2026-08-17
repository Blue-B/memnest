//! `memnest watch`: automatic conversation logging with no host configuration.
//!
//! MCP carries tool calls, not session events, so automatic logging normally
//! needs a per-host extension. But every host already writes its transcript to
//! a local JSONL file, and that file is the same event stream an extension
//! would subscribe to. Tailing it works for any host that keeps one, including
//! hosts this crate has never heard of, and costs the user zero lines of
//! configuration:
//!
//! ```text
//! memnest watch
//! ```
//!
//! Hosts are recognised by the shape of a line rather than by the directory it
//! sits in, so `--path` can point at a transcript anywhere.
//!
//! Like `tail -f`, a file seen for the first time is followed from its end.
//! Historical transcripts are only imported when `backfill` is set, so turning
//! the watcher on cannot flood the store with months of old sessions.

use crate::models::{ChunkType, Importance, Metadata};
use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Transcript locations checked when no `--path` is given.
const DEFAULT_ROOTS: &[&str] = &[".claude/projects", ".pi/agent/sessions"];

/// Shorter turns are acknowledgements ("ok", "go on") that cost an embedding
/// and match nothing later. Counted in characters so the budget means the same
/// for Korean as for English.
const MIN_TEXT_CHARS: usize = 12;

/// Upper bound on one stored turn, matching the pi extension's AutoLog cap.
const MAX_TEXT_CHARS: usize = 8000;

/// Bytes read from one file per cycle. A first `backfill` pass over a large
/// transcript is spread across cycles instead of loaded at once.
const MAX_BYTES_PER_CYCLE: u64 = 4 * 1024 * 1024;

/// Lines inspected when recovering a pi session header mid-file.
const HEADER_SCAN_LINES: usize = 200;

const STATE_FILE: &str = "watch-state.json";

/// One conversation turn worth storing.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Event {
    /// Host that wrote the line, reported to the service as the adapter.
    host: &'static str,
    /// `user` or `assistant`; other roles never reach here.
    role: &'static str,
    text: String,
    session_id: String,
    cwd: Option<String>,
    /// Host's own id for the line, so a stored chunk can be traced back.
    event_id: Option<String>,
}

impl Event {
    /// Collection name. The engine buckets by cwd basename, and a transcript
    /// without a cwd belongs to the same root bucket as other unattributed logs.
    fn project(&self) -> String {
        self.cwd
            .as_deref()
            .map(Path::new)
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("root")
            .to_string()
    }

    /// Rendered document, matching the prefixes the pi extension already writes
    /// so both paths read alike in search results.
    fn document(&self) -> (String, bool) {
        let label = if self.role == "user" {
            "User said"
        } else {
            "Assistant answered"
        };
        let (text, truncated) = clip(&self.text, MAX_TEXT_CHARS);
        (format!("{label}: {text}"), truncated)
    }
}

/// Per-file progress. `session_id` and `cwd` are cached because pi records them
/// once in a header line that a resumed read starts past.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct FileState {
    #[serde(default)]
    offset: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cwd: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct WatchState {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    files: BTreeMap<String, FileState>,
}

/// A parsed line and the offset that reading it reaches. `event` is `None` for
/// lines that carry no storable turn; their offset still has to advance.
#[derive(Debug)]
struct Pending {
    end_offset: u64,
    event: Option<Event>,
}

fn clip(text: &str, max_chars: usize) -> (String, bool) {
    if text.chars().count() <= max_chars {
        return (text.to_string(), false);
    }
    let head: String = text.chars().take(max_chars).collect();
    (format!("{head}\n…[truncated]"), true)
}

/// Remove `<system-reminder>` blocks, which are context the harness injects
/// into the transcript rather than anything a human or the model said.
fn strip_reminders(text: &str) -> String {
    const OPEN: &str = "<system-reminder>";
    const CLOSE: &str = "</system-reminder>";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(OPEN) {
        out.push_str(&rest[..start]);
        match rest[start + OPEN.len()..].find(CLOSE) {
            Some(end) => rest = &rest[start + OPEN.len() + end + CLOSE.len()..],
            // Unterminated block: everything after it is injected text.
            None => {
                rest = "";
                break;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Pull conversation text out of a message body, dropping every part that is
/// machinery: reasoning traces, tool calls, tool output, images. Those are the
/// parts that made the pi extension's AutoLog unreadable.
fn message_text(message: &Value, text_part: &str) -> String {
    let content = message.get("content");
    let raw = match content {
        // Some turns store the body as a bare string.
        Some(Value::String(text)) => text.clone(),
        Some(Value::Array(parts)) => parts
            .iter()
            .filter(|part| part.get("type").and_then(Value::as_str) == Some(text_part))
            .filter_map(|part| part.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    };
    strip_reminders(&raw).trim().to_string()
}

fn is_storable(text: &str) -> bool {
    text.chars().count() >= MIN_TEXT_CHARS
}

/// Claude Code: one object per line, `type` carries the role, and `cwd` and
/// `sessionId` repeat on every line.
fn parse_claude_code(line: &Value) -> Option<Event> {
    let role = match line.get("type").and_then(Value::as_str) {
        Some("user") => "user",
        Some("assistant") => "assistant",
        _ => return None,
    };
    // Sidechains are a subagent talking to itself, and meta lines are injected
    // context; neither is part of the conversation the user had.
    if line.get("isSidechain").and_then(Value::as_bool) == Some(true)
        || line.get("isMeta").and_then(Value::as_bool) == Some(true)
    {
        return None;
    }
    let message = line.get("message")?;
    // A `user` line whose parts are all tool_result is tool output wearing the
    // user role; keeping only `text` parts drops it.
    let text = message_text(message, "text");
    if !is_storable(&text) {
        return None;
    }
    Some(Event {
        host: "claude-code",
        role,
        text,
        session_id: line
            .get("sessionId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        cwd: line
            .get("cwd")
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|cwd| !cwd.is_empty()),
        event_id: line
            .get("uuid")
            .and_then(Value::as_str)
            .map(str::to_string),
    })
}

/// pi: `type` is always `message` and the role sits inside. `toolResult` is a
/// role of its own, which keeps tool output out without inspecting parts.
fn parse_pi(line: &Value, state: &FileState) -> Option<Event> {
    if line.get("type").and_then(Value::as_str) != Some("message") {
        return None;
    }
    let message = line.get("message")?;
    let role = match message.get("role").and_then(Value::as_str) {
        Some("user") => "user",
        Some("assistant") => "assistant",
        _ => return None,
    };
    let text = message_text(message, "text");
    if !is_storable(&text) {
        return None;
    }
    Some(Event {
        host: "pi",
        role,
        text,
        session_id: state.session_id.clone().unwrap_or_default(),
        cwd: state.cwd.clone(),
        event_id: line.get("id").and_then(Value::as_str).map(str::to_string),
    })
}

/// Recognise the host from the line itself, so a transcript found anywhere is
/// handled the same way. Header lines update the cached session identity.
fn parse_line(raw: &str, state: &mut FileState) -> Option<Event> {
    let line: Value = serde_json::from_str(raw.trim()).ok()?;
    if let Some(header) = pi_header(&line) {
        let (session_id, cwd) = header;
        state.session_id = session_id;
        state.cwd = cwd;
        return None;
    }
    if let Some(event) = parse_claude_code(&line) {
        // Claude Code repeats the identity on every line; keep it so the state
        // file stays useful for inspection.
        state.session_id = Some(event.session_id.clone());
        state.cwd = event.cwd.clone();
        return Some(event);
    }
    parse_pi(&line, state)
}

/// pi opens a transcript with a `session` line holding the id and cwd.
fn pi_header(line: &Value) -> Option<(Option<String>, Option<String>)> {
    if line.get("type").and_then(Value::as_str) != Some("session") {
        return None;
    }
    Some((
        line.get("id").and_then(Value::as_str).map(str::to_string),
        line.get("cwd")
            .and_then(Value::as_str)
            .map(str::to_string)
            .filter(|cwd| !cwd.is_empty()),
    ))
}

/// Recover session identity when reading resumes past the header line.
fn scan_header(path: &Path) -> Option<(Option<String>, Option<String>)> {
    let file = std::fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let mut buf = Vec::new();
    for _ in 0..HEADER_SCAN_LINES {
        buf.clear();
        if reader.read_until(b'\n', &mut buf).ok()? == 0 {
            break;
        }
        let Ok(line) = serde_json::from_slice::<Value>(&buf) else {
            continue;
        };
        if let Some(header) = pi_header(&line) {
            return Some(header);
        }
        // A Claude Code line carries the same facts on its face.
        if let Some(event) = parse_claude_code(&line) {
            return Some((Some(event.session_id), event.cwd));
        }
    }
    None
}

/// Read whatever has been appended since the last successful send. Only whole
/// lines are returned: a transcript is appended to while we read it, and half a
/// line now would be a lost line later.
fn read_pending(path: &Path, state: &mut FileState) -> Result<Vec<Pending>> {
    let file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();
    if len < state.offset {
        // Rotated or rewritten. Start over rather than seek past the end.
        tracing::debug!("watch: {} shrank, rereading from the start", path.display());
        *state = FileState::default();
    }
    if len == state.offset {
        return Ok(Vec::new());
    }
    if state.offset > 0 && state.session_id.is_none()
        && let Some((session_id, cwd)) = scan_header(path)
    {
        state.session_id = session_id;
        state.cwd = cwd;
    }

    let mut reader = BufReader::new(file);
    reader.seek(SeekFrom::Start(state.offset))?;

    let mut pending = Vec::new();
    let mut consumed = 0u64;
    let mut buf = Vec::new();
    loop {
        buf.clear();
        let read = reader.read_until(b'\n', &mut buf)?;
        if read == 0 || !buf.ends_with(b"\n") {
            break;
        }
        consumed += read as u64;
        // Transcripts are UTF-8; a damaged byte should cost one character, not
        // the whole file.
        let line = String::from_utf8_lossy(&buf);
        pending.push(Pending {
            end_offset: state.offset + consumed,
            event: parse_line(&line, state),
        });
        if consumed >= MAX_BYTES_PER_CYCLE {
            break;
        }
    }
    Ok(pending)
}

/// Every `*.jsonl` under the given roots. Missing roots are normal: most
/// machines have only one or two of these hosts installed.
fn discover(roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = roots
        .iter()
        .filter(|root| root.exists())
        .flat_map(|root| {
            walkdir::WalkDir::new(root)
                .follow_links(false)
                .into_iter()
                .filter_map(std::result::Result::ok)
                .filter(|entry| entry.file_type().is_file())
                .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "jsonl"))
                .map(|entry| entry.into_path())
        })
        .collect();
    files.sort();
    files.dedup();
    files
}

fn default_roots() -> Vec<PathBuf> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    DEFAULT_ROOTS.iter().map(|dir| home.join(dir)).collect()
}

fn load_state(path: &Path) -> WatchState {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

/// Write through a temporary file so an interrupted save cannot leave a
/// truncated state file, which would replay or skip turns on the next run.
fn save_state(path: &Path, state: &WatchState) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("json.tmp");
    std::fs::write(&temp, serde_json::to_vec_pretty(state)?)?;
    std::fs::rename(&temp, path)?;
    Ok(())
}

/// Outcome of one send. A rejected event is dropped instead of retried: a line
/// the service refuses would otherwise block the file forever.
enum Sent {
    Stored,
    Rejected(String),
}

async fn post_event(client: &reqwest::Client, base_url: &str, event: &Event) -> Result<Sent> {
    let (document, truncated) = event.document();
    let metadata = Metadata {
        chunk_type: ChunkType::AutoLog,
        importance: Importance::Log,
        session_id: event.session_id.clone(),
        cwd: event.cwd.clone(),
        source: Some(format!("{}.transcript", event.host)),
        adapter: Some(event.host.to_string()),
        adapter_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        role: Some(event.role.to_string()),
        event_id: event.event_id.clone(),
        truncated,
        ..Default::default()
    };
    let mut request = client
        .post(format!("{}/add", base_url.trim_end_matches('/')))
        .json(&json!({
            "text": document,
            "project": event.project(),
            "metadata": metadata,
        }));
    if let Ok(token) = std::env::var("MEMNEST_TOKEN")
        && !token.trim().is_empty()
    {
        request = request.bearer_auth(token.trim());
    }

    let response = request.send().await.context("service unreachable")?;
    let status = response.status();
    if status.is_success() {
        Ok(Sent::Stored)
    } else if status.is_client_error() {
        Ok(Sent::Rejected(status.to_string()))
    } else {
        // 5xx is the service having a bad moment; the turn is worth retrying.
        Err(anyhow!("service returned {status}"))
    }
}

/// Send one file's pending turns, advancing the offset only past turns that
/// were actually accepted so a failure costs a retry rather than a memory.
async fn drain_file(
    client: &reqwest::Client,
    base_url: &str,
    path: &Path,
    state: &mut FileState,
) -> Result<usize> {
    let pending = read_pending(path, state)?;
    let mut stored = 0;
    for item in pending {
        if let Some(event) = &item.event {
            match post_event(client, base_url, event).await {
                Ok(Sent::Stored) => stored += 1,
                Ok(Sent::Rejected(status)) => {
                    tracing::warn!("watch: {} rejected a turn ({status})", path.display());
                }
                Err(e) => return Err(e),
            }
        }
        state.offset = item.end_offset;
    }
    Ok(stored)
}

/// One pass over every transcript. Returns the number of turns stored.
async fn sweep(
    client: &reqwest::Client,
    base_url: &str,
    roots: &[PathBuf],
    state: &mut WatchState,
    backfill: bool,
) -> usize {
    let mut stored = 0;
    for path in discover(roots) {
        let key = path.to_string_lossy().to_string();
        let known = state.files.contains_key(&key);
        let entry = state.files.entry(key).or_default();
        if !known && !backfill {
            // First sighting: follow from the end, like `tail -f`, so enabling
            // the watcher does not import every past session.
            entry.offset = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            if let Some((session_id, cwd)) = scan_header(&path) {
                entry.session_id = session_id;
                entry.cwd = cwd;
            }
            continue;
        }
        match drain_file(client, base_url, &path, entry).await {
            Ok(count) => stored += count,
            Err(e) => {
                // Almost always the service being down. Keep the offset where
                // it is and try the same turns again next cycle.
                tracing::warn!("watch: paused on {} ({e:#})", path.display());
            }
        }
    }
    stored
}

/// Entry point for the `watch` subcommand.
pub async fn run(
    url: Option<&str>,
    paths: &[String],
    state_dir: &Path,
    once: bool,
    interval_secs: u64,
    backfill: bool,
) -> Result<()> {
    let base_url = crate::hook::resolve_url(url);
    let roots: Vec<PathBuf> = if paths.is_empty() {
        default_roots()
    } else {
        paths.iter().map(PathBuf::from).collect()
    };
    if roots.is_empty() {
        return Err(anyhow!(
            "no transcript directory to watch; pass --path <dir>"
        ));
    }

    let state_path = state_dir.join(STATE_FILE);
    let mut state = load_state(&state_path);
    state.version = 1;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()?;
    let interval = Duration::from_secs(interval_secs.max(1));

    tracing::info!(
        "watch: {} -> {base_url} (state {})",
        roots
            .iter()
            .map(|root| root.display().to_string())
            .collect::<Vec<_>>()
            .join(", "),
        state_path.display()
    );

    loop {
        let stored = sweep(&client, &base_url, &roots, &mut state, backfill).await;
        if stored > 0 {
            tracing::info!("watch: stored {stored} turn(s)");
        }
        if let Err(e) = save_state(&state_path, &state) {
            tracing::warn!("watch: could not save progress ({e:#})");
        }
        if once {
            return Ok(());
        }
        tokio::time::sleep(interval).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// Long enough to clear MIN_TEXT_CHARS without being a real conversation.
    const BODY: &str = "the deploy port is eight three two zero";

    fn claude_line(role: &str, text: &str) -> String {
        json!({
            "type": role,
            "uuid": "u-1",
            "sessionId": "s-1",
            "cwd": "/home/dev/acme",
            "isSidechain": false,
            "message": { "role": role, "content": [{ "type": "text", "text": text }] }
        })
        .to_string()
    }

    fn write_lines(path: &Path, lines: &[String]) {
        let mut file = std::fs::File::create(path).unwrap();
        for line in lines {
            writeln!(file, "{line}").unwrap();
        }
    }

    fn append_lines(path: &Path, lines: &[String]) {
        let mut file = std::fs::OpenOptions::new().append(true).open(path).unwrap();
        for line in lines {
            writeln!(file, "{line}").unwrap();
        }
    }

    fn drain(path: &Path, state: &mut FileState) -> Vec<Event> {
        let pending = read_pending(path, state).unwrap();
        let mut events = Vec::new();
        for item in pending {
            if let Some(event) = item.event {
                events.push(event);
            }
            state.offset = item.end_offset;
        }
        events
    }

    #[test]
    fn claude_code_turns_are_parsed_with_identity() {
        let mut state = FileState::default();
        let event = parse_line(&claude_line("user", BODY), &mut state).unwrap();
        assert_eq!(event.host, "claude-code");
        assert_eq!(event.role, "user");
        assert_eq!(event.text, BODY);
        assert_eq!(event.session_id, "s-1");
        assert_eq!(event.cwd.as_deref(), Some("/home/dev/acme"));
        // The engine buckets by cwd basename, not by the whole path.
        assert_eq!(event.project(), "acme");
        assert!(event.document().0.starts_with("User said: "));

        let assistant = parse_line(&claude_line("assistant", BODY), &mut state).unwrap();
        assert_eq!(assistant.role, "assistant");
        assert!(assistant.document().0.starts_with("Assistant answered: "));
    }

    #[test]
    fn pi_turns_take_identity_from_the_session_header() {
        let mut state = FileState::default();
        let header =
            json!({"type":"session","id":"pi-9","cwd":"/home/dev/widgets"}).to_string();
        // The header itself stores nothing but teaches the parser who is talking.
        assert!(parse_line(&header, &mut state).is_none());
        assert_eq!(state.session_id.as_deref(), Some("pi-9"));

        let line = json!({
            "type": "message",
            "id": "m-3",
            "message": { "role": "user", "content": [{ "type": "text", "text": BODY }] }
        })
        .to_string();
        let event = parse_line(&line, &mut state).unwrap();
        assert_eq!(event.host, "pi");
        assert_eq!(event.session_id, "pi-9");
        assert_eq!(event.project(), "widgets");
        assert_eq!(event.event_id.as_deref(), Some("m-3"));

        // A transcript with no cwd anywhere still has to land somewhere.
        let mut bare = FileState::default();
        parse_line(&json!({"type":"session","id":"pi-0"}).to_string(), &mut bare);
        let orphan = parse_line(&line, &mut bare).unwrap();
        assert_eq!(orphan.project(), "root");
    }

    #[test]
    fn machinery_never_becomes_a_memory() {
        let mut state = FileState::default();
        let noise = [
            // Tool output wearing the user role, which is most of a transcript.
            json!({"type":"user","sessionId":"s","cwd":"/tmp/x","message":{"role":"user",
                "content":[{"type":"tool_result","content":"58296 rows returned"}]}}),
            // Reasoning traces are not what anyone said.
            json!({"type":"assistant","sessionId":"s","cwd":"/tmp/x","message":{"role":"assistant",
                "content":[{"type":"thinking","thinking":"weighing the two options here"}]}}),
            json!({"type":"assistant","sessionId":"s","cwd":"/tmp/x","message":{"role":"assistant",
                "content":[{"type":"tool_use","name":"bash","input":{"command":"ls -la"}}]}}),
            // pi keeps tool output under a role of its own.
            json!({"type":"message","id":"m","message":{"role":"toolResult",
                "content":[{"type":"text","text":"exit status 0 and a long tail of output"}]}}),
            // Acknowledgements carry nothing to retrieve on.
            json!({"type":"user","sessionId":"s","cwd":"/tmp/x","message":{"role":"user",
                "content":[{"type":"text","text":"ok"}]}}),
            // Subagent chatter is not the user's conversation.
            json!({"type":"user","sessionId":"s","cwd":"/tmp/x","isSidechain":true,
                "message":{"role":"user","content":[{"type":"text","text":BODY}]}}),
            // Not a conversation line at all.
            json!({"type":"queue-operation","sessionId":"s"}),
        ];
        for line in noise {
            assert!(
                parse_line(&line.to_string(), &mut state).is_none(),
                "stored machinery: {line}"
            );
        }

        // Injected context is stripped, and what is left decides the outcome.
        let reminder = format!("<system-reminder>{BODY}</system-reminder>");
        assert!(parse_line(&claude_line("user", &reminder), &mut state).is_none());
        let mixed = format!("<system-reminder>ignore this</system-reminder>{BODY}");
        let kept = parse_line(&claude_line("user", &mixed), &mut state).unwrap();
        assert_eq!(kept.text, BODY);
        assert_eq!(strip_reminders("a<system-reminder>b"), "a");
    }

    #[test]
    fn a_resumed_read_starts_where_the_last_send_stopped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("session.jsonl");
        write_lines(&path, &[claude_line("user", BODY)]);

        let mut state = FileState::default();
        assert_eq!(drain(&path, &mut state).len(), 1);
        let after_first = state.offset;
        assert!(after_first > 0);

        // Nothing new means nothing re-sent, which is the whole point.
        assert!(drain(&path, &mut state).is_empty());

        append_lines(&path, &[claude_line("assistant", BODY)]);
        assert_eq!(drain(&path, &mut state).len(), 1);
        assert!(state.offset > after_first);

        // A restart reads the offset back from disk and stays put.
        let state_path = dir.path().join(STATE_FILE);
        let mut saved = WatchState::default();
        saved
            .files
            .insert(path.to_string_lossy().to_string(), state.clone());
        save_state(&state_path, &saved).unwrap();
        let reloaded = load_state(&state_path);
        let mut restored = reloaded.files[&path.to_string_lossy().to_string()].clone();
        assert_eq!(restored.offset, state.offset);
        assert!(drain(&path, &mut restored).is_empty());
    }

    #[test]
    fn a_half_written_line_waits_for_its_newline() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("partial.jsonl");
        write_lines(&path, &[claude_line("user", BODY)]);
        let mut state = FileState::default();
        drain(&path, &mut state);
        let settled = state.offset;

        // A transcript is appended to while we read it; consuming a fragment
        // now would drop the rest of the line forever.
        let mut file = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        write!(file, "{{\"type\":\"user\",\"sess").unwrap();
        file.flush().unwrap();
        assert!(drain(&path, &mut state).is_empty());
        assert_eq!(state.offset, settled);

        // Once the writer finishes the line, it is read normally.
        writeln!(file, "ionId\":\"s-1\",\"cwd\":\"/home/dev/acme\",\"message\":{{\"role\":\"user\",\"content\":\"{BODY}\"}}}}").unwrap();
        file.flush().unwrap();
        let events = drain(&path, &mut state);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].text, BODY);
    }

    #[test]
    fn a_shrunken_file_is_reread_from_the_start() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("rotated.jsonl");
        write_lines(
            &path,
            &[claude_line("user", BODY), claude_line("assistant", BODY)],
        );
        let mut state = FileState::default();
        assert_eq!(drain(&path, &mut state).len(), 2);

        // Rotation leaves a shorter file, so the stored offset now points past
        // the end and would silently read nothing forever.
        write_lines(&path, &[claude_line("user", BODY)]);
        let events = drain(&path, &mut state);
        assert_eq!(events.len(), 1);
        assert_eq!(state.offset, std::fs::metadata(&path).unwrap().len());
    }

    #[test]
    fn identity_is_recovered_when_reading_resumes_past_the_header() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("pi.jsonl");
        let message = json!({
            "type": "message",
            "id": "m-1",
            "message": { "role": "assistant", "content": [{ "type": "text", "text": BODY }] }
        })
        .to_string();
        write_lines(
            &path,
            &[
                json!({"type":"session","id":"pi-42","cwd":"/home/dev/acme"}).to_string(),
                message.clone(),
            ],
        );

        // A fresh process knows the offset but not who was talking, because the
        // header sits behind it.
        let mut state = FileState {
            offset: std::fs::metadata(&path).unwrap().len(),
            ..Default::default()
        };
        append_lines(&path, &[message]);
        let events = drain(&path, &mut state);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].session_id, "pi-42");
        assert_eq!(events[0].project(), "acme");
    }

    #[test]
    fn long_turns_are_clipped_in_characters() {
        let (short, truncated) = clip("짧은 문장", MAX_TEXT_CHARS);
        assert_eq!(short, "짧은 문장");
        assert!(!truncated);

        // Counting bytes would cut a Korean turn at a third of the budget.
        let long: String = "한".repeat(MAX_TEXT_CHARS + 10);
        let (clipped, truncated) = clip(&long, MAX_TEXT_CHARS);
        assert!(truncated);
        assert!(clipped.starts_with(&"한".repeat(MAX_TEXT_CHARS)));
        assert!(clipped.ends_with("[truncated]"));
    }

    #[test]
    fn discovery_finds_transcripts_and_ignores_everything_else() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("project-a");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("a.jsonl"), "").unwrap();
        std::fs::write(dir.path().join("notes.md"), "").unwrap();
        std::fs::write(dir.path().join("state.json"), "").unwrap();

        let found = discover(&[dir.path().to_path_buf(), dir.path().join("missing")]);
        assert_eq!(found, vec![nested.join("a.jsonl")]);
    }
}
