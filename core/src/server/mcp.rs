use crate::MemorySystem;
use crate::models::{ChunkType, Importance, MemoryChunk, Metadata};
use crate::redaction::redact_text;
use anyhow::Result;
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};
use std::sync::Arc;
use tokio::sync::RwLock;

pub async fn run_stdio(system: Arc<RwLock<MemorySystem>>) -> Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                write_response(
                    &mut stdout,
                    json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":e.to_string()}}),
                )?;
                continue;
            }
        };
        let id = req.get("id").cloned().unwrap_or(Value::Null);
        let method = req.get("method").and_then(Value::as_str).unwrap_or("");
        if method == "shutdown" {
            write_response(&mut stdout, json!({"jsonrpc":"2.0","id":id,"result":null}))?;
            break;
        }
        if method.starts_with("notifications/") {
            continue;
        }
        let result = match method {
            "initialize" => json!({
                "protocolVersion": "2024-11-05",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "memnest", "version": env!("CARGO_PKG_VERSION")}
            }),
            "tools/list" => json!({"tools": tools()}),
            "tools/call" => {
                let params = req.get("params").cloned().unwrap_or_default();
                match call_tool(system.clone(), &params).await {
                    Ok(text) => json!({"content":[{"type":"text","text":text}]}),
                    Err(e) => {
                        json!({"content":[{"type":"text","text":format!("error: {}", e)}],"isError":true})
                    }
                }
            }
            _ => json!({"error": format!("unknown method: {}", method)}),
        };
        write_response(
            &mut stdout,
            json!({"jsonrpc":"2.0","id":id,"result":result}),
        )?;
    }
    Ok(())
}

fn write_response(stdout: &mut io::Stdout, value: Value) -> Result<()> {
    writeln!(stdout, "{}", serde_json::to_string(&value)?)?;
    stdout.flush()?;
    Ok(())
}

fn tools() -> Vec<Value> {
    vec![
        json!({"name": "memory_add", "description": "Save a memory chunk", "inputSchema": {"type":"object","properties":{"text":{"type":"string"},"project":{"type":"string"}},"required":["text"]}}),
        json!({"name": "memory_update", "description": "Update an existing memory chunk by id and refresh indexes", "inputSchema": {"type":"object","properties":{"id":{"type":"string"},"text":{"type":"string"},"project":{"type":"string"},"importance":{"type":"string","enum":["log","knowledge","decision","preference"]},"chunk_type":{"type":"string","enum":["auto_log","manual","filtered","consolidated"]}},"required":["id"]}}),
        json!({"name": "memory_search", "description": "Search memory with hybrid BM25/vector retrieval", "inputSchema": {"type":"object","properties":{"query":{"type":"string"},"project":{"type":"string","default":"all"},"n_results":{"type":"integer","default":10},"recent_first":{"type":"boolean","default":false}},"required":["query"]}}),
        json!({"name": "memory_context", "description": "Return a compact context pack: core notes + matching facts + retrieved memories", "inputSchema": {"type":"object","properties":{"query":{"type":"string"},"project":{"type":"string","default":"all"},"n_results":{"type":"integer","default":6},"max_notes":{"type":"integer","default":12},"max_facts":{"type":"integer","default":8}},"required":["query"]}}),
        json!({"name": "memory_stats", "description": "Return memory system statistics", "inputSchema": {"type":"object","properties":{}}}),
        json!({"name": "memory_facts", "description": "Search structured facts (subject-predicate-object)", "inputSchema": {"type":"object","properties":{"query":{"type":"string"},"max_results":{"type":"integer","default":20}},"required":["query"]}}),
        json!({"name": "memory_sessions", "description": "List recent session summaries", "inputSchema": {"type":"object","properties":{"project":{"type":"string"},"n":{"type":"integer","default":5}},"required":[]}}),
        json!({"name": "note_get", "description": "Get a note by key", "inputSchema": {"type":"object","properties":{"key":{"type":"string"}},"required":["key"]}}),
        json!({"name": "note_set", "description": "Set a note key-value", "inputSchema": {"type":"object","properties":{"key":{"type":"string"},"value":{"type":"string"}},"required":["key","value"]}}),
        json!({"name": "note_delete", "description": "Delete a note by key", "inputSchema": {"type":"object","properties":{"key":{"type":"string"}},"required":["key"]}}),
        json!({"name": "note_list", "description": "List all notes", "inputSchema": {"type":"object","properties":{}}}),
        json!({"name": "server_info", "description": "Get server connection info", "inputSchema": {"type":"object","properties":{"name":{"type":"string"}},"required":[]}}),
        json!({"name": "server_add", "description": "Add a server", "inputSchema": {"type":"object","properties":{"name":{"type":"string"},"host":{"type":"string"},"user":{"type":"string"},"password":{"type":"string"},"port":{"type":"integer","default":22},"note":{"type":"string"}},"required":["name","host","user","password"]}}),
        json!({"name": "server_update", "description": "Update a server field", "inputSchema": {"type":"object","properties":{"name":{"type":"string"},"field":{"type":"string"},"value":{"type":"string"}},"required":["name","field","value"]}}),
        json!({"name": "memory_graph_query", "description": "Query knowledge graph", "inputSchema": {"type":"object","properties":{"node":{"type":"string"},"depth":{"type":"integer","default":2}},"required":["node"]}}),
        json!({"name": "memory_lifecycle_run", "description": "Run memory lifecycle (decay/consolidation)", "inputSchema": {"type":"object","properties":{}}}),
        json!({"name": "memory_session_fork", "description": "Reparent every chunk from one session id onto a new session id + cwd. Mirrors a CLI-level fork (e.g. `pi --fork`) so memory follows the new conversation instead of being orphaned in the source bucket. Set dry_run=true to preview the count without moving.", "inputSchema": {"type":"object","properties":{"from_session_id":{"type":"string"},"to_session_id":{"type":"string"},"to_cwd":{"type":"string","description":"Absolute cwd of the forked session. Project bucket is derived from its basename unless to_project is given."},"to_project":{"type":"string"},"dry_run":{"type":"boolean","default":false}},"required":["from_session_id","to_session_id","to_cwd"]}}),
        json!({"name": "secret_set", "description": "Save a credential (PAT, API key, password) AES-GCM encrypted. Plain value is returned only via secret_get.", "inputSchema": {"type":"object","properties":{"key":{"type":"string"},"value":{"type":"string"},"kind":{"type":"string","description":"free-form classifier e.g. github_pat, openai_key"},"note":{"type":"string"}},"required":["key","value"]}}),
        json!({"name": "secret_get", "description": "Retrieve and decrypt a stored credential by key", "inputSchema": {"type":"object","properties":{"key":{"type":"string"}},"required":["key"]}}),
        json!({"name": "secret_list", "description": "List stored credential keys (values NEVER returned)", "inputSchema": {"type":"object","properties":{}}}),
        json!({"name": "secret_delete", "description": "Delete a stored credential by key", "inputSchema": {"type":"object","properties":{"key":{"type":"string"}},"required":["key"]}}),
    ]
}

async fn call_tool(system: Arc<RwLock<MemorySystem>>, params: &Value) -> Result<String> {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or_default();
    match name {
        "memory_add" => memory_add(system, &args).await,
        "memory_update" => memory_update(system, &args).await,
        "memory_search" => memory_search(system, &args).await,
        "memory_context" => memory_context(system, &args).await,
        "memory_stats" => memory_stats(system).await,
        "memory_facts" => memory_facts(system, &args).await,
        "memory_sessions" => memory_sessions(system, &args).await,
        "note_get" => note_get(system, &args).await,
        "note_set" => note_set(system, &args).await,
        "note_delete" => note_delete(system, &args).await,
        "note_list" => note_list(system).await,
        "server_info" => server_info(system, &args).await,
        "server_add" => server_add(system, &args).await,
        "server_update" => server_update(system, &args).await,
        "memory_graph_query" => memory_graph_query(system, &args).await,
        "memory_lifecycle_run" => memory_lifecycle_run(system).await,
        "memory_session_fork" => memory_session_fork(system, &args).await,
        "secret_set" => secret_set(system, &args).await,
        "secret_get" => secret_get(system, &args).await,
        "secret_list" => secret_list(system).await,
        "secret_delete" => secret_delete(system, &args).await,
        _ => Ok(format!("unknown tool: {}", name)),
    }
}

async fn secret_set(system: Arc<RwLock<MemorySystem>>, args: &Value) -> Result<String> {
    let key = args.get("key").and_then(Value::as_str).unwrap_or("").trim();
    let value = args.get("value").and_then(Value::as_str).unwrap_or("");
    anyhow::ensure!(!key.is_empty(), "key is required");
    anyhow::ensure!(!value.is_empty(), "value is required");
    let kind = args
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let note = args
        .get("note")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let secret = crate::models::Secret {
        key: key.to_string(),
        kind,
        value: value.to_string(),
        note,
        updated: chrono::Utc::now(),
    };
    let sys = system.read().await;
    sys.db.write().await.insert_secret(&secret)?;
    Ok(format!(
        "secret stored encrypted: {} (encryption={})",
        key,
        if crate::crypto::is_enabled() {
            "AES-256-GCM"
        } else {
            "disabled (no master key)"
        }
    ))
}

async fn secret_get(system: Arc<RwLock<MemorySystem>>, args: &Value) -> Result<String> {
    let key = args.get("key").and_then(Value::as_str).unwrap_or("").trim();
    anyhow::ensure!(!key.is_empty(), "key is required");
    let sys = system.read().await;
    let db = sys.db.read().await;
    match db.get_secret(key)? {
        Some(secret) => Ok(format!(
            "{}\n--\nkind: {}\nnote: {}\nupdated: {}",
            secret.value,
            secret.kind,
            secret.note,
            secret.updated.to_rfc3339()
        )),
        None => Ok(format!("secret '{}' not found", key)),
    }
}

async fn secret_list(system: Arc<RwLock<MemorySystem>>) -> Result<String> {
    let sys = system.read().await;
    let db = sys.db.read().await;
    let secrets = db.list_secret_meta()?;
    if secrets.is_empty() {
        return Ok("no secrets stored".to_string());
    }
    let mut lines = vec!["=== secrets (values not shown) ===".to_string()];
    for s in secrets {
        lines.push(format!(
            "  {} [{}] {} (updated {})",
            s.key,
            if s.kind.is_empty() { "-" } else { &s.kind },
            if s.note.is_empty() { "" } else { &s.note },
            s.updated.to_rfc3339()
        ));
    }
    Ok(lines.join("\n"))
}

async fn secret_delete(system: Arc<RwLock<MemorySystem>>, args: &Value) -> Result<String> {
    let key = args.get("key").and_then(Value::as_str).unwrap_or("").trim();
    anyhow::ensure!(!key.is_empty(), "key is required");
    let sys = system.read().await;
    let removed = sys.db.write().await.delete_secret(key)?;
    if removed {
        Ok(format!("secret deleted: {}", key))
    } else {
        Ok(format!("secret '{}' not found", key))
    }
}

async fn memory_add(system: Arc<RwLock<MemorySystem>>, args: &Value) -> Result<String> {
    let raw = args.get("text").and_then(Value::as_str).unwrap_or("");
    anyhow::ensure!(!raw.trim().is_empty(), "text is required");
    let text = redact_text(raw);
    let project = args
        .get("project")
        .and_then(Value::as_str)
        .unwrap_or("default")
        .to_string();

    // Exact-match dedup runs synchronously on the request path: it's a single
    // indexed SELECT and lets us return a clear "duplicate" status instead of
    // silently growing the store. Semantic dedup happens in persist_chunk
    // because it requires the embedding.
    {
        let sys = system.read().await;
        let db = sys.db.read().await;
        if let Some(existing_id) = db.find_exact_duplicate(&project, &text)? {
            drop(db);
            let _ = sys.db.write().await.touch_chunk(&existing_id);
            return Ok(format!(
                "memory deduplicated (exact match): {} ({})",
                existing_id, project
            ));
        }
    }

    let id = format!(
        "manual_{}",
        uuid::Uuid::new_v4().to_string().replace('-', "")[..16].to_string()
    );

    // Fire-and-forget: respond immediately, perform embed + write in background.
    // This keeps the user's response path free of embedding latency, matching the
    // old hook-based "Stop hook" architecture's behaviour.
    let bg_id = id.clone();
    let bg_project = project.clone();
    let bg_text = text.clone();
    let bg_system = system.clone();
    tokio::spawn(async move {
        if let Err(e) = persist_chunk(bg_system, bg_id, bg_project, bg_text).await {
            tracing::warn!("background memory_add failed: {e:#}");
        }
    });

    Ok(format!("memory queued: {} ({})", id, project))
}

async fn persist_chunk(
    system: Arc<RwLock<MemorySystem>>,
    id: String,
    project: String,
    text: String,
) -> Result<()> {
    let sys = system.read().await;
    let embedder = sys.embedder.clone();
    // Run CPU-bound embedding off the runtime worker thread.
    let embed_text = text.clone();
    let embedding = tokio::task::spawn_blocking(move || embedder.encode_document(&embed_text))
        .await
        .map_err(|e| anyhow::anyhow!("embed task join: {e}"))??;

    // Semantic dedup: if a chunk in the same project already lives within
    // cosine distance 0.05 (~95% similarity) we treat the new write as a
    // duplicate and just touch the existing chunk instead of inserting.
    // Threshold mirrors the old Factory hooks: tight enough to ignore minor
    // re-phrasings, loose enough to allow legitimate new content.
    {
        let index = sys.vector_index.read().await;
        let neighbors = index.search(&embedding, 5)?;
        drop(index);
        if let Some((existing_id, distance)) = neighbors.first() {
            if *distance < 0.05 {
                // Verify same project before suppressing — embeddings are
                // global but chunks are project-scoped.
                let db = sys.db.read().await;
                if let Ok(Some(existing)) = db.get_chunk(existing_id) {
                    if existing.project == project {
                        drop(db);
                        let _ = sys.db.write().await.touch_chunk(existing_id);
                        tracing::debug!(
                            "semantic dedup suppressed {} (≈{}, distance={:.4})",
                            id,
                            existing_id,
                            distance
                        );
                        return Ok(());
                    }
                }
            }
        }
    }

    let chunk = MemoryChunk {
        id: id.clone(),
        project: project.clone(),
        document: text,
        embedding: Some(embedding.clone()),
        metadata: Metadata {
            chunk_type: ChunkType::Manual,
            ..Default::default()
        },
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    sys.db.write().await.insert_chunk(&chunk)?;
    sys.add_text_doc(&chunk.id, &chunk.project, &chunk.document)
        .await?;
    sys.vector_index.write().await.add(&chunk.id, &embedding)?;
    Ok(())
}

async fn memory_update(system: Arc<RwLock<MemorySystem>>, args: &Value) -> Result<String> {
    let id = args.get("id").and_then(Value::as_str).unwrap_or("").trim();
    anyhow::ensure!(!id.is_empty(), "id is required");

    let sys = system.read().await;
    let mut chunk = {
        let db = sys.db.read().await;
        match db.get_chunk(id)? {
            Some(chunk) => chunk,
            None => return Ok(format!("memory '{}' not found", id)),
        }
    };

    let mut text_changed = false;
    if let Some(raw) = args.get("text").and_then(Value::as_str) {
        anyhow::ensure!(!raw.trim().is_empty(), "text must not be empty");
        let text = redact_text(raw);
        text_changed = text != chunk.document;
        chunk.document = text;
    }
    if let Some(project) = args.get("project").and_then(Value::as_str) {
        if !project.trim().is_empty() {
            chunk.project = project.trim().to_string();
        }
    }
    if let Some(importance) = args.get("importance").and_then(Value::as_str) {
        chunk.metadata.importance = parse_importance(importance)?;
    }
    if let Some(chunk_type) = args.get("chunk_type").and_then(Value::as_str) {
        chunk.metadata.chunk_type = parse_chunk_type(chunk_type)?;
    }

    let mut embedding_changed = false;
    if text_changed || chunk.embedding.is_none() {
        let embedder = sys.embedder.clone();
        let embed_text = chunk.document.clone();
        chunk.embedding = Some(
            tokio::task::spawn_blocking(move || embedder.encode_document(&embed_text))
                .await
                .map_err(|e| anyhow::anyhow!("embed task join: {e}"))??,
        );
        embedding_changed = true;
    }
    chunk.updated_at = chrono::Utc::now();
    sys.db.write().await.insert_chunk(&chunk)?;
    sys.add_text_doc(&chunk.id, &chunk.project, &chunk.document)
        .await?;
    if embedding_changed {
        if let Some(embedding) = &chunk.embedding {
            let mut vector_index = sys.vector_index.write().await;
            vector_index.add(&chunk.id, embedding)?;
            vector_index.save()?;
        }
    }
    Ok(format!("memory updated: {} ({})", chunk.id, chunk.project))
}

fn parse_importance(value: &str) -> Result<Importance> {
    match value {
        "log" => Ok(Importance::Log),
        "knowledge" => Ok(Importance::Knowledge),
        "decision" => Ok(Importance::Decision),
        "preference" => Ok(Importance::Preference),
        other => anyhow::bail!("invalid importance: {other}"),
    }
}

fn parse_chunk_type(value: &str) -> Result<ChunkType> {
    match value {
        "auto_log" => Ok(ChunkType::AutoLog),
        "manual" => Ok(ChunkType::Manual),
        "filtered" => Ok(ChunkType::Filtered),
        "consolidated" => Ok(ChunkType::Consolidated),
        other => anyhow::bail!("invalid chunk_type: {other}"),
    }
}

async fn memory_context(system: Arc<RwLock<MemorySystem>>, args: &Value) -> Result<String> {
    let query = args.get("query").and_then(Value::as_str).unwrap_or("");
    anyhow::ensure!(!query.trim().is_empty(), "query is required");
    let project = args.get("project").and_then(Value::as_str).unwrap_or("all");
    let n = args.get("n_results").and_then(Value::as_u64).unwrap_or(6) as usize;
    let max_notes = args.get("max_notes").and_then(Value::as_u64).unwrap_or(12) as usize;
    let max_facts = args.get("max_facts").and_then(Value::as_u64).unwrap_or(8) as usize;

    let search_text = memory_search(
        system.clone(),
        &json!({"query": query, "project": project, "n_results": n.clamp(1, 20)}),
    )
    .await?;

    let sys = system.read().await;
    let db = sys.db.read().await;
    let mut notes = db.get_notes()?;
    notes.sort_by(|a, b| b.updated.cmp(&a.updated));
    notes.truncate(max_notes.clamp(0, 50));
    let query_lower = query.to_lowercase();
    let mut facts = db
        .get_facts(1000)?
        .into_iter()
        .filter(|fact| {
            format!("{} {} {}", fact.subject, fact.predicate, fact.object)
                .to_lowercase()
                .contains(&query_lower)
        })
        .collect::<Vec<_>>();
    facts.truncate(max_facts.clamp(0, 50));

    let mut lines = vec!["<memnest_context>".to_string()];
    if !notes.is_empty() {
        lines.push("core_notes:".to_string());
        for note in notes {
            lines.push(format!("- {}: {}", note.key, redact_text(&note.value)));
        }
    }
    if !facts.is_empty() {
        lines.push("facts:".to_string());
        for fact in facts {
            lines.push(format!(
                "- {} {} {}",
                fact.subject, fact.predicate, fact.object
            ));
        }
    }
    lines.push("retrieved_memories:".to_string());
    lines.push(search_text);
    lines.push("</memnest_context>".to_string());
    Ok(lines.join("\n"))
}

pub(crate) async fn memory_search(
    system: Arc<RwLock<MemorySystem>>,
    args: &Value,
) -> Result<String> {
    let query = args.get("query").and_then(Value::as_str).unwrap_or("");
    anyhow::ensure!(!query.trim().is_empty(), "query is required");
    let project = args.get("project").and_then(Value::as_str).unwrap_or("all");
    let n = args.get("n_results").and_then(Value::as_u64).unwrap_or(10) as usize;
    let recent_first = args
        .get("recent_first")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    // Delegate to the shared HTTP ranking core so the MCP tool and the HTTP
    // /search endpoint return identically-ranked results (composite scoring +
    // MMR diversity + config-driven weights). Previously this path had its own
    // weaker scoring — RRF order plus a keyword bonus, with no composite
    // re-rank and no diversification.
    let items =
        crate::server::api::run_hybrid_search(system, query, project, n, recent_first, false).await;

    let mut lines = vec![format!("=== memory search results ({}) ===", query)];
    if items.is_empty() {
        lines.push("no results".to_string());
    }
    for (i, item) in items.iter().enumerate() {
        lines.push(format!(
            "[{}] project={} score={:.4} id={}",
            i + 1,
            item.project,
            item.score,
            item.id
        ));
        lines.push(format!(
            "    {}",
            item.document.chars().take(500).collect::<String>()
        ));
    }
    Ok(lines.join("\n"))
}

async fn memory_stats(system: Arc<RwLock<MemorySystem>>) -> Result<String> {
    let sys = system.read().await;
    let db = sys.db.read().await;
    Ok(serde_json::to_string_pretty(&json!({
        "total_chunks": db.chunk_count()?,
        "session_summaries": db.summary_count()?,
        "facts": db.fact_count()?,
        "notes": db.note_count()?,
        "servers": db.server_count()?,
    }))?)
}

async fn memory_facts(system: Arc<RwLock<MemorySystem>>, args: &Value) -> Result<String> {
    let query = args.get("query").and_then(Value::as_str).unwrap_or("");
    let max_results = args
        .get("max_results")
        .and_then(Value::as_u64)
        .unwrap_or(20) as usize;
    let sys = system.read().await;
    let db = sys.db.read().await;
    let facts = db.get_facts(1000)?;
    let query_lower = query.to_lowercase();
    let mut matched = Vec::new();
    for fact in facts {
        let text = format!("{} {} {}", fact.subject, fact.predicate, fact.object).to_lowercase();
        if text.contains(&query_lower) {
            matched.push(fact);
        }
    }
    matched.truncate(max_results);
    if matched.is_empty() {
        return Ok(format!("no facts matching '{}'", query));
    }
    let mut lines = vec![format!("=== facts ({}) ===", query)];
    for (i, f) in matched.iter().enumerate() {
        lines.push(format!(
            "[{}] {} - {}: {}",
            i + 1,
            f.subject,
            f.predicate,
            f.object
        ));
    }
    Ok(lines.join("\n"))
}

async fn memory_sessions(system: Arc<RwLock<MemorySystem>>, args: &Value) -> Result<String> {
    let project = args.get("project").and_then(Value::as_str).unwrap_or("");
    let n = args.get("n").and_then(Value::as_u64).unwrap_or(5) as usize;
    let sys = system.read().await;
    let db = sys.db.read().await;
    let summaries = if project.is_empty() {
        db.get_summaries_by_project("", n)?
    } else {
        db.get_summaries_by_project(project, n)?
    };
    if summaries.is_empty() {
        return Ok("no session summaries".to_string());
    }
    let mut lines = vec!["=== session summaries ===".to_string()];
    for s in summaries {
        lines.push(format!(
            "[{}] {} ({})",
            s.created_at.to_rfc3339(),
            s.project,
            s.session_id
        ));
        lines.push(format!("    {}", s.summary));
    }
    Ok(lines.join("\n"))
}

async fn note_get(system: Arc<RwLock<MemorySystem>>, args: &Value) -> Result<String> {
    let key = args.get("key").and_then(Value::as_str).unwrap_or("");
    anyhow::ensure!(!key.is_empty(), "key is required");
    let sys = system.read().await;
    let db = sys.db.read().await;
    match db.get_note(key)? {
        Some(note) => Ok(format!(
            "[{}] {} (updated: {})",
            note.key,
            note.value,
            note.updated.to_rfc3339()
        )),
        None => Ok(format!("note '{}' not found", key)),
    }
}

async fn note_set(system: Arc<RwLock<MemorySystem>>, args: &Value) -> Result<String> {
    let key = args.get("key").and_then(Value::as_str).unwrap_or("");
    let value = args.get("value").and_then(Value::as_str).unwrap_or("");
    anyhow::ensure!(!key.is_empty(), "key is required");
    let sys = system.read().await;
    let db = sys.db.write().await;
    let note = crate::models::Note {
        key: key.to_string(),
        value: value.to_string(),
        updated: chrono::Utc::now(),
        prev: None,
    };
    db.insert_note(&note)?;
    Ok(format!("note set: {} = {}", key, value))
}

async fn note_list(system: Arc<RwLock<MemorySystem>>) -> Result<String> {
    let sys = system.read().await;
    let db = sys.db.read().await;
    let notes = db.get_notes()?;
    if notes.is_empty() {
        return Ok("no notes".to_string());
    }
    let mut lines = vec!["=== notes ===".to_string()];
    for note in notes {
        lines.push(format!(
            "  {}: {} ({})",
            note.key,
            note.value,
            note.updated.to_rfc3339()
        ));
    }
    Ok(lines.join("\n"))
}

async fn note_delete(system: Arc<RwLock<MemorySystem>>, args: &Value) -> Result<String> {
    let key = args.get("key").and_then(Value::as_str).unwrap_or("").trim();
    anyhow::ensure!(!key.is_empty(), "key is required");
    let sys = system.read().await;
    if sys.db.write().await.delete_note(key)? {
        Ok(format!("note deleted: {}", key))
    } else {
        Ok(format!("note '{}' not found", key))
    }
}

async fn server_info(system: Arc<RwLock<MemorySystem>>, args: &Value) -> Result<String> {
    let name = args.get("name").and_then(Value::as_str).unwrap_or("");
    let sys = system.read().await;
    let db = sys.db.read().await;
    let servers = db.get_servers()?;
    if servers.is_empty() {
        return Ok("no servers registered".to_string());
    }
    if name.is_empty() {
        let mut lines = vec!["=== servers ===".to_string()];
        for s in servers {
            lines.push(format!("  {}: {}@{}:{}", s.name, s.user, s.host, s.port));
        }
        return Ok(lines.join("\n"));
    }
    match servers.into_iter().find(|s| s.name == name) {
        Some(s) => Ok(format!(
            "{}: {}@{}:{} (note: {})",
            s.name, s.user, s.host, s.port, s.note
        )),
        None => Ok(format!("server '{}' not found", name)),
    }
}

async fn server_add(system: Arc<RwLock<MemorySystem>>, args: &Value) -> Result<String> {
    let name = args.get("name").and_then(Value::as_str).unwrap_or("");
    let host = args.get("host").and_then(Value::as_str).unwrap_or("");
    let user = args.get("user").and_then(Value::as_str).unwrap_or("");
    let password = args.get("password").and_then(Value::as_str).unwrap_or("");
    let port = args.get("port").and_then(Value::as_u64).unwrap_or(22) as u16;
    let note = args.get("note").and_then(Value::as_str).unwrap_or("");
    anyhow::ensure!(
        !name.is_empty() && !host.is_empty() && !user.is_empty(),
        "name, host, user required"
    );
    let sys = system.read().await;
    let db = sys.db.write().await;
    let server = crate::models::ServerInfo {
        name: name.to_string(),
        host: host.to_string(),
        user: user.to_string(),
        password: password.to_string(),
        port,
        ssh_cmd: format!("ssh -p {} {}@{}", port, user, host),
        scp_cmd: format!("scp -P {} {{src}} {}@{}:{{dst}}", port, user, host),
        note: note.to_string(),
        project_path: None,
        updated: chrono::Utc::now(),
    };
    db.insert_server(&server)?;
    Ok(format!("server added: {}@{}:{}", user, host, port))
}

async fn server_update(system: Arc<RwLock<MemorySystem>>, args: &Value) -> Result<String> {
    let name = args.get("name").and_then(Value::as_str).unwrap_or("");
    let field = args.get("field").and_then(Value::as_str).unwrap_or("");
    let value = args.get("value").and_then(Value::as_str).unwrap_or("");
    anyhow::ensure!(
        !name.is_empty() && !field.is_empty(),
        "name and field required"
    );
    let sys = system.read().await;
    let db = sys.db.write().await;
    let mut server = db
        .get_server(name)?
        .ok_or_else(|| anyhow::anyhow!("server not found"))?;
    match field {
        "host" => server.host = value.to_string(),
        "user" => server.user = value.to_string(),
        "password" => server.password = value.to_string(),
        "port" => server.port = value.parse()?,
        "note" => server.note = value.to_string(),
        _ => return Ok(format!("unknown field: {}", field)),
    }
    server.updated = chrono::Utc::now();
    db.insert_server(&server)?;
    Ok(format!("server {} updated: {} = {}", name, field, value))
}

async fn memory_graph_query(system: Arc<RwLock<MemorySystem>>, args: &Value) -> Result<String> {
    let node = args.get("node").and_then(Value::as_str).unwrap_or("");
    let depth = args.get("depth").and_then(Value::as_u64).unwrap_or(2) as usize;
    anyhow::ensure!(!node.is_empty(), "node is required");
    let sys = system.read().await;
    let graph = sys.graph.read().await;
    let results = graph.bfs_traverse(node, depth);
    if results.is_empty() {
        return Ok(format!("no graph nodes related to '{}'", node));
    }
    let mut lines = vec![format!("=== graph query: '{}' (depth={}) ===", node, depth)];
    for (name, d) in results {
        lines.push(format!("  [depth {}] {}", d, name));
    }
    Ok(lines.join("\n"))
}

async fn memory_lifecycle_run(system: Arc<RwLock<MemorySystem>>) -> Result<String> {
    let result = crate::lifecycle::run_lifecycle(system).await?;
    Ok(serde_json::to_string_pretty(&result)?)
}

async fn memory_session_fork(system: Arc<RwLock<MemorySystem>>, args: &Value) -> Result<String> {
    let from = args
        .get("from_session_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let to = args
        .get("to_session_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let to_cwd = args
        .get("to_cwd")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let to_project = args
        .get("to_project")
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let dry_run = args
        .get("dry_run")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    anyhow::ensure!(!from.is_empty(), "from_session_id is required");
    anyhow::ensure!(!to.is_empty(), "to_session_id is required");
    anyhow::ensure!(!to_cwd.is_empty(), "to_cwd is required");
    anyhow::ensure!(from != to, "from_session_id must differ from to_session_id");

    let project = to_project.unwrap_or_else(|| project_from_cwd(&to_cwd));

    let sys = system.read().await;
    if dry_run {
        let db = sys.db.read().await;
        let count = db
            .get_chunks_by_session(&from)
            .map(|v| v.len())
            .unwrap_or(0);
        return Ok(serde_json::to_string_pretty(&json!({
            "status": "ok",
            "dry_run": true,
            "matched": count,
            "moved": 0,
            "to_project": project,
        }))?);
    }

    let moved = {
        let db = sys.db.write().await;
        db.reparent_session(&from, &to, &project, &to_cwd)?
    };
    let _ = sys.reindex_after_fork(&moved).await;

    Ok(serde_json::to_string_pretty(&json!({
        "status": "ok",
        "dry_run": false,
        "matched": moved.len(),
        "moved": moved.len(),
        "to_project": project,
        "ids": moved.iter().map(|c| &c.id).collect::<Vec<_>>(),
    }))?)
}

/// Mirror of the HTTP handler's helper. Kept colocated so MCP doesn't depend
/// on the api module being public.
fn project_from_cwd(cwd: &str) -> String {
    let trimmed = cwd.trim().trim_end_matches(['/', '\\']);
    if trimmed.is_empty() {
        return "default".into();
    }
    let last = trimmed
        .rsplit(|c| c == '/' || c == '\\')
        .next()
        .unwrap_or("")
        .trim();
    if last.is_empty() {
        "default".into()
    } else {
        last.to_string()
    }
}




