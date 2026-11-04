//! Task cards: handoff notes for work that spans sessions, for every host.
//!
//! The working model writes a card's body with the `task_card` tool. The user's
//! own messages are appended to the open card verbatim by the prompt hook, never
//! by the model, so a weak summarizer cannot drop or reverse what the user said.
//! Cards are ordinary Manual memories: active ones in `task-cards`, finished
//! ones in `task-cards-done`, so session-start lookups only scan work in progress.
//!
//! Hosts reach this through three doors with the same storage:
//! - MCP `task_card`: list, get, save, done, close. Any MCP client.
//! - `memnest hook` (`POST /task-card/hook`): for hosts with SessionStart and
//!   UserPromptSubmit hooks (Claude Code, Codex). Lists cards on the first
//!   prompt, records user messages into the open card, and reloads open cards
//!   after compaction.
//! - The pi extension keeps its own copy of this logic so it works with older
//!   cores; it reads and writes the same card format.
//!
//! Hook and MCP calls arrive on separate channels, so the hook hands the model a
//! session id to pass back with `task_card`. That id keys the open cards here.

use axum::{
    Json,
    extract::State,
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use tokio::sync::RwLock;

use super::operations::{self, OperationError, RememberInput, SearchInput};
use crate::MemorySystem;
use crate::models::{ChunkType, Importance, MemoryKind, Metadata};

type System = Arc<RwLock<MemorySystem>>;
type Result<T> = std::result::Result<T, OperationError>;

const PROJECT: &str = "task-cards";
const DONE_PROJECT: &str = "task-cards-done";
const MARK: &str = "[memnest-task-card]";
// The search index drops bracketed queries, so lookups search the bare word.
const ENTRY: &str = "memnest-task-card";
const LOG: &str = "## User's own words (kept verbatim by memnest, oldest first)";
const TRIMMED: &str = "- (older messages trimmed; memory_search finds them)";
const LOG_MAX: usize = 8000;
const MESSAGE_MAX: usize = 1000;
const MAX_CHARS: usize = 12000;
const MAX_OPEN: usize = 3;
const LIST: usize = 20;
const PAGE: usize = 50;
const PENDING_MAX: usize = 20;
/// Codex shows about 2,500 tokens of one hook output; stay near that.
const COMPACT_MAX: usize = 6000;
const MAX_SESSIONS: usize = 1000;

/// Measured on a real long session; see docs/task-cards.md.
pub const RULE: &str = "The user's stated policies and prohibitions for this task take priority over your own analysis and earlier plans. If a proposal would go against one, first quote the user's words it conflicts with and ask before proposing it. The card's next steps are a plan, not approval: before changing, submitting, publishing, or deleting anything the user has not approved in this conversation, say what you will do and ask.";
const FORMAT: &str = "Sections: Goal; User policies and prohibitions (dated, close to the user's words, note repeats); Decisions and reasons; Done (what, where); Tried and failed (why); Current state (numbers); Next and waiting; Unverified. Keep what is still valid, move finished items to Done, no tool output, no guesses, under about 6000 characters. Do not copy the user's-own-words section; memnest keeps it.";

pub fn tool_schema() -> Value {
    json!({
        "name": "task_card",
        "description": format!("Handoff card for work that continues across sessions or days. get opens a card; save writes the newest body and replaces the previous version with the same title. When a memnest hook gave you a session id, pass it as session: memnest then records every user message into the open card verbatim and reloads it after compaction, and close stops that when the user moves on to unrelated work. Up to {MAX_OPEN} cards stay open per session. Create a card when a task will continue in a later session; after that, save when work progressed or a decision changed the plan, and when the user stops for the day, not after every message. done moves a finished task out of the list; get still opens it and saving it again reopens it. list with text searches cards by topic. {FORMAT}"),
        "inputSchema": {"type": "object", "properties": {
            "action": {"type": "string", "enum": ["list", "get", "save", "done", "close"]},
            "title": {"type": "string", "description": "Short task name; the same title updates the same card."},
            "text": {"type": "string", "description": "Card body for save (without the user's-own-words section), or a topic to search for list."},
            "session": {"type": "string", "description": "Session id from a memnest hook reminder, if one was given."},
            "cwd": {"type": "string", "description": "Absolute working directory, recorded as provenance."}
        }, "required": ["action"]}
    })
}

#[derive(Clone)]
struct Card {
    id: String,
    title: String,
    updated: String,
}

fn line(card: &Card) -> String {
    let updated = if card.updated.is_empty() {
        "unknown"
    } else {
        &card.updated
    };
    format!("- {} (updated {updated})", card.title)
}

fn title_of(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn clip(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}

/// Cards ranked by relevance to `request`, or newest first without one.
async fn find(system: &System, request: &str, project: &str) -> Vec<Card> {
    let request = request.trim();
    let query = if request.is_empty() {
        ENTRY.to_string()
    } else {
        format!("{ENTRY} {}", clip(request, 300))
    };
    let Ok(out) = operations::search(
        system.clone(),
        SearchInput {
            query,
            project: project.to_string(),
            cwd: None,
            n_results: PAGE,
            recent_first: request.is_empty(),
            evidence_only: false,
            durable_only: false,
            category: None,
            exclude_reserved: false,
            adapter: "task-card".into(),
        },
    )
    .await
    else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    let mut cards = Vec::new();
    for item in out.results {
        let Some(rest) = item.document.strip_prefix(MARK) else {
            continue;
        };
        let title = title_of(rest.lines().next().unwrap_or_default());
        if title.is_empty() || !seen.insert(title.clone()) {
            continue;
        }
        cards.push(Card {
            id: item.id,
            title,
            updated: clip(&item.timestamp, 16),
        });
    }
    cards
}

/// Exact title: newest page first, then a title search for older cards.
async fn by_title(system: &System, title: &str, project: &str) -> Option<Card> {
    if let Some(card) = find(system, "", project)
        .await
        .into_iter()
        .find(|c| c.title == title)
    {
        return Some(card);
    }
    find(system, title, project)
        .await
        .into_iter()
        .find(|c| c.title == title)
}

/// Raw card text without the marker line; internal rewrites must not see redaction.
async fn read(system: &System, id: &str) -> Option<String> {
    let sys = system.read().await;
    let db = sys.db.read().await;
    let chunk = db.get_chunk(id).ok()??;
    crate::access::require_read_chunk(&chunk).ok()?;
    if chunk.project != PROJECT && chunk.project != DONE_PROJECT {
        return None;
    }
    Some(
        chunk
            .document
            .split_once('\n')
            .map(|(_, body)| body.trim().to_string())
            .unwrap_or_default(),
    )
}

/// A card is the model's body plus memnest's verbatim log of user messages.
fn split(text: &str) -> (String, Vec<String>) {
    match text.find(LOG) {
        None => (text.trim().to_string(), Vec::new()),
        Some(at) => (
            text[..at].trim().to_string(),
            text[at + LOG.len()..]
                .lines()
                .filter(|l| l.starts_with("- "))
                .map(str::to_string)
                .collect(),
        ),
    }
}

fn compose(body: &str, log: &[String]) -> String {
    let mut dropped = log.iter().any(|l| l == TRIMMED);
    let mut kept: Vec<&String> = log.iter().filter(|l| *l != TRIMMED).collect();
    let size = |kept: &[&String]| kept.iter().map(|l| l.chars().count() + 1).sum::<usize>();
    while size(&kept) > LOG_MAX && kept.len() > 1 {
        kept.remove(0);
        dropped = true;
    }
    if kept.is_empty() {
        return body.to_string();
    }
    let kept: Vec<&str> = kept.iter().map(|s| s.as_str()).collect();
    format!(
        "{body}\n\n{LOG}\n{}{}",
        if dropped {
            format!("{TRIMMED}\n")
        } else {
            String::new()
        },
        kept.join("\n")
    )
}

fn stamp() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M").to_string()
}

fn entry(prompt: &str) -> String {
    format!("- {} {}", stamp(), clip(&title_of(prompt), MESSAGE_MAX))
}

async fn set_text_or_project(
    system: &System,
    id: &str,
    text: Option<String>,
    project: Option<&str>,
) -> Result<()> {
    operations::update(
        system.clone(),
        crate::server::api::UpdateRequest {
            id: id.to_string(),
            sensitive: None,
            text,
            project: project.map(str::to_string),
            metadata: None,
            chunk_type: None,
            importance: None,
        },
    )
    .await
    .map(|_| ())
}

/// Card rewrites inside this process run one at a time, so a log append never
/// overwrites another session's append. The pi extension writes through
/// `/update` and re-checks its own writes.
fn writes() -> &'static tokio::sync::Mutex<()> {
    static WRITES: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
    WRITES.get_or_init(|| tokio::sync::Mutex::new(()))
}

async fn append(system: &System, title: &str, entries: &[String]) -> Result<()> {
    if entries.is_empty() {
        return Ok(());
    }
    let _write = writes().lock().await;
    let Some(card) = by_title(system, title, PROJECT).await else {
        return Ok(());
    };
    let Some(text) = read(system, &card.id).await else {
        return Ok(());
    };
    let (body, mut log) = split(&text);
    for e in entries {
        if !log.contains(e) {
            log.push(e.clone());
        }
    }
    let text = format!("{MARK} {title}\n{}", compose(&body, &log));
    set_text_or_project(system, &card.id, Some(text), None).await
}

async fn save(
    system: &System,
    title: &str,
    text: &str,
    extra: &[String],
    cwd: Option<String>,
) -> Result<(String, Option<String>)> {
    // The model may echo the pinned card; the log section is never taken from it.
    let (body, _) = split(text);
    if body.is_empty() {
        return Err(OperationError::bad("save needs title and text"));
    }
    let size = body.chars().count();
    if size > MAX_CHARS {
        return Err(OperationError::bad(format!(
            "card is {size} characters; shorten it below {MAX_CHARS} and save again"
        )));
    }
    let _write = writes().lock().await;
    let mut previous = by_title(system, title, PROJECT).await;
    if previous.is_none() {
        // Saving a finished task reopens it: move it back so the new version can supersede it.
        if let Some(finished) = by_title(system, title, DONE_PROJECT).await
            && set_text_or_project(system, &finished.id, None, Some(PROJECT))
                .await
                .is_ok()
        {
            previous = Some(finished);
        }
    }
    let stored = match &previous {
        Some(card) => split(&read(system, &card.id).await.unwrap_or_default()).1,
        None => Vec::new(),
    };
    let mut log = stored.clone();
    log.extend(extra.iter().filter(|e| !stored.contains(e)).cloned());
    let out = operations::remember(
        system.clone(),
        RememberInput {
            text: format!("{MARK} {title}\n{}", compose(&body, &log)),
            project: PROJECT.into(),
            cwd: None,
            metadata: Some(Metadata {
                chunk_type: ChunkType::Manual,
                importance: Importance::Decision,
                memory_kind: MemoryKind::Record,
                supersedes: previous.as_ref().map(|c| c.id.clone()),
                cwd,
                adapter: Some("task-card".into()),
                ..Metadata::default()
            }),
            sensitive: false,
        },
    )
    .await?;
    Ok((out.id, previous.map(|c| c.id)))
}

#[derive(Default)]
struct Session {
    open: Vec<String>,
    pending: Vec<String>,
    listed: bool,
    seen: Option<Instant>,
}

/// shortcut: open cards live in process memory; a core restart forgets them and the next prompt lists cards again.
fn with_session<T>(id: &str, f: impl FnOnce(&mut Session) -> T) -> T {
    static SESSIONS: OnceLock<Mutex<HashMap<String, Session>>> = OnceLock::new();
    let mut map = SESSIONS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    if map.len() >= MAX_SESSIONS && !map.contains_key(id) {
        let oldest = map
            .iter()
            .min_by_key(|(_, s)| s.seen)
            .map(|(k, _)| k.clone());
        if let Some(oldest) = oldest {
            map.remove(&oldest);
        }
    }
    let session = map.entry(id.to_string()).or_default();
    session.seen = Some(Instant::now());
    f(session)
}

fn open_in(session: &mut Session, title: &str) {
    session.open.retain(|t| t != title);
    session.open.push(title.to_string());
    if session.open.len() > MAX_OPEN {
        session.open.remove(0);
    }
    session.listed = true;
}

fn arg(args: &Value, key: &str) -> String {
    args.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

/// The `task_card` tool, shared by MCP and `POST /task-card`.
pub async fn tool(system: &System, args: &Value) -> Result<String> {
    let action = arg(args, "action");
    let title = title_of(&arg(args, "title"));
    let session = Some(arg(args, "session")).filter(|s| !s.is_empty());
    let needs_title = matches!(action.as_str(), "get" | "save" | "done" | "close");
    if needs_title && title.is_empty() {
        return Err(OperationError::bad(format!("{action} needs a title")));
    }
    match action.as_str() {
        "list" => {
            let cards = find(system, &arg(args, "text"), PROJECT).await;
            Ok(if cards.is_empty() {
                "no task cards are saved; treat this as a new task and do not search again".into()
            } else {
                cards.iter().map(line).collect::<Vec<_>>().join("\n")
            })
        }
        "get" => {
            let active = by_title(system, &title, PROJECT).await;
            let card = match &active {
                Some(card) => Some(card.clone()),
                None => by_title(system, &title, DONE_PROJECT).await,
            };
            let Some(card) = card else {
                let near: Vec<String> = find(system, &title, PROJECT)
                    .await
                    .iter()
                    .take(5)
                    .map(line)
                    .collect();
                let hint = if near.is_empty() {
                    String::new()
                } else {
                    format!("; closest titles:\n{}", near.join("\n"))
                };
                return Err(OperationError::not_found(format!(
                    "no task card titled \"{title}\"{hint}"
                )));
            };
            if let (Some(session), Some(_)) = (&session, &active) {
                // Only the request that picked the task up; earlier ones may be other work.
                let last = with_session(session, |s| {
                    let last = s.pending.pop();
                    s.pending.clear();
                    open_in(s, &title);
                    last
                });
                append(system, &title, &last.into_iter().collect::<Vec<_>>()).await?;
            }
            let text = read(system, &card.id).await;
            let latest = match by_title(system, &title, PROJECT).await {
                Some(c) if active.is_some() => read(system, &c.id).await,
                _ => text,
            };
            let text =
                latest.ok_or_else(|| OperationError::internal("could not read the task card"))?;
            let (body, log) = split(&text);
            let note = if active.is_some() {
                ""
            } else {
                "(this task was marked done; save it again to reopen)\n\n"
            };
            Ok(format!("{note}{RULE}\n\n{}", compose(&body, &log)))
        }
        "save" => {
            let extra = session
                .as_deref()
                .map(|s| with_session(s, |s| std::mem::take(&mut s.pending)))
                .unwrap_or_default();
            let cwd = Some(arg(args, "cwd")).filter(|c| !c.is_empty());
            let (id, replaced) = save(system, &title, &arg(args, "text"), &extra, cwd).await?;
            if let Some(session) = &session {
                with_session(session, |s| open_in(s, &title));
            }
            Ok(match replaced {
                Some(old) => format!("saved task card \"{title}\" as {id} (replaces {old})"),
                None => format!("saved task card \"{title}\" as {id}"),
            })
        }
        "done" => {
            let _write = writes().lock().await;
            let card = by_title(system, &title, PROJECT).await.ok_or_else(|| {
                OperationError::not_found(format!("no active task card titled \"{title}\""))
            })?;
            // Moving the record keeps its id and text; only the active collection stays small.
            set_text_or_project(system, &card.id, None, Some(DONE_PROJECT)).await?;
            if let Some(session) = &session {
                with_session(session, |s| s.open.retain(|t| t != &title));
            }
            Ok(format!(
                "marked task card \"{title}\" done; it no longer appears in the list"
            ))
        }
        "close" => {
            let session = session.ok_or_else(|| {
                OperationError::bad("close needs the session id from the memnest hook")
            })?;
            let was_open = with_session(&session, |s| {
                let was = s.open.contains(&title);
                s.open.retain(|t| t != &title);
                s.pending.clear();
                was
            });
            if !was_open {
                return Err(OperationError::bad(format!(
                    "task card \"{title}\" is not open"
                )));
            }
            Ok(format!(
                "closed task card \"{title}\"; it is no longer reloaded or recorded into"
            ))
        }
        _ => Err(OperationError::bad(
            "action must be list, get, save, done, or close",
        )),
    }
}

/// Context for one host hook event; empty when there is nothing to add.
pub async fn hook(
    system: &System,
    event: &str,
    session: &str,
    prompt: &str,
    source: &str,
) -> String {
    let session = session.trim();
    if session.is_empty() {
        return String::new();
    }
    match event {
        "SessionStart" if source == "clear" => {
            with_session(session, |s| *s = Session::default());
            String::new()
        }
        "SessionStart" if source == "compact" => reload_after_compaction(system, session).await,
        "UserPromptSubmit" => on_prompt(system, session, prompt).await,
        _ => String::new(),
    }
}

async fn on_prompt(system: &System, session: &str, prompt: &str) -> String {
    let prompt = prompt.trim();
    if prompt.is_empty() || prompt.starts_with('/') {
        return String::new();
    }
    let said = entry(prompt);
    let (current, list) = with_session(session, |s| {
        let current = s.open.last().cloned();
        if current.is_none() {
            s.pending.push(said.clone());
            if s.pending.len() > PENDING_MAX {
                s.pending.remove(0);
            }
        }
        let list = current.is_none() && !s.listed;
        s.listed = true;
        (current, list)
    });
    if let Some(title) = current {
        let _ = append(system, &title, &[said]).await;
        return String::new();
    }
    if !list {
        return String::new();
    }
    let (matching, recent) = tokio::join!(find(system, prompt, PROJECT), find(system, "", PROJECT));
    let mut cards = matching.clone();
    cards.extend(
        recent
            .iter()
            .filter(|c| !matching.iter().any(|m| m.title == c.title))
            .cloned(),
    );
    cards.truncate(LIST);
    if cards.is_empty() {
        // Hosts like Claude Code defer MCP tool descriptions, so without this line the
        // model never learns that cards exist or which session id to pass.
        return format!(
            "[memnest task cards] No saved task cards. If this request starts work that will continue in a later session, create one with the task_card tool (action=save, session=\"{session}\"); memnest then keeps the user's messages in it verbatim."
        );
    }
    let order = if matching.is_empty() {
        "newest first"
    } else {
        "most relevant to this request first"
    };
    let latest = recent
        .first()
        .map(|c| format!("\nMost recently saved: {}", c.title))
        .unwrap_or_default();
    let crowded = if recent.len() >= PAGE {
        format!(
            "\n{PAGE} or more active cards. When it fits the conversation, ask the user which finished tasks to mark done."
        )
    } else {
        String::new()
    };
    format!(
        "[memnest task cards] Saved handoff cards, {order}. Open one with the task_card tool (action=get, session=\"{session}\") when the request is about that task: continuing it or asking about it. A new task that only shares a word or an area with a card is not about it, so answer without opening a card. If the user is continuing something but it is unclear which task, ask.\n{}{latest}{crowded}",
        cards.iter().map(line).collect::<Vec<_>>().join("\n")
    )
}

/// Compaction summarizes the transcript, card included. Bring back the rule, the
/// user's latest words, and the start of the body, and ask for a full reload.
async fn reload_after_compaction(system: &System, session: &str) -> String {
    let open = with_session(session, |s| s.open.clone());
    if open.is_empty() {
        return String::new();
    }
    let budget = COMPACT_MAX / open.len();
    let mut blocks = Vec::new();
    for title in open.iter().rev() {
        let Some(card) = by_title(system, title, PROJECT).await else {
            continue;
        };
        let (body, log) = split(&read(system, &card.id).await.unwrap_or_default());
        let mut words: Vec<&String> = Vec::new();
        let mut used = 0;
        for l in log.iter().rev() {
            used += l.chars().count() + 1;
            if used > budget / 2 {
                break;
            }
            words.insert(0, l);
        }
        let words: Vec<&str> = words.iter().map(|s| s.as_str()).collect();
        blocks.push(format!(
            "### {title}\nReload the full card now with the task_card tool (action=get, title=\"{title}\", session=\"{session}\"). If decisions or user corrections since its last save are missing, save it afterwards.\nUser's latest words, verbatim:\n{}\nCard start:\n{}",
            words.join("\n"),
            clip(&body, budget / 2)
        ));
    }
    if blocks.is_empty() {
        return String::new();
    }
    format!(
        "[memnest task cards] The conversation was just compacted. These cards were open.\n{RULE}\n\n{}",
        blocks.join("\n\n")
    )
}

/// `POST /task-card`: the tool over plain HTTP for adapters without MCP.
pub async fn endpoint(State(system): State<System>, Json(args): Json<Value>) -> Response {
    match tool(&system, &args).await {
        Ok(text) => Json(json!({"status": "ok", "text": text})).into_response(),
        Err(error) => crate::server::api::operation_error_response(error),
    }
}

/// `POST /task-card/hook`: what `memnest hook` asks for on each host event.
pub async fn hook_endpoint(State(system): State<System>, Json(req): Json<Value>) -> Json<Value> {
    let context = hook(
        &system,
        &arg(&req, "event"),
        &arg(&req, "session_id"),
        req.get("prompt")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        &arg(&req, "source"),
    )
    .await;
    Json(json!({"context": context}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::server::api::test_support::build_system;

    #[test]
    fn log_is_split_composed_and_bounded() {
        let log: Vec<String> = (0..30)
            .map(|i| format!("- 2026-11-03 10:{i:02} {}", "가".repeat(400)))
            .collect();
        let text = compose("body", &log);
        assert!(text.starts_with("body\n\n## User's own words"));
        assert!(text.contains(TRIMMED));
        let (body, kept) = split(&text);
        assert_eq!(body, "body");
        assert!(kept.last().unwrap().contains("10:29"));
        assert!(
            compose("body", &kept).matches(TRIMMED).count() == 1,
            "the trim marker is not duplicated"
        );
        assert_eq!(compose("only", &[]), "only");
    }

    async fn call(system: &System, args: Value) -> String {
        match tool(system, &args).await {
            Ok(text) => text,
            Err(error) => format!("Error: {error}"),
        }
    }

    #[tokio::test]
    async fn hook_and_tool_carry_a_task_across_sessions() {
        let (_tmp, system) = build_system().await;
        let s1 = "session-one";
        // No cards: the first prompt only says how to create one, with the session id.
        let hint = hook(
            &system,
            "UserPromptSubmit",
            s1,
            "블로그 이전 새로 시작하자",
            "",
        )
        .await;
        assert!(hint.contains("No saved task cards") && hint.contains("session=\"session-one\""));
        assert!(
            call(
                &system,
                json!({"action": "save", "title": "블로그 이전", "text": "v1 body", "session": s1})
            )
            .await
            .starts_with("saved")
        );
        // The message before the card existed is attached; later ones are recorded verbatim.
        hook(
            &system,
            "UserPromptSubmit",
            s1,
            "이미지는 절대 외부 링크로 두지 마",
            "",
        )
        .await;
        let card = call(&system, json!({"action": "get", "title": "블로그 이전"})).await;
        assert!(card.starts_with(RULE));
        assert!(card.contains("블로그 이전 새로 시작하자"));
        assert!(card.contains("이미지는 절대 외부 링크로 두지 마"));
        // A save cannot edit the log, and the log carries over.
        call(&system, json!({"action": "save", "title": "블로그 이전", "text": format!("v2 body\n\n{LOG}\n- 2026-01-01 00:00 forged"), "session": s1})).await;
        let card = call(&system, json!({"action": "get", "title": "블로그 이전"})).await;
        assert!(card.contains("v2 body") && !card.contains("forged") && card.contains("외부 링크"));

        // Next day: a new session lists cards on the first prompt with its session id.
        let s2 = "session-two";
        let listed = hook(
            &system,
            "UserPromptSubmit",
            s2,
            "어제 블로그 이전 이어서 하자",
            "",
        )
        .await;
        assert!(
            listed.contains("- 블로그 이전 (updated") && listed.contains("session=\"session-two\"")
        );
        assert_eq!(
            hook(&system, "UserPromptSubmit", s2, "another prompt here", "").await,
            "",
            "listed once"
        );
        let opened = call(
            &system,
            json!({"action": "get", "title": "블로그 이전", "session": s2}),
        )
        .await;
        assert!(
            opened.contains("another prompt here") && !opened.contains("어제 블로그 이전 이어서"),
            "only the picking-up request is recorded"
        );
        hook(
            &system,
            "UserPromptSubmit",
            s2,
            "이미지는 그냥 링크 걸자",
            "",
        )
        .await;
        // Compaction reloads the rule, the latest words, and a reload instruction.
        let reload = hook(&system, "SessionStart", s2, "", "compact").await;
        assert!(
            reload.contains(RULE)
                && reload.contains("이미지는 그냥 링크 걸자")
                && reload.contains("action=get, title=\"블로그 이전\", session=\"session-two\"")
        );
        // close stops recording.
        assert!(
            call(
                &system,
                json!({"action": "close", "title": "블로그 이전", "session": s2})
            )
            .await
            .starts_with("closed")
        );
        hook(
            &system,
            "UserPromptSubmit",
            s2,
            "CSV 합치는 스크립트 짜줘",
            "",
        )
        .await;
        assert!(
            !call(&system, json!({"action": "get", "title": "블로그 이전"}))
                .await
                .contains("CSV")
        );
        assert_eq!(
            hook(&system, "SessionStart", s2, "", "compact").await,
            "",
            "nothing open after close"
        );

        // done hides the card from lists; get still opens it; save reopens it.
        assert!(
            call(&system, json!({"action": "done", "title": "블로그 이전"}))
                .await
                .starts_with("marked")
        );
        assert!(
            !call(&system, json!({"action": "list"}))
                .await
                .contains("블로그 이전")
        );
        assert!(
            call(&system, json!({"action": "get", "title": "블로그 이전"}))
                .await
                .starts_with("(this task was marked done")
        );
        assert!(
            call(
                &system,
                json!({"action": "save", "title": "블로그 이전", "text": "reopened"})
            )
            .await
            .contains("replaces")
        );
        let list = call(&system, json!({"action": "list"})).await;
        assert_eq!(list.matches("- 블로그 이전 (").count(), 1);
        assert!(
            call(&system, json!({"action": "get", "title": "블로그"}))
                .await
                .contains("closest titles")
        );
        assert!(
            call(&system, json!({"action": "close", "title": "블로그 이전"}))
                .await
                .contains("needs the session id")
        );
    }
}
