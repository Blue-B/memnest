use axum::{
    extract::{Path, Query, State},
    response::{Html, IntoResponse, Json, Response},
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::MemorySystem;
use crate::models::*;
use crate::redaction::redact_text;

// ── Request/Response Types ───────────────────────────────────

#[derive(Deserialize)]
pub struct SearchRequest {
    query: String,
    #[serde(default = "default_project")]
    project: String,
    #[serde(default = "default_n")]
    n_results: usize,
    #[serde(default)]
    recent_first: bool,
}

fn default_project() -> String {
    "all".to_string()
}
fn default_n() -> usize {
    10
}

#[derive(Serialize)]
pub struct SearchResponse {
    results: Vec<SearchResultItem>,
    total: usize,
    elapsed_ms: u128,
}

#[derive(Serialize)]
pub struct SearchResultItem {
    pub id: String,
    pub project: String,
    pub document: String,
    pub score: f32,
    pub timestamp: String,
    pub chunk_type: String,
    pub importance: String,
}

#[derive(Deserialize)]
pub struct AddRequest {
    text: String,
    #[serde(default)]
    project: String,
    #[serde(default)]
    metadata: Option<Metadata>,
}

#[derive(Deserialize)]
pub struct DeleteRequest {
    ids: Vec<String>,
}

#[derive(Deserialize)]
pub struct UpdateRequest {
    id: String,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    project: Option<String>,
    #[serde(default)]
    metadata: Option<MetadataPatch>,
    #[serde(default)]
    chunk_type: Option<ChunkType>,
    #[serde(default)]
    importance: Option<Importance>,
}

#[derive(Deserialize)]
pub struct MetadataPatch {
    #[serde(default)]
    chunk_type: Option<ChunkType>,
    #[serde(default)]
    importance: Option<Importance>,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    cwd: Option<String>,
    #[serde(default)]
    parent_session_id: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    tool: Option<String>,
    #[serde(default)]
    event_id: Option<String>,
    #[serde(default)]
    sequence: Option<i64>,
    #[serde(default)]
    total: Option<i64>,
    #[serde(default)]
    truncated: Option<bool>,
    #[serde(default)]
    raw_chunk: Option<String>,
    #[serde(default)]
    access_count: Option<i64>,
    #[serde(default)]
    keywords: Option<Vec<String>>,
    #[serde(default)]
    sensitive: Option<bool>,
}

#[derive(Deserialize)]
pub struct ContextRequest {
    query: String,
    #[serde(default = "default_project")]
    project: String,
    #[serde(default = "default_context_results")]
    n_results: usize,
    #[serde(default = "default_context_notes")]
    max_notes: usize,
    #[serde(default = "default_context_facts")]
    max_facts: usize,
}

fn default_context_results() -> usize {
    6
}
fn default_context_notes() -> usize {
    12
}
fn default_context_facts() -> usize {
    8
}

#[derive(Deserialize)]
pub struct NoteSetRequest {
    key: String,
    value: String,
}

#[derive(Deserialize)]
pub struct PruneRequest {
    #[serde(default)]
    project: String,
    #[serde(default)]
    chunk_type: Option<ChunkType>,
    #[serde(default)]
    importance: Option<Importance>,
    #[serde(default)]
    keep_latest: Option<usize>,
    #[serde(default)]
    older_than_days: Option<i64>,
    #[serde(default)]
    dry_run: bool,
}

#[derive(Deserialize)]
pub struct ReprojectRequest {
    #[serde(default)]
    from_project: String,
    to_project: String,
    #[serde(default)]
    session_id: String,
    #[serde(default)]
    dry_run: bool,
}

#[derive(Deserialize)]
pub struct SummaryRequest {
    project: String,
    session_id: String,
    summary: String,
}

#[derive(Deserialize)]
pub struct FactRequest {
    subject: String,
    predicate: String,
    object: String,
    #[serde(default)]
    source_session: Option<String>,
}

#[derive(Deserialize)]
pub struct CompactRequest {
    #[serde(default)]
    project: String,
    #[serde(default = "default_compact_limit")]
    limit: usize,
    #[serde(default)]
    dry_run: bool,
    #[serde(default = "default_true")]
    vacuum: bool,
}

fn default_compact_limit() -> usize {
    10_000
}
fn default_true() -> bool {
    true
}

#[derive(Serialize)]
pub struct PruneResponse {
    matched: usize,
    deleted: usize,
    ids: Vec<String>,
}

#[derive(Serialize)]
pub struct ReprojectResponse {
    matched: usize,
    updated: usize,
    ids: Vec<String>,
}

#[derive(Deserialize)]
pub struct ForkSessionRequest {
    pub from_session_id: String,
    pub to_session_id: String,
    /// Target cwd of the forked session. Required — the new project bucket
    /// is derived from its basename so chunks land in the right collection.
    pub to_cwd: String,
    /// Optional explicit project bucket override. Defaults to `basename(to_cwd)`.
    #[serde(default)]
    pub to_project: Option<String>,
    /// When true, just reports the match count without performing the move.
    #[serde(default)]
    pub dry_run: bool,
}

#[derive(Serialize)]
pub struct ForkSessionResponse {
    pub status: String,
    pub matched: usize,
    pub moved: usize,
    pub to_project: String,
    pub ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Serialize)]
pub struct SummaryResponse {
    status: String,
    id: String,
    facts: usize,
}

#[derive(Serialize)]
pub struct FactResponse {
    status: String,
    id: String,
}

#[derive(Serialize)]
pub struct CompactResponse {
    matched: usize,
    updated: usize,
    vacuumed: bool,
}

#[derive(Serialize)]
pub struct ContextResponse {
    query: String,
    project: String,
    notes: Vec<Note>,
    facts: Vec<Fact>,
    memories: Vec<SearchResultItem>,
    prompt: String,
}

#[derive(Serialize)]
pub struct HealthResponse {
    status: String,
    version: String,
}

#[derive(Serialize)]
pub struct StatsResponse {
    total_chunks: usize,
    total_sessions: usize,
    total_facts: usize,
    total_notes: usize,
    total_servers: usize,
    graph_nodes: usize,
    graph_edges: usize,
}

// ── API Handlers ─────────────────────────────────────────────

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}

pub async fn search(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Json(req): Json<SearchRequest>,
) -> Json<SearchResponse> {
    let started = std::time::Instant::now();
    let items = run_hybrid_search(
        system,
        &req.query,
        &req.project,
        req.n_results,
        req.recent_first,
        false,
    )
    .await;
    let total = items.len();
    Json(SearchResponse {
        results: items,
        total,
        elapsed_ms: started.elapsed().as_millis(),
    })
}

pub(crate) async fn run_hybrid_search(
    system: Arc<RwLock<MemorySystem>>,
    query: &str,
    project: &str,
    n: usize,
    recent_first: bool,
    require_visible_match: bool,
) -> Vec<SearchResultItem> {
    let n_results = n.clamp(1, 50);
    let project = if project.trim().is_empty() {
        "all"
    } else {
        project
    };
    let candidate_multiplier = if project == "all" { 4 } else { 20 };
    let candidate_limit = (n_results * candidate_multiplier).clamp(20, 1000);
    let sys = system.read().await;
    let db = sys.db.read().await;

    let text_results = sys
        .text_search(query, candidate_limit)
        .await
        .unwrap_or_default();
    let text_score_by_id: HashMap<String, f32> = text_results.iter().cloned().collect();

    let vector_results = if require_visible_match {
        Vec::new()
    } else {
        let embedder = sys.embedder.clone();
        let query_owned = query.to_string();
        let encoded = tokio::task::spawn_blocking(move || embedder.encode_query(&query_owned))
            .await
            .ok()
            .and_then(|res| res.ok());
        match encoded {
            Some(query_embedding) => sys
                .vector_index
                .read()
                .await
                .search(&query_embedding, candidate_limit)
                .unwrap_or_default(),
            None => Vec::new(),
        }
    };
    let vector_distance_by_id: HashMap<String, f32> = vector_results.iter().cloned().collect();
    let fused =
        crate::index::hybrid::rrf_fusion(&vector_results, &text_results, 60.0).unwrap_or_default();

    let keywords = crate::search::extract_keywords(query, 2);
    let distance_cutoff = sys.config.distance_cutoff;
    let lexical_available = !text_results.is_empty();
    let allow_semantic_fallback = keywords.len() >= 2;
    let vector_only_budget = if lexical_available || !allow_semantic_fallback {
        0
    } else {
        sys.config.low_relevance_fallback
    };
    let mut vector_only_used = 0usize;
    let mut items: Vec<(SearchResultItem, Vec<f32>)> = Vec::new();
    for (id, score) in fused {
        if let Ok(Some(c)) = db.get_chunk(&id) {
            if project != "all" && c.project != project {
                continue;
            }
            let keyword_ratio = keyword_match_ratio(&c.document, &keywords);
            if require_visible_match && keyword_ratio <= 0.0 {
                continue;
            }
            let text_hit = text_score_by_id.contains_key(&id);
            let vector_hit = vector_distance_by_id
                .get(&id)
                .is_some_and(|distance| *distance <= distance_cutoff);
            if !text_hit && keyword_ratio <= 0.0 {
                if vector_hit && vector_only_used < vector_only_budget {
                    vector_only_used += 1;
                } else {
                    continue;
                }
            }
            let final_score = score
                + keyword_bonus_from_ratio(keyword_ratio, sys.config.keyword_max_bonus)
                + importance_bonus(&c.metadata.importance)
                + type_bonus(&c.metadata.chunk_type)
                - recency_penalty(
                    c.created_at,
                    sys.config.recency_penalty_rate,
                    sys.config.recency_penalty_cap,
                );
            let embedding = c.embedding.clone().unwrap_or_default();
            items.push((
                SearchResultItem {
                    id: c.id,
                    project: c.project,
                    document: if require_visible_match {
                        query_excerpt(&redact_text(&c.document), query, 600)
                    } else {
                        redact_text(&c.document).chars().take(600).collect()
                    },
                    score: final_score,
                    timestamp: c.created_at.to_rfc3339(),
                    chunk_type: format!("{:?}", c.metadata.chunk_type),
                    importance: format!("{:?}", c.metadata.importance),
                },
                embedding,
            ));
            if items.len() >= candidate_limit {
                break;
            }
        }
    }
    items.sort_by(|a, b| {
        b.0.score
            .partial_cmp(&a.0.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if recent_first {
        items.sort_by(|a, b| b.0.timestamp.cmp(&a.0.timestamp));
        let ranked = items.into_iter().map(|(item, _)| item).collect();
        return diversify_by_project(ranked, n_results);
    }
    // Content-diversify with MMR when enabled (0 < mmr_lambda < 1); otherwise
    // fall back to the legacy per-project cap. MMR stops near-duplicate chunks
    // from crowding out distinct-but-relevant ones — the dominant failure mode
    // on auto-logged stores where a single project holds nearly all chunks.
    let lambda = sys.config.mmr_lambda;
    if lambda > 0.0 && lambda < 1.0 {
        mmr_select(items, lambda, n_results)
    } else {
        let ranked = items.into_iter().map(|(item, _)| item).collect();
        diversify_by_project(ranked, n_results)
    }
}

pub async fn add(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Json(req): Json<AddRequest>,
) -> Json<HashMap<String, String>> {
    // Fire-and-forget: respond immediately, perform embed + DB write + index update
    // in a background task. This matches the legacy Stop-hook architecture where
    // memory writes never block the user-facing response path.
    let project = if req.project.is_empty() {
        "default".to_string()
    } else {
        req.project
    };
    let text = redact_text(&req.text);
    let metadata = req.metadata.unwrap_or(Metadata {
        chunk_type: ChunkType::Manual,
        ..Default::default()
    });

    // Exact-match dedup before queueing — cheap indexed lookup, lets the
    // client see a clear "deduplicated" status instead of paying for an
    // embedding round trip just to discard the result.
    {
        let sys = system.read().await;
        let db = sys.db.read().await;
        if let Ok(Some(existing_id)) = db.find_exact_duplicate(&project, &text) {
            drop(db);
            let _ = sys.db.write().await.touch_chunk(&existing_id);
            let mut map = HashMap::new();
            map.insert("status".to_string(), "deduplicated".to_string());
            map.insert("id".to_string(), existing_id);
            map.insert("project".to_string(), project);
            return Json(map);
        }
    }

    let id = format!(
        "manual_{}",
        uuid::Uuid::new_v4().to_string().replace("-", "")[..16].to_string()
    );

    let bg_system = system.clone();
    let bg_id = id.clone();
    let bg_project = project.clone();
    let bg_text = text.clone();
    let bg_metadata = metadata.clone();
    tokio::spawn(async move {
        if let Err(e) =
            persist_chunk_async(bg_system, bg_id, bg_project, bg_text, bg_metadata).await
        {
            tracing::warn!("background api add failed: {e:#}");
        }
    });

    let mut map = HashMap::new();
    map.insert("status".to_string(), "queued".to_string());
    map.insert("id".to_string(), id);
    map.insert("project".to_string(), project);
    Json(map)
}

async fn persist_chunk_async(
    system: Arc<RwLock<MemorySystem>>,
    id: String,
    project: String,
    text: String,
    metadata: Metadata,
) -> anyhow::Result<()> {
    let sys = system.read().await;
    let embedder = sys.embedder.clone();
    let embed_text = text.clone();
    let embedding = tokio::task::spawn_blocking(move || embedder.encode_document(&embed_text))
        .await
        .map_err(|e| anyhow::anyhow!("embed join: {e}"))??;

    // Semantic dedup: cosine distance < 0.05 (~95% similar) suppresses the
    // insert and just refreshes the existing chunk. Mirrors mcp::persist_chunk.
    {
        let index = sys.vector_index.read().await;
        let neighbors = index.search(&embedding, 5)?;
        drop(index);
        if let Some((existing_id, distance)) = neighbors.first() {
            if *distance < 0.05 {
                let db = sys.db.read().await;
                if let Ok(Some(existing)) = db.get_chunk(existing_id) {
                    if existing.project == project {
                        drop(db);
                        let _ = sys.db.write().await.touch_chunk(existing_id);
                        tracing::debug!(
                            "api semantic dedup suppressed {} (≈{}, distance={:.4})",
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
        metadata,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    {
        let db = sys.db.write().await;
        db.insert_chunk(&chunk)?;
    }
    sys.add_text_doc(&chunk.id, &chunk.project, &chunk.document)
        .await?;
    {
        let mut vector_index = sys.vector_index.write().await;
        vector_index.add(&chunk.id, &embedding)?;
        let _ = vector_index.save();
    }
    Ok(())
}

pub async fn delete(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Json(req): Json<DeleteRequest>,
) -> Json<HashMap<String, serde_json::Value>> {
    let sys = system.read().await;
    let db = sys.db.write().await;

    let mut deleted = Vec::new();
    let mut not_found = Vec::new();

    for id in req.ids {
        match db.delete_chunk(&id) {
            Ok(true) => deleted.push(id),
            Ok(false) => not_found.push(id),
            Err(_) => not_found.push(id),
        }
    }
    drop(db);

    if !deleted.is_empty() {
        let _ = sys.remove_text_docs(&deleted).await;

        let mut vector_index = sys.vector_index.write().await;
        for id in &deleted {
            let _ = vector_index.remove(id);
        }
        let _ = vector_index.save();
    }

    let mut result = HashMap::new();
    result.insert("deleted".to_string(), serde_json::json!(deleted));
    result.insert("not_found".to_string(), serde_json::json!(not_found));
    Json(result)
}

pub async fn update(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Json(req): Json<UpdateRequest>,
) -> Json<HashMap<String, serde_json::Value>> {
    let mut out = HashMap::new();
    let id = req.id.trim();
    if id.is_empty() {
        out.insert("status".to_string(), serde_json::json!("error"));
        out.insert("message".to_string(), serde_json::json!("id is required"));
        return Json(out);
    }

    let sys = system.read().await;
    let mut chunk = {
        let db = sys.db.read().await;
        match db.get_chunk(id) {
            Ok(Some(chunk)) => chunk,
            Ok(None) => {
                out.insert("status".to_string(), serde_json::json!("not_found"));
                out.insert("id".to_string(), serde_json::json!(id));
                return Json(out);
            }
            Err(e) => {
                out.insert("status".to_string(), serde_json::json!("error"));
                out.insert("message".to_string(), serde_json::json!(e.to_string()));
                return Json(out);
            }
        }
    };

    let original_document = chunk.document.clone();
    let mut text_changed = false;
    if let Some(text) = req.text {
        if text.trim().is_empty() {
            out.insert("status".to_string(), serde_json::json!("error"));
            out.insert(
                "message".to_string(),
                serde_json::json!("text must not be empty"),
            );
            return Json(out);
        }
        let redacted = redact_text(&text);
        text_changed = redacted != original_document;
        chunk.document = redacted;
    }
    if let Some(project) = req.project {
        let project = project.trim();
        if !project.is_empty() {
            chunk.project = project.to_string();
        }
    }
    if let Some(metadata) = req.metadata {
        apply_metadata_patch(&mut chunk.metadata, metadata);
    }
    if let Some(chunk_type) = req.chunk_type {
        chunk.metadata.chunk_type = chunk_type;
    }
    if let Some(importance) = req.importance {
        chunk.metadata.importance = importance;
    }

    let mut embedding_changed = false;
    if text_changed || chunk.embedding.is_none() {
        let embedder = sys.embedder.clone();
        let embed_text = chunk.document.clone();
        match tokio::task::spawn_blocking(move || embedder.encode_document(&embed_text)).await {
            Ok(Ok(embedding)) => {
                chunk.embedding = Some(embedding);
                embedding_changed = true;
            }
            Ok(Err(e)) => {
                out.insert("status".to_string(), serde_json::json!("error"));
                out.insert("message".to_string(), serde_json::json!(e.to_string()));
                return Json(out);
            }
            Err(e) => {
                out.insert("status".to_string(), serde_json::json!("error"));
                out.insert(
                    "message".to_string(),
                    serde_json::json!(format!("embed join: {e}")),
                );
                return Json(out);
            }
        }
    }

    chunk.updated_at = chrono::Utc::now();
    match sys.db.write().await.insert_chunk(&chunk) {
        Ok(()) => {}
        Err(e) => {
            out.insert("status".to_string(), serde_json::json!("error"));
            out.insert("message".to_string(), serde_json::json!(e.to_string()));
            return Json(out);
        }
    }

    if let Err(e) = sys
        .add_text_doc(&chunk.id, &chunk.project, &chunk.document)
        .await
    {
        out.insert("status".to_string(), serde_json::json!("error"));
        out.insert("message".to_string(), serde_json::json!(e.to_string()));
        return Json(out);
    }
    if embedding_changed {
        if let Some(embedding) = &chunk.embedding {
            let mut vector_index = sys.vector_index.write().await;
            if let Err(e) = vector_index
                .add(&chunk.id, embedding)
                .and_then(|_| vector_index.save())
            {
                out.insert("status".to_string(), serde_json::json!("error"));
                out.insert("message".to_string(), serde_json::json!(e.to_string()));
                return Json(out);
            }
        }
    }

    out.insert("status".to_string(), serde_json::json!("ok"));
    out.insert("id".to_string(), serde_json::json!(chunk.id));
    out.insert("project".to_string(), serde_json::json!(chunk.project));
    out.insert(
        "updated_at".to_string(),
        serde_json::json!(chunk.updated_at.to_rfc3339()),
    );
    out.insert("text_changed".to_string(), serde_json::json!(text_changed));
    Json(out)
}

fn apply_metadata_patch(target: &mut Metadata, patch: MetadataPatch) {
    if let Some(value) = patch.chunk_type {
        target.chunk_type = value;
    }
    if let Some(value) = patch.importance {
        target.importance = value;
    }
    if let Some(value) = patch.session_id {
        target.session_id = value;
    }
    if let Some(value) = patch.cwd {
        target.cwd = Some(value);
    }
    if let Some(value) = patch.parent_session_id {
        target.parent_session_id = Some(value);
    }
    if let Some(value) = patch.source {
        target.source = Some(value);
    }
    if let Some(value) = patch.role {
        target.role = Some(value);
    }
    if let Some(value) = patch.tool {
        target.tool = Some(value);
    }
    if let Some(value) = patch.event_id {
        target.event_id = Some(value);
    }
    if let Some(value) = patch.sequence {
        target.sequence = Some(value);
    }
    if let Some(value) = patch.total {
        target.total = Some(value);
    }
    if let Some(value) = patch.truncated {
        target.truncated = value;
    }
    if let Some(value) = patch.raw_chunk {
        target.raw_chunk = Some(value);
    }
    if let Some(value) = patch.access_count {
        target.access_count = value;
    }
    if let Some(value) = patch.keywords {
        target.keywords = value;
    }
    if let Some(value) = patch.sensitive {
        target.sensitive = value;
    }
}

pub async fn context_pack(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Json(req): Json<ContextRequest>,
) -> Json<ContextResponse> {
    let query = req.query.trim().to_string();
    let project = if req.project.trim().is_empty() {
        "all".to_string()
    } else {
        req.project.trim().to_string()
    };
    let memories = if query.is_empty() {
        Vec::new()
    } else {
        run_hybrid_search(
            system.clone(),
            &query,
            &project,
            req.n_results.clamp(1, 20),
            false,
            true,
        )
        .await
    };

    let sys = system.read().await;
    let db = sys.db.read().await;
    let mut notes = db.get_notes().unwrap_or_default();
    notes.sort_by(|a, b| b.updated.cmp(&a.updated));
    notes.truncate(req.max_notes.clamp(0, 50));

    let query_lower = query.to_lowercase();
    let mut facts = db.get_facts(1000).unwrap_or_default();
    if !query_lower.is_empty() {
        facts.retain(|fact| {
            format!("{} {} {}", fact.subject, fact.predicate, fact.object)
                .to_lowercase()
                .contains(&query_lower)
        });
    }
    facts.truncate(req.max_facts.clamp(0, 50));
    drop(db);
    drop(sys);

    let prompt = render_context_prompt(&notes, &facts, &memories);
    Json(ContextResponse {
        query,
        project,
        notes,
        facts,
        memories,
        prompt,
    })
}

fn render_context_prompt(notes: &[Note], facts: &[Fact], memories: &[SearchResultItem]) -> String {
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
    if !memories.is_empty() {
        lines.push("retrieved_memories:".to_string());
        for item in memories {
            lines.push(format!(
                "- [{}:{} score={:.3}] {}",
                item.project,
                item.id,
                item.score,
                item.document.replace('\n', " ")
            ));
        }
    }
    if notes.is_empty() && facts.is_empty() && memories.is_empty() {
        lines.push("(no relevant context)".to_string());
    }
    lines.push("</memnest_context>".to_string());
    lines.join("\n")
}

pub async fn prune(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Json(req): Json<PruneRequest>,
) -> Json<PruneResponse> {
    let project = if req.project.is_empty() {
        "default".to_string()
    } else {
        req.project
    };
    if req.keep_latest.is_none() && req.older_than_days.is_none() {
        return Json(PruneResponse {
            matched: 0,
            deleted: 0,
            ids: Vec::new(),
        });
    }

    let keep_latest = req.keep_latest.unwrap_or(0);
    let cutoff = req
        .older_than_days
        .map(|days| chrono::Utc::now() - chrono::Duration::days(days));

    let sys = system.read().await;
    let db = sys.db.write().await;
    let chunks = db
        .get_chunks_by_project(&project, 100_000)
        .unwrap_or_default();

    let mut matching_seen = 0usize;
    let mut ids = Vec::new();
    for chunk in chunks {
        if let Some(chunk_type) = &req.chunk_type {
            if &chunk.metadata.chunk_type != chunk_type {
                continue;
            }
        }
        if let Some(importance) = &req.importance {
            if &chunk.metadata.importance != importance {
                continue;
            }
        }

        matching_seen += 1;
        if keep_latest > 0 && matching_seen <= keep_latest {
            continue;
        }
        if let Some(cutoff) = cutoff {
            if chunk.created_at > cutoff {
                continue;
            }
        }
        ids.push(chunk.id);
    }

    let matched = ids.len();
    let mut deleted = 0usize;
    if !req.dry_run {
        for id in &ids {
            if db.delete_chunk(id).unwrap_or(false) {
                deleted += 1;
            }
        }
    }
    drop(db);

    if deleted > 0 {
        let _ = sys.remove_text_docs(&ids).await;

        let mut vector_index = sys.vector_index.write().await;
        for id in &ids {
            let _ = vector_index.remove(id);
        }
        let _ = vector_index.save();
    }

    Json(PruneResponse {
        matched,
        deleted,
        ids,
    })
}

pub async fn reproject(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Json(req): Json<ReprojectRequest>,
) -> Json<ReprojectResponse> {
    if req.to_project.trim().is_empty() || req.to_project == "default" {
        return Json(ReprojectResponse {
            matched: 0,
            updated: 0,
            ids: Vec::new(),
        });
    }

    let from_project = if req.from_project.is_empty() {
        "default".to_string()
    } else {
        req.from_project
    };

    let sys = system.read().await;
    let db = sys.db.write().await;
    let chunks = db
        .get_chunks_by_project(&from_project, 100_000)
        .unwrap_or_default();

    let mut moved = Vec::new();
    for mut chunk in chunks {
        if !req.session_id.is_empty() && chunk.metadata.session_id != req.session_id {
            continue;
        }

        moved.push(chunk.id.clone());
        if !req.dry_run {
            chunk.project = req.to_project.clone();
            chunk.updated_at = chrono::Utc::now();
            let _ = db.insert_chunk(&chunk);
        }
    }
    drop(db);

    if !req.dry_run && !moved.is_empty() {
        let db = sys.db.read().await;
        let mut docs = Vec::with_capacity(moved.len());
        for id in &moved {
            if let Ok(Some(chunk)) = db.get_chunk(id) {
                docs.push((chunk.id, chunk.project, chunk.document));
            }
        }
        let _ = sys.add_text_docs(&docs).await;
    }

    Json(ReprojectResponse {
        matched: moved.len(),
        updated: if req.dry_run { 0 } else { moved.len() },
        ids: moved,
    })
}

/// POST /sessions/fork — reparent every chunk belonging to `from_session_id`
/// onto a new session id + cwd. Used by CLIs that implement a fork primitive
/// (`pi --fork`, claude-code resume into new path, codex subagent fork) so the
/// memory side mirrors the jsonl-level fork instead of leaving orphan chunks
/// in the source bucket.
pub async fn fork_session(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Json(req): Json<ForkSessionRequest>,
) -> Json<ForkSessionResponse> {
    let from = req.from_session_id.trim().to_string();
    let to = req.to_session_id.trim().to_string();
    let to_cwd = req.to_cwd.trim().to_string();
    if from.is_empty() || to.is_empty() || to_cwd.is_empty() {
        return Json(ForkSessionResponse {
            status: "error".into(),
            matched: 0,
            moved: 0,
            to_project: String::new(),
            ids: Vec::new(),
            message: Some("from_session_id, to_session_id, and to_cwd are required".into()),
        });
    }
    if from == to {
        return Json(ForkSessionResponse {
            status: "error".into(),
            matched: 0,
            moved: 0,
            to_project: String::new(),
            ids: Vec::new(),
            message: Some("from_session_id must differ from to_session_id".into()),
        });
    }

    let to_project = req
        .to_project
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| project_from_cwd(&to_cwd));

    let sys = system.read().await;

    if req.dry_run {
        // Count without mutating. Cheap pre-flight for clients that want to
        // confirm with the user before committing.
        let db = sys.db.read().await;
        let ids = db
            .get_chunks_by_session(&from)
            .unwrap_or_default()
            .into_iter()
            .map(|c| c.id)
            .collect::<Vec<_>>();
        return Json(ForkSessionResponse {
            status: "ok".into(),
            matched: ids.len(),
            moved: 0,
            to_project,
            ids,
            message: None,
        });
    }

    let moved = {
        let db = sys.db.write().await;
        match db.reparent_session(&from, &to, &to_project, &to_cwd) {
            Ok(moved) => moved,
            Err(e) => {
                return Json(ForkSessionResponse {
                    status: "error".into(),
                    matched: 0,
                    moved: 0,
                    to_project,
                    ids: Vec::new(),
                    message: Some(format!("reparent failed: {e}")),
                });
            }
        }
    };

    // Refresh FTS5 project field so the moved chunks search under the new bucket.
    let _ = sys.reindex_after_fork(&moved).await;

    let ids: Vec<String> = moved.iter().map(|c| c.id.clone()).collect();
    Json(ForkSessionResponse {
        status: "ok".into(),
        matched: moved.len(),
        moved: moved.len(),
        to_project,
        ids,
        message: None,
    })
}

/// Derive a project bucket name from an absolute cwd. Falls back to `default`
/// when the path is empty or has no last component (e.g. `/`).
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

pub async fn add_summary(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Json(req): Json<SummaryRequest>,
) -> Json<SummaryResponse> {
    let project = if req.project.trim().is_empty() {
        "default".to_string()
    } else {
        req.project
    };
    let session_id = req.session_id.trim().to_string();
    let summary_text = redact_text(&req.summary);
    if session_id.is_empty() || summary_text.trim().is_empty() {
        return Json(SummaryResponse {
            status: "error".to_string(),
            id: String::new(),
            facts: 0,
        });
    }

    let summary = SessionSummary {
        id: format!(
            "summary_{}",
            uuid::Uuid::new_v4().to_string().replace('-', "")[..16].to_string()
        ),
        project,
        session_id,
        summary: summary_text,
        created_at: chrono::Utc::now(),
    };
    let facts = crate::facts::extract_explicit_facts(&summary.summary, Some(&summary.session_id));
    let fact_count = facts.len();

    let sys = system.read().await;
    let db = sys.db.write().await;
    let status = match db.insert_summary(&summary) {
        Ok(_) => {
            for fact in facts {
                let existing = db.get_fact(&fact.id).ok().flatten();
                let fact = crate::facts::merge_fact(existing, fact);
                let _ = db.insert_fact(&fact);
            }
            "ok"
        }
        Err(_) => "error",
    };

    Json(SummaryResponse {
        status: status.to_string(),
        id: summary.id,
        facts: if status == "ok" { fact_count } else { 0 },
    })
}

pub async fn add_fact(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Json(req): Json<FactRequest>,
) -> Json<FactResponse> {
    let Some(fact) = crate::facts::make_fact(
        &req.subject,
        &req.predicate,
        &req.object,
        req.source_session.as_deref(),
    ) else {
        return Json(FactResponse {
            status: "error".to_string(),
            id: String::new(),
        });
    };

    let sys = system.read().await;
    let db = sys.db.write().await;
    let existing = db.get_fact(&fact.id).ok().flatten();
    let fact = crate::facts::merge_fact(existing, fact);
    let status = match db.insert_fact(&fact) {
        Ok(_) => "ok",
        Err(_) => "error",
    };
    Json(FactResponse {
        status: status.to_string(),
        id: fact.id,
    })
}

pub async fn compact(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Json(req): Json<CompactRequest>,
) -> Json<CompactResponse> {
    let limit = req.limit.clamp(1, 100_000);
    let sys = system.read().await;
    let db = sys.db.write().await;
    let chunks = if req.project.trim().is_empty() || req.project == "all" {
        db.get_all_chunks(limit).unwrap_or_default()
    } else {
        db.get_chunks_by_project(&req.project, limit)
            .unwrap_or_default()
    };

    let matched = chunks.len();
    let mut updated = 0usize;
    if !req.dry_run {
        for mut chunk in chunks {
            let redacted = redact_text(&chunk.document);
            chunk.document = redacted;
            chunk.embedding = sys.embedder.encode_document(&chunk.document).ok();
            chunk.updated_at = chrono::Utc::now();
            if db.insert_chunk(&chunk).is_ok() {
                updated += 1;
            }
        }
    }
    drop(db);

    if !req.dry_run && updated > 0 {
        let db = sys.db.read().await;
        let chunks = if req.project.trim().is_empty() || req.project == "all" {
            db.get_all_chunks(limit).unwrap_or_default()
        } else {
            db.get_chunks_by_project(&req.project, limit)
                .unwrap_or_default()
        };
        let mut vector_index = sys.vector_index.write().await;
        let mut docs = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            docs.push((
                chunk.id.clone(),
                chunk.project.clone(),
                chunk.document.clone(),
            ));
            let _ = vector_index.remove(&chunk.id);
            if let Some(embedding) = &chunk.embedding {
                let _ = vector_index.add(&chunk.id, embedding);
            }
        }
        let _ = sys.add_text_docs(&docs).await;
    }

    let mut vacuumed = false;
    if !req.dry_run && req.vacuum {
        vacuumed = sys.db.write().await.vacuum().is_ok();
    }

    Json(CompactResponse {
        matched,
        updated,
        vacuumed,
    })
}

pub async fn list_collections(
    State(system): State<Arc<RwLock<MemorySystem>>>,
) -> Json<Vec<CollectionStat>> {
    let sys = system.read().await;
    let db = sys.db.read().await;
    Json(db.collection_stats(500).unwrap_or_default())
}

#[derive(Deserialize)]
pub struct CollectionMetaRequest {
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

/// PUT /collection/:name/meta
/// Body: { "kind": "playbook|project|autolog|archive", "description": "..." }
/// Both fields optional; missing fields keep existing values.
pub async fn set_collection_meta(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Path(name): Path<String>,
    Json(req): Json<CollectionMetaRequest>,
) -> Json<HashMap<String, serde_json::Value>> {
    let mut out = HashMap::new();
    // Validate kind if provided.
    if let Some(k) = req.kind.as_deref() {
        if !matches!(k, "playbook" | "project" | "autolog" | "archive") {
            out.insert("status".into(), serde_json::Value::String("error".into()));
            out.insert(
                "message".into(),
                serde_json::Value::String(format!(
                    "invalid kind '{}' — must be playbook|project|autolog|archive",
                    k
                )),
            );
            return Json(out);
        }
    }
    let sys = system.read().await;
    let db = sys.db.write().await;
    match db.upsert_collection_meta(&name, req.kind.as_deref(), req.description.as_deref()) {
        Ok(()) => {
            let meta = db.get_collection_meta(&name).ok().flatten();
            out.insert("status".into(), serde_json::Value::String("ok".into()));
            out.insert("name".into(), serde_json::Value::String(name));
            if let Some((kind, description)) = meta {
                out.insert("kind".into(), serde_json::Value::String(kind));
                out.insert("description".into(), serde_json::Value::String(description));
            }
        }
        Err(e) => {
            out.insert("status".into(), serde_json::Value::String("error".into()));
            out.insert("message".into(), serde_json::Value::String(e.to_string()));
        }
    }
    Json(out)
}

pub async fn collection_detail(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Path(name): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let sys = system.read().await;
    let db = sys.db.read().await;

    let chunks = db.get_chunks_by_project(&name, 100).unwrap_or_default();

    if params.get("format") == Some(&"html".to_string()) {
        let mut items_html = String::new();
        for chunk in &chunks {
            let imp = format!("{:?}", chunk.metadata.importance);
            let typ = match chunk.metadata.chunk_type {
                ChunkType::Manual => "수동",
                ChunkType::AutoLog => "자동",
                ChunkType::Filtered => "필터링",
                ChunkType::Consolidated => "통합",
            };
            items_html.push_str(&format!(
                r#"<div class="bg-white dark:bg-gray-900 rounded-lg border border-gray-200 dark:border-gray-800 p-4 mb-3">
                 <div class="flex items-center gap-2 mb-2">
                  <span class="text-[10px] px-1.5 py-0.5 rounded bg-gray-100 dark:bg-gray-800 text-gray-600 dark:text-gray-400 font-medium">{}</span>
                  <span class="text-[10px] px-1.5 py-0.5 rounded bg-gray-100 dark:bg-gray-800 text-gray-600 dark:text-gray-400">{}</span>
                  <span class="text-[10px] text-gray-400 ml-auto">{}</span>
                 </div>
                 <div class="text-sm text-gray-800 dark:text-gray-200 whitespace-pre-wrap leading-relaxed">{}</div>
                </div>"#,
                imp, typ,
                chunk.created_at.format("%Y-%m-%d %H:%M"),
                html_escape(&redact_text(&chunk.document))
            ));
        }

        if items_html.is_empty() {
            items_html =
                r#"<div class="text-center py-12 text-gray-500 text-sm">메모리가 없습니다</div>"#
                    .to_string();
        }

        let content = format!(
            r##"<div class="flex items-center justify-between mb-6">
             <div>
              <h1 class="text-xl font-semibold tracking-tight">{}</h1>
              <p class="text-sm text-gray-500 dark:text-gray-400 mt-0.5">{}개 메모리</p>
             </div>
             <a href="/viewer/collections" class="quiet-chip text-xs">컬렉션</a>
            </div>
            {}"##,
            name,
            chunks.len(),
            items_html
        );

        let html = BASE_HTML
            .replace("__TITLE__", &name)
            .replace("__VERSION__", env!("CARGO_PKG_VERSION"))
            .replace(
                "__ACTIVE_DASHBOARD__",
                "",
            )
            .replace(
                "__ACTIVE_COLLECTIONS__",
                "bg-stone-950/[0.07] dark:bg-white/[0.08] text-stone-950 dark:text-stone-100 font-medium ring-1 ring-black/5 dark:ring-white/10",
            )
            .replace(
                "__ACTIVE_SEARCH__",
                "",
            )
            .replace("__CONTENT__", &content);

        return Html(html).into_response();
    }

    let items: Vec<SearchResultItem> = chunks
        .into_iter()
        .map(|c| SearchResultItem {
            id: c.id,
            project: c.project,
            document: redact_text(&c.document).chars().take(600).collect(),
            score: 0.0,
            timestamp: c.created_at.to_rfc3339(),
            chunk_type: format!("{:?}", c.metadata.chunk_type),
            importance: format!("{:?}", c.metadata.importance),
        })
        .collect();

    Json(items).into_response()
}

pub async fn list_sessions(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<Vec<SessionSummary>> {
    let sys = system.read().await;
    let db = sys.db.read().await;

    let project = params.get("project").cloned().unwrap_or_default();
    let limit = params.get("n").and_then(|s| s.parse().ok()).unwrap_or(20);

    let summaries = if project.is_empty() {
        db.get_summaries(limit).unwrap_or_default()
    } else {
        db.get_summaries_by_project(&project, limit)
            .unwrap_or_default()
    };

    Json(summaries)
}

pub async fn list_facts(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<Vec<Fact>> {
    let sys = system.read().await;
    let db = sys.db.read().await;
    let limit = params
        .get("n")
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(100)
        .clamp(1, 1000);
    let query = params.get("query").map(|value| value.to_lowercase());
    let facts = db
        .get_facts(if query.is_some() { 1000 } else { limit })
        .unwrap_or_default();
    let mut facts = if let Some(query) = query {
        facts
            .into_iter()
            .filter(|fact| {
                format!("{} {} {}", fact.subject, fact.predicate, fact.object)
                    .to_lowercase()
                    .contains(&query)
            })
            .collect::<Vec<_>>()
    } else {
        facts
    };
    facts.truncate(limit);
    Json(facts)
}

pub async fn list_notes(State(system): State<Arc<RwLock<MemorySystem>>>) -> Json<Vec<Note>> {
    let sys = system.read().await;
    let db = sys.db.read().await;
    let notes = db.get_notes().unwrap_or_default();
    Json(notes)
}

pub async fn get_note(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Path(key): Path<String>,
) -> Json<HashMap<String, serde_json::Value>> {
    let mut out = HashMap::new();
    let sys = system.read().await;
    let db = sys.db.read().await;
    match db.get_note(&key) {
        Ok(Some(note)) => {
            out.insert("status".into(), serde_json::json!("ok"));
            out.insert("note".into(), serde_json::json!(note));
        }
        Ok(None) => {
            out.insert("status".into(), serde_json::json!("not_found"));
            out.insert("key".into(), serde_json::json!(key));
        }
        Err(e) => {
            out.insert("status".into(), serde_json::json!("error"));
            out.insert("message".into(), serde_json::json!(e.to_string()));
        }
    }
    Json(out)
}

pub async fn set_note(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Json(req): Json<NoteSetRequest>,
) -> Json<HashMap<String, serde_json::Value>> {
    let mut out = HashMap::new();
    let key = req.key.trim();
    if key.is_empty() {
        out.insert("status".into(), serde_json::json!("error"));
        out.insert("message".into(), serde_json::json!("key is required"));
        return Json(out);
    }
    let sys = system.read().await;
    let db = sys.db.write().await;
    let prev = db.get_note(key).ok().flatten().map(|note| NotePrev {
        value: note.value,
        date: note.updated,
    });
    let note = Note {
        key: key.to_string(),
        value: req.value,
        updated: chrono::Utc::now(),
        prev,
    };
    match db.insert_note(&note) {
        Ok(()) => {
            out.insert("status".into(), serde_json::json!("ok"));
            out.insert("note".into(), serde_json::json!(note));
        }
        Err(e) => {
            out.insert("status".into(), serde_json::json!("error"));
            out.insert("message".into(), serde_json::json!(e.to_string()));
        }
    }
    Json(out)
}

pub async fn delete_note(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Path(key): Path<String>,
) -> Json<HashMap<String, serde_json::Value>> {
    let mut out = HashMap::new();
    let sys = system.read().await;
    match sys.db.write().await.delete_note(&key) {
        Ok(true) => {
            out.insert("status".into(), serde_json::json!("deleted"));
            out.insert("key".into(), serde_json::json!(key));
        }
        Ok(false) => {
            out.insert("status".into(), serde_json::json!("not_found"));
            out.insert("key".into(), serde_json::json!(key));
        }
        Err(e) => {
            out.insert("status".into(), serde_json::json!("error"));
            out.insert("message".into(), serde_json::json!(e.to_string()));
        }
    }
    Json(out)
}

pub async fn list_servers(
    State(system): State<Arc<RwLock<MemorySystem>>>,
) -> Json<Vec<ServerInfo>> {
    let sys = system.read().await;
    let db = sys.db.read().await;
    let servers = db.get_servers().unwrap_or_default();
    Json(servers)
}

#[derive(Deserialize)]
pub struct SecretSetRequest {
    pub key: String,
    pub value: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub note: String,
}

/// POST /secrets — store an encrypted credential.
/// Response intentionally omits the value to prevent accidental disclosure
/// in logs or proxies; clients should call GET /secrets/{key} to read it back.
pub async fn set_secret(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Json(req): Json<SecretSetRequest>,
) -> Json<HashMap<String, String>> {
    let mut out = HashMap::new();
    if req.key.trim().is_empty() || req.value.is_empty() {
        out.insert("status".into(), "error".into());
        out.insert("message".into(), "key and value are required".into());
        return Json(out);
    }
    let secret = Secret {
        key: req.key.trim().to_string(),
        kind: req.kind,
        value: req.value,
        note: req.note,
        updated: chrono::Utc::now(),
    };
    let sys = system.read().await;
    let result = sys.db.write().await.insert_secret(&secret);
    match result {
        Ok(()) => {
            out.insert("status".into(), "ok".into());
            out.insert("key".into(), secret.key);
            out.insert(
                "encryption".into(),
                if crate::crypto::is_enabled() {
                    "aes-256-gcm".into()
                } else {
                    "disabled".into()
                },
            );
        }
        Err(e) => {
            out.insert("status".into(), "error".into());
            out.insert("message".into(), e.to_string());
        }
    }
    Json(out)
}

/// GET /secrets/{key} — decrypt and return a credential value.
pub async fn get_secret(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Path(key): Path<String>,
) -> Json<HashMap<String, String>> {
    let mut out = HashMap::new();
    let sys = system.read().await;
    let db = sys.db.read().await;
    match db.get_secret(&key) {
        Ok(Some(secret)) => {
            out.insert("status".into(), "ok".into());
            out.insert("key".into(), secret.key);
            out.insert("kind".into(), secret.kind);
            out.insert("note".into(), secret.note);
            out.insert("value".into(), secret.value);
            out.insert("updated".into(), secret.updated.to_rfc3339());
        }
        Ok(None) => {
            out.insert("status".into(), "not_found".into());
            out.insert("key".into(), key);
        }
        Err(e) => {
            out.insert("status".into(), "error".into());
            out.insert("message".into(), e.to_string());
        }
    }
    Json(out)
}

/// GET /secrets — list metadata only. Values are never returned by this endpoint.
pub async fn list_secrets(
    State(system): State<Arc<RwLock<MemorySystem>>>,
) -> Json<Vec<HashMap<String, String>>> {
    let sys = system.read().await;
    let db = sys.db.read().await;
    let secrets = db.list_secret_meta().unwrap_or_default();
    let items = secrets
        .into_iter()
        .map(|s| {
            let mut m = HashMap::new();
            m.insert("key".into(), s.key);
            m.insert("kind".into(), s.kind);
            m.insert("note".into(), s.note);
            m.insert("updated".into(), s.updated.to_rfc3339());
            m
        })
        .collect();
    Json(items)
}

pub async fn delete_secret(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Path(key): Path<String>,
) -> Json<HashMap<String, String>> {
    let mut out = HashMap::new();
    let sys = system.read().await;
    match sys.db.write().await.delete_secret(&key) {
        Ok(true) => {
            out.insert("status".into(), "ok".into());
            out.insert("key".into(), key);
        }
        Ok(false) => {
            out.insert("status".into(), "not_found".into());
            out.insert("key".into(), key);
        }
        Err(e) => {
            out.insert("status".into(), "error".into());
            out.insert("message".into(), e.to_string());
        }
    }
    Json(out)
}

pub async fn stats(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let sys = system.read().await;
    let db = sys.db.read().await;

    let total_chunks = db.chunk_count().unwrap_or(0);
    let total_sessions = db.summary_count().unwrap_or(0);
    let total_facts = db.fact_count().unwrap_or(0);
    let total_notes = db.note_count().unwrap_or(0);
    let total_servers = db.server_count().unwrap_or(0);
    let (graph_nodes, graph_edges) = db.graph_stats().unwrap_or((0, 0));

    if params.get("format") == Some(&"html".to_string()) {
        let endpoint_rows = r##"
          <div class="endpoint-row">
           <span class="metric-label">GET</span><code class="text-sm">/health</code><span class="text-xs text-stone-500">서비스 상태 확인</span>
          </div>
          <div class="endpoint-row">
           <span class="metric-label">GET</span><code class="text-sm">/collections</code><span class="text-xs text-stone-500">컬렉션 목록</span>
          </div>
          <div class="endpoint-row">
           <span class="metric-label">GET</span><code class="text-sm">/facts?n=100</code><span class="text-xs text-stone-500">저장된 지식</span>
          </div>
          <div class="endpoint-row">
           <span class="metric-label">POST</span><code class="text-sm">/search</code><span class="text-xs text-stone-500">메모리 검색</span>
          </div>
        "##;
        let content = format!(
            r##"<section class="panel hero-panel p-6 md:p-8 mb-5">
             <div class="eyebrow mb-3">System</div>
             <h1 class="text-3xl md:text-5xl font-semibold tracking-tight max-w-3xl leading-tight" data-i18n="system.title">운영 상태와 연동 경로.</h1>
             <p class="text-sm md:text-base text-slate-600 dark:text-slate-300 mt-4 max-w-2xl leading-relaxed" data-i18n="system.subtitle">현재 저장 규모와 외부 연동에 필요한 경로를 확인합니다. 자동화에서 필요한 원시 응답은 디버그 링크로만 열어 둡니다.</p>
             <div class="signal-line mt-7 mb-5 w-full max-w-xl"></div>
             <div class="grid grid-cols-2 md:grid-cols-6 gap-3">
              <div><div class="metric-label">chunks</div><div class="text-2xl font-semibold mt-1">{}</div></div>
              <div><div class="metric-label">sessions</div><div class="text-2xl font-semibold mt-1">{}</div></div>
              <div><div class="metric-label">facts</div><div class="text-2xl font-semibold mt-1">{}</div></div>
              <div><div class="metric-label">notes</div><div class="text-2xl font-semibold mt-1">{}</div></div>
              <div><div class="metric-label">servers</div><div class="text-2xl font-semibold mt-1">{}</div></div>
              <div><div class="metric-label">graph</div><div class="text-2xl font-semibold mt-1">{} / {}</div></div>
             </div>
            </section>
            <div class="grid grid-cols-1 lg:grid-cols-[0.95fr_1.05fr] gap-5">
             <section class="panel p-5">
              <div class="flex items-start justify-between gap-4 mb-4">
               <div>
                <div class="eyebrow mb-2">Endpoint catalog</div>
                <h2 class="text-xl font-semibold tracking-tight" data-i18n="system.endpoints">연동 경로</h2>
               </div>
               <a href="/stats" class="quiet-chip text-xs" data-i18n="system.debug">디버그 JSON</a>
              </div>
              {}
             </section>
             <section class="panel p-5">
              <div class="eyebrow mb-2">Request examples</div>
              <div class="space-y-3">
               <pre class="text-xs overflow-x-auto rounded-xl bg-stone-950 text-stone-100 p-4">curl -s http://127.0.0.1:3111/health</pre>
               <pre class="text-xs overflow-x-auto rounded-xl bg-stone-950 text-stone-100 p-4">curl -s -X POST http://127.0.0.1:3111/search \
  -H 'content-type: application/json' \
  -d '{{"query":"memnest","project":"all","n_results":10}}'</pre>
               <p class="text-xs leading-relaxed text-slate-500 dark:text-slate-400">컬렉션 범위는 요청의 project 값으로 지정합니다. 전체 검색은 project를 all로 보냅니다.</p>
              </div>
             </section>
            </div>"##,
            total_chunks,
            total_sessions,
            total_facts,
            total_notes,
            total_servers,
            graph_nodes,
            graph_edges,
            endpoint_rows
        );

        let html = BASE_HTML
            .replace("__TITLE__", "System")
            .replace("__VERSION__", env!("CARGO_PKG_VERSION"))
            .replace("__ACTIVE_DASHBOARD__", "")
            .replace("__ACTIVE_COLLECTIONS__", "")
            .replace("__ACTIVE_SEARCH__", "")
            .replace("__CONTENT__", &content);

        return Html(html).into_response();
    }

    Json(StatsResponse {
        total_chunks,
        total_sessions,
        total_facts,
        total_notes,
        total_servers,
        graph_nodes,
        graph_edges,
    })
    .into_response()
}

// ── Viewer HTML ──────────────────────────────────────────────

const BASE_HTML: &str = r##"<!DOCTYPE html>
<html lang="ko">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>__TITLE__ | Memnest</title>
<style>
* { box-sizing: border-box; }
body { margin: 0; font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; letter-spacing: 0; }
a { color: inherit; text-decoration: none; }
button, input, select { font: inherit; }
button { cursor: pointer; border: 0; }
svg { display: block; }
::selection { background: rgba(190,123,55,.24); }
.block { display: block; }
.flex { display: flex; }
.grid { display: grid; }
.hidden { display: none; }
.inline { display: inline; }
.fixed { position: fixed; }
.relative { position: relative; }
.left-0 { left: 0; }
.right-0 { right: 0; }
.top-0 { top: 0; }
.top-14 { top: 3.5rem; }
.z-10 { z-index: 10; }
.z-30 { z-index: 30; }
.z-40 { z-index: 40; }
.min-h-screen { min-height: 100vh; }
.w-full { width: 100%; }
.w-4 { width: 1rem; }
.w-5 { width: 1.25rem; }
.w-7 { width: 1.75rem; }
.w-8 { width: 2rem; }
.w-9 { width: 2.25rem; }
.h-4 { height: 1rem; }
.h-5 { height: 1.25rem; }
.h-7 { height: 1.75rem; }
.h-8 { height: 2rem; }
.h-9 { height: 2.25rem; }
.h-14 { height: 3.5rem; }
.max-w-xl { max-width: 36rem; }
.max-w-2xl { max-width: 42rem; }
.max-w-3xl { max-width: 48rem; }
.max-w-6xl { max-width: 72rem; }
.mx-auto { margin-left: auto; margin-right: auto; }
.ml-2 { margin-left: .5rem; }
.ml-auto { margin-left: auto; }
.mt-0\.5 { margin-top: .125rem; }
.mt-1 { margin-top: .25rem; }
.mt-2 { margin-top: .5rem; }
.mt-3 { margin-top: .75rem; }
.mt-4 { margin-top: 1rem; }
.mt-7 { margin-top: 1.75rem; }
.mb-1 { margin-bottom: .25rem; }
.mb-2 { margin-bottom: .5rem; }
.mb-3 { margin-bottom: .75rem; }
.mb-4 { margin-bottom: 1rem; }
.mb-5 { margin-bottom: 1.25rem; }
.mb-6 { margin-bottom: 1.5rem; }
.p-2 { padding: .5rem; }
.p-3 { padding: .75rem; }
.p-4 { padding: 1rem; }
.p-5 { padding: 1.25rem; }
.p-6 { padding: 1.5rem; }
.p-8 { padding: 2rem; }
.px-1\.5 { padding-left: .375rem; padding-right: .375rem; }
.px-2 { padding-left: .5rem; padding-right: .5rem; }
.px-3 { padding-left: .75rem; padding-right: .75rem; }
.px-4 { padding-left: 1rem; padding-right: 1rem; }
.px-5 { padding-left: 1.25rem; padding-right: 1.25rem; }
.px-6 { padding-left: 1.5rem; padding-right: 1.5rem; }
.py-0\.5 { padding-top: .125rem; padding-bottom: .125rem; }
.py-2 { padding-top: .5rem; padding-bottom: .5rem; }
.py-3 { padding-top: .75rem; padding-bottom: .75rem; }
.py-6 { padding-top: 1.5rem; padding-bottom: 1.5rem; }
.py-10 { padding-top: 2.5rem; padding-bottom: 2.5rem; }
.py-12 { padding-top: 3rem; padding-bottom: 3rem; }
.pt-14 { padding-top: 3.5rem; }
.gap-2 { gap: .5rem; }
.gap-3 { gap: .75rem; }
.gap-4 { gap: 1rem; }
.gap-5 { gap: 1.25rem; }
.space-y-1 > * + * { margin-top: .25rem; }
.space-y-3 > * + * { margin-top: .75rem; }
.items-center { align-items: center; }
.items-start { align-items: flex-start; }
.justify-center { justify-content: center; }
.justify-between { justify-content: space-between; }
.flex-wrap { flex-wrap: wrap; }
.grid-cols-1 { grid-template-columns: repeat(1, minmax(0, 1fr)); }
.grid-cols-2 { grid-template-columns: repeat(2, minmax(0, 1fr)); }
.truncate { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.overflow-x-auto { overflow-x: auto; }
.whitespace-nowrap { white-space: nowrap; }
.whitespace-pre-wrap { white-space: pre-wrap; }
.rounded { border-radius: .25rem; }
.rounded-md { border-radius: .375rem; }
.rounded-lg { border-radius: .5rem; }
.rounded-xl { border-radius: .75rem; }
.rounded-2xl { border-radius: 1rem; }
.rounded-full { border-radius: 9999px; }
.rounded-none { border-radius: 0; }
.border { border: 1px solid rgba(55,50,36,.12); }
.border-0 { border: 0; }
.border-t { border-top: 1px solid rgba(55,50,36,.10); }
.border-black\/10 { border-color: rgba(0,0,0,.10); }
.border-gray-200 { border-color: rgb(229 231 235); }
.bg-transparent { background: transparent; }
.bg-white { background: rgb(255 255 255); }
.bg-gray-100 { background: rgb(243 244 246); }
.bg-slate-100 { background: rgb(241 245 249); }
.bg-slate-950, .bg-stone-950 { background: rgb(12 10 9); }
.bg-emerald-100 { background: rgb(209 250 229); }
.shadow-sm { box-shadow: 0 1px 2px rgba(0,0,0,.08); }
.text-center { text-align: center; }
.text-right { text-align: right; }
.text-xs { font-size: .75rem; line-height: 1rem; }
.text-sm { font-size: .875rem; line-height: 1.25rem; }
.text-xl { font-size: 1.25rem; line-height: 1.75rem; }
.text-2xl { font-size: 1.5rem; line-height: 2rem; }
.text-3xl { font-size: 1.875rem; line-height: 2.25rem; }
.text-\[10px\] { font-size: 10px; line-height: 1rem; }
.text-\[11px\] { font-size: 11px; line-height: 1rem; }
.font-medium { font-weight: 500; }
.font-semibold { font-weight: 600; }
.uppercase { text-transform: uppercase; }
.tracking-tight, .tracking-\[0\.18em\] { letter-spacing: 0; }
.leading-tight { line-height: 1.15; }
.leading-relaxed { line-height: 1.625; }
.text-white { color: rgb(255 255 255); }
.text-gray-400 { color: rgb(156 163 175); }
.text-gray-500 { color: rgb(107 114 128); }
.text-gray-600 { color: rgb(75 85 99); }
.text-gray-800 { color: rgb(31 41 55); }
.text-gray-900 { color: rgb(17 24 39); }
.text-slate-400 { color: rgb(148 163 184); }
.text-slate-500 { color: rgb(100 116 139); }
.text-slate-600 { color: rgb(71 85 105); }
.text-slate-800 { color: rgb(30 41 59); }
.text-stone-100 { color: rgb(245 245 244); }
.text-stone-50 { color: rgb(250 250 249); }
.text-stone-500 { color: rgb(120 113 108); }
.text-stone-600 { color: rgb(87 83 78); }
.text-stone-700 { color: rgb(68 64 60); }
.text-stone-800 { color: rgb(41 37 36); }
.text-emerald-700 { color: rgb(4 120 87); }
.text-amber-500 { color: rgb(245 158 11); }
.placeholder\:text-stone-500::placeholder { color: rgb(120 113 108); }
.outline-none { outline: none; }
.transition-colors { transition: color .18s ease, background-color .18s ease, border-color .18s ease; }
.transition-opacity { transition: opacity .18s ease; }
.hover\:opacity-90:hover { opacity: .9; }
.hover\:bg-gray-100:hover { background: rgb(243 244 246); }
.hover\:bg-white\/5:hover { background: rgba(255,255,255,.05); }
.hover\:bg-white\/40:hover { background: rgba(255,255,255,.40); }
.hover\:border-slate-400:hover { border-color: rgb(148 163 184); }
.hover\:text-stone-950:hover, .group:hover .group-hover\:text-stone-950 { color: rgb(12 10 9); }
.antialiased { -webkit-font-smoothing: antialiased; -moz-osx-font-smoothing: grayscale; }
.dark .dark\:hidden { display: none; }
.dark .dark\:inline { display: inline; }
.dark .dark\:bg-gray-800 { background: rgb(31 41 55); }
.dark .dark\:bg-gray-900 { background: rgb(17 24 39); }
.dark .dark\:bg-stone-100, .dark .dark\:bg-white { background: rgb(245 245 244); }
.dark .dark\:bg-emerald-400\/10 { background: rgba(52,211,153,.10); }
.dark .dark\:bg-white\/10 { background: rgba(255,255,255,.10); }
.dark .dark\:border-gray-800 { border-color: rgb(31 41 55); }
.dark .dark\:border-white\/10 { border-color: rgba(255,255,255,.10); }
.dark .dark\:text-gray-100 { color: rgb(243 244 246); }
.dark .dark\:text-gray-200 { color: rgb(229 231 235); }
.dark .dark\:text-gray-400 { color: rgb(156 163 175); }
.dark .dark\:text-slate-200 { color: rgb(226 232 240); }
.dark .dark\:text-slate-300 { color: rgb(203 213 225); }
.dark .dark\:text-slate-400 { color: rgb(148 163 184); }
.dark .dark\:text-slate-950 { color: rgb(2 6 23); }
.dark .dark\:text-stone-100 { color: rgb(245 245 244); }
.dark .dark\:text-stone-200 { color: rgb(231 229 228); }
.dark .dark\:text-stone-300 { color: rgb(214 211 209); }
.dark .dark\:text-stone-400 { color: rgb(168 162 158); }
.dark .dark\:text-stone-950 { color: rgb(12 10 9); }
.dark .dark\:text-emerald-300 { color: rgb(110 231 183); }
.dark .dark\:hover\:bg-white\/10:hover { background: rgba(255,255,255,.10); }
.dark .dark\:hover\:bg-gray-800:hover { background: rgb(31 41 55); }
.dark .dark\:hover\:border-white\/25:hover { border-color: rgba(255,255,255,.25); }
.dark .dark\:hover\:text-white:hover, .dark .group:hover .dark\:group-hover\:text-white { color: rgb(255 255 255); }
@media (min-width: 768px) {
  .md\:flex { display: flex; }
  .md\:hidden { display: none; }
  .md\:grid-cols-2 { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  .md\:grid-cols-6 { grid-template-columns: repeat(6, minmax(0, 1fr)); }
  .md\:p-8 { padding: 2rem; }
  .md\:pt-0 { padding-top: 0; }
  .md\:py-8 { padding-top: 2rem; padding-bottom: 2rem; }
  .md\:text-base { font-size: 1rem; line-height: 1.5rem; }
  .md\:text-5xl { font-size: 3rem; line-height: 1; }
}
@media (min-width: 1024px) {
  .lg\:grid-cols-\[0\.95fr_1\.05fr\] { grid-template-columns: .95fr 1.05fr; }
}
.memory-canvas {
  position: fixed;
  inset: 0;
  width: 100%;
  height: 100%;
  z-index: 0;
  pointer-events: none;
  opacity: .42;
}
.atlas-vignette {
  position: fixed;
  inset: 0;
  z-index: 0;
  pointer-events: none;
  background:
    linear-gradient(90deg, rgba(244,241,232,.88) 0%, rgba(244,241,232,.42) 28%, rgba(244,241,232,.16) 58%, rgba(244,241,232,.50) 100%),
    radial-gradient(circle at 72% 16%, rgba(255,255,255,.52), transparent 28%),
    radial-gradient(circle at 12% 92%, rgba(22,31,28,.26), transparent 24%);
}
.dark .atlas-vignette {
  background:
    linear-gradient(90deg, rgba(7,11,15,.84) 0%, rgba(7,11,15,.42) 36%, rgba(7,11,15,.18) 62%, rgba(7,11,15,.70) 100%),
    radial-gradient(circle at 70% 18%, rgba(214,165,92,.12), transparent 30%),
    radial-gradient(circle at 18% 88%, rgba(0,0,0,.42), transparent 24%);
}
.glass {
  background: rgba(253,250,241,.72);
  border: 1px solid rgba(46,50,40,.14);
  box-shadow: 0 18px 50px rgba(32,42,37,.08), inset 0 1px 0 rgba(255,255,255,.55);
}
.glass-strong {
  background:
    linear-gradient(180deg, rgba(253,250,241,.92), rgba(233,230,211,.78)),
    linear-gradient(90deg, rgba(110,96,68,.06), transparent);
  border-right: 1px solid rgba(46,50,40,.14);
  box-shadow: 12px 0 50px rgba(32,42,37,.10);
  backdrop-filter: blur(14px);
}
.dark .glass {
  background: rgba(12,18,20,.68);
  border-color: rgba(228,220,197,.12);
  box-shadow: 0 18px 50px rgba(0,0,0,.26);
}
.dark .glass-strong {
  background:
    linear-gradient(180deg, rgba(11,16,17,.94), rgba(24,28,22,.80)),
    linear-gradient(90deg, rgba(223,179,107,.06), transparent);
  border-right-color: rgba(228,220,197,.12);
  box-shadow: 12px 0 50px rgba(0,0,0,.30);
  backdrop-filter: blur(14px);
}
.panel {
  border-radius: 8px;
  background: linear-gradient(180deg, rgba(255,253,245,.58), rgba(255,253,245,.28));
  border: 0;
  border-top: 1px solid rgba(24,33,39,.14);
  border-left: 1px solid rgba(24,33,39,.08);
  box-shadow: none;
  backdrop-filter: blur(8px);
  transition: border-color .18s ease, transform .18s ease, box-shadow .18s ease;
}
.panel:hover {
  border-color: rgba(133,103,64,.38);
  transform: translateY(-1px);
  box-shadow: 0 22px 56px rgba(42,45,32,.08);
}
.dark .panel {
  background: linear-gradient(180deg, rgba(12,18,24,.58), rgba(12,18,24,.28));
  border-color: rgba(228,220,197,.12);
  box-shadow: none;
}
.dark .panel:hover {
  border-color: rgba(223,179,107,.28);
  box-shadow: 0 22px 56px rgba(0,0,0,.28);
}
.metric-card {
  border-radius: 8px;
  padding: 1rem;
  background: rgba(255,253,245,.42);
  border: 0;
  border-left: 2px solid rgba(141,94,42,.42);
  box-shadow: none;
  backdrop-filter: blur(8px);
}
.dark .metric-card {
  background: rgba(12,18,24,.44);
  border-color: rgba(223,179,107,.48);
  box-shadow: none;
}
.gradient-bg {
  background: #f4f1e8;
  background-image:
    url('/assets/memory-atlas.png'),
    linear-gradient(120deg, rgba(255,255,255,.7), rgba(226,236,220,.72) 48%, rgba(238,224,202,.62)),
    repeating-linear-gradient(0deg, rgba(24,33,39,.035) 0 1px, transparent 1px 34px),
    repeating-linear-gradient(90deg, rgba(24,33,39,.035) 0 1px, transparent 1px 34px);
  background-size: cover, auto, auto, auto;
  background-position: center, center, center, center;
  background-attachment: fixed, fixed, fixed, fixed;
}
.dark .gradient-bg {
  background: #070b0f;
  background-image:
    url('/assets/memory-atlas.png'),
    linear-gradient(120deg, rgba(20,34,31,.88), rgba(8,13,18,.95) 52%, rgba(37,29,22,.84)),
    repeating-linear-gradient(0deg, rgba(228,220,197,.04) 0 1px, transparent 1px 34px),
    repeating-linear-gradient(90deg, rgba(228,220,197,.035) 0 1px, transparent 1px 34px);
  background-size: cover, auto, auto, auto;
  background-position: center, center, center, center;
  background-attachment: fixed, fixed, fixed, fixed;
  background-blend-mode: multiply, normal, normal, normal;
}
.metric-card:hover {
  transform: translateY(-2px);
}
.metric-label {
  font-size: 10px;
  letter-spacing: .14em;
  text-transform: uppercase;
  color: rgb(102 112 90);
}
.dark .metric-label { color: rgb(172 164 139); }
.hero-panel {
  min-height: 21rem;
  position: relative;
  overflow: hidden;
}
.hero-panel::before {
  content: "";
  position: absolute;
  inset: -20%;
  background:
    radial-gradient(circle at 22% 24%, rgba(207,139,68,.20), transparent 24%),
    radial-gradient(circle at 72% 34%, rgba(88,109,80,.18), transparent 28%),
    linear-gradient(135deg, transparent 35%, rgba(255,255,255,.22));
  opacity: .9;
  pointer-events: none;
}
.hero-panel > * { position: relative; z-index: 1; }
.signal-line {
  height: 2px;
  border-radius: 999px;
  background: linear-gradient(90deg, rgba(89,101,76,.85), rgba(207,139,68,.55), transparent);
}
.dark .signal-line {
  background: linear-gradient(90deg, rgba(203,210,165,.82), rgba(225,158,75,.56), transparent);
}
.atlas-logo {
  width: 2.35rem;
  height: 2.35rem;
  color: rgb(68 77 57);
  filter: drop-shadow(0 10px 16px rgba(67,55,37,.18));
}
.dark .atlas-logo {
  color: rgb(221 205 160);
  filter: drop-shadow(0 10px 16px rgba(0,0,0,.28));
}
.sidebar-rule {
  height: 1px;
  background: linear-gradient(90deg, rgba(121,108,77,.36), transparent);
}
.nav-rail {
  background: rgba(253,250,241,.62);
  border: 1px solid rgba(46,50,40,.13);
  border-radius: 18px;
  box-shadow: 0 24px 70px rgba(34,34,24,.16);
  backdrop-filter: blur(18px);
}
.dark .nav-rail {
  background: rgba(8,13,18,.64);
  border-color: rgba(228,220,197,.12);
  box-shadow: 0 24px 70px rgba(0,0,0,.32);
}
.eyebrow {
  color: rgb(125 91 47);
  font-size: 10px;
  letter-spacing: .18em;
  text-transform: uppercase;
  font-weight: 700;
}
.dark .eyebrow { color: rgb(221 173 105); }
.bento-board {
  display: grid;
  grid-template-columns: 1.25fr .75fr;
  gap: 1rem;
}
@media (max-width: 1024px) {
  .bento-board { grid-template-columns: 1fr; }
}
.quiet-chip {
  display: inline-flex;
  align-items: center;
  height: 1.65rem;
  padding: 0 .65rem;
  border-radius: 999px;
  background: rgba(255,255,255,.46);
  border: 1px solid rgba(46,50,40,.12);
  color: rgb(83 91 72);
}
.dark .quiet-chip {
  background: rgba(255,255,255,.06);
  border-color: rgba(228,220,197,.12);
  color: rgb(210 202 178);
}
.nav-item {
  position: relative;
  overflow: hidden;
  width: 44px;
  height: 44px;
  justify-content: center;
}
.nav-item svg { transition: transform .22s ease, color .22s ease; }
.nav-item:hover svg { transform: translateX(2px) rotate(-2deg); }
.nav-label {
  position: absolute;
  left: 54px;
  top: 50%;
  transform: translateY(-50%) translateX(-6px);
  opacity: 0;
  pointer-events: none;
  white-space: nowrap;
  border-radius: 999px;
  padding: .35rem .7rem;
  background: rgba(18,20,15,.88);
  color: rgba(255,253,245,.94);
  font-size: 12px;
  box-shadow: 0 12px 28px rgba(0,0,0,.18);
  transition: opacity .18s ease, transform .18s ease;
}
.nav-item:hover .nav-label {
  opacity: 1;
  transform: translateY(-50%) translateX(0);
}
.nav-item::after {
  content: "";
  position: absolute;
  inset: 1px;
  border-radius: 8px;
  pointer-events: none;
  background: linear-gradient(90deg, transparent, rgba(232,171,92,.10), transparent);
  opacity: 0;
  transform: translateX(-60%);
  transition: opacity .2s ease, transform .35s ease;
}
.nav-item:hover::after {
  opacity: 1;
  transform: translateX(60%);
}
.nav-glyph {
  width: 1.05rem;
  height: 1.05rem;
  color: rgb(93 103 86);
}
.dark .nav-glyph {
  color: rgb(201 190 161);
}
.nav-rail { display: none !important; }
.product-topbar {
  position: relative;
  z-index: 60;
  width: min(1180px, calc(100% - 32px));
  height: 62px;
  display: none;
  align-items: center;
  justify-content: space-between;
  margin: 18px auto 0;
  padding: 0 10px 0 16px;
  border-radius: 999px;
  background: rgba(252,248,236,.68);
  border: 1px solid rgba(53,49,38,.16);
  box-shadow: 0 18px 60px rgba(40,38,27,.14), inset 0 1px 0 rgba(255,255,255,.62);
  backdrop-filter: blur(18px) saturate(1.08);
}
@media (min-width: 768px) {
  .product-topbar { display: flex; }
}
.dark .product-topbar {
  background: rgba(9,13,15,.68);
  border-color: rgba(229,218,188,.14);
  box-shadow: 0 18px 60px rgba(0,0,0,.32), inset 0 1px 0 rgba(255,255,255,.06);
}
.brand-lockup {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 210px;
}
.topnav {
  display: flex;
  align-items: center;
  gap: 4px;
  border-radius: 999px;
  padding: 4px;
  background: rgba(255,255,255,.32);
  border: 1px solid rgba(53,49,38,.09);
}
.dark .topnav {
  background: rgba(255,255,255,.05);
  border-color: rgba(229,218,188,.09);
}
.top-link {
  height: 38px;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 0 14px;
  border-radius: 999px;
  font-size: 13px;
  color: rgb(71 75 62);
  transition: background .18s ease, color .18s ease, transform .18s ease;
}
.top-link:hover {
  background: rgba(255,255,255,.46);
  transform: translateY(-1px);
}
.top-link.is-active {
  background: rgb(53 49 38) !important;
  color: rgb(250 248 240) !important;
  box-shadow: 0 1px 2px rgba(53,49,38,.22), inset 0 0 0 1px rgba(255,255,255,.05);
  transform: none;
}
.top-link.is-active:hover {
  background: rgb(53 49 38) !important;
  color: rgb(250 248 240) !important;
  transform: none;
}
.dark .top-link {
  color: rgb(214 206 181);
}
.dark .top-link:hover {
  background: rgba(255,255,255,.08);
}
.dark .top-link.is-active {
  background: rgb(245 245 244) !important;
  color: rgb(28 25 23) !important;
}
.top-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 270px;
  justify-content: flex-end;
}
.locale-switch {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  height: 34px;
  padding: 3px;
  border-radius: 999px;
  background: rgba(255,255,255,.34);
  border: 1px solid rgba(53,49,38,.10);
}
.locale-switch button {
  height: 26px;
  padding: 0 9px;
  border-radius: 999px;
  font-size: 11px;
  color: rgb(90 86 70);
}
.locale-switch button.is-active {
  background: rgb(28 25 23);
  color: rgb(250 250 249);
}
.dark .locale-switch {
  background: rgba(255,255,255,.06);
  border-color: rgba(229,218,188,.10);
}
.dark .locale-switch button { color: rgb(214 206 181); }
.dark .locale-switch button.is-active {
  background: rgb(245 245 244);
  color: rgb(28 25 23);
}
.status-pill {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  height: 34px;
  padding: 0 12px;
  border-radius: 999px;
  font-size: 12px;
  color: rgb(76 83 67);
  background: rgba(255,255,255,.34);
  border: 1px solid rgba(53,49,38,.10);
}
.dark .status-pill {
  color: rgb(214 206 181);
  background: rgba(255,255,255,.06);
  border-color: rgba(229,218,188,.10);
}
.status-dot {
  width: 7px;
  height: 7px;
  border-radius: 999px;
  background: rgb(22 163 74);
  box-shadow: 0 0 0 4px rgba(22,163,74,.10);
}
.workbench {
  padding-top: 26px;
}
@media (max-width: 767px) {
  .workbench { padding-top: 72px; }
}
.workbench .hero-panel {
  border-radius: 24px;
  min-height: 420px;
  background:
    linear-gradient(110deg, rgba(255,253,245,.82), rgba(255,253,245,.26) 62%, rgba(255,253,245,.08)),
    radial-gradient(circle at 76% 22%, rgba(188,124,55,.18), transparent 26%);
  border: 1px solid rgba(53,49,38,.13);
  box-shadow: 0 28px 90px rgba(41,39,28,.16);
}
.dark .workbench .hero-panel {
  background:
    linear-gradient(110deg, rgba(10,15,16,.84), rgba(10,15,16,.32) 62%, rgba(10,15,16,.16)),
    radial-gradient(circle at 76% 22%, rgba(209,139,58,.15), transparent 26%);
  border-color: rgba(229,218,188,.13);
  box-shadow: 0 28px 90px rgba(0,0,0,.34);
}
.workbench .bento-board {
  grid-template-columns: minmax(0, 1fr);
  gap: 18px;
}
.workbench .metric-card {
  border-radius: 18px;
  border-left: 0;
  border-top: 1px solid rgba(53,49,38,.16);
  background: rgba(255,253,245,.40);
}
.workbench .panel {
  border-radius: 18px;
  background: rgba(255,253,245,.48);
  border: 1px solid rgba(53,49,38,.11);
}
.dark .workbench .panel,
.dark .workbench .metric-card {
  background: rgba(10,15,16,.46);
  border-color: rgba(229,218,188,.11);
}
.command-surface {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 320px;
  gap: 18px;
  align-items: stretch;
}
@media (max-width: 1024px) {
  .command-surface { grid-template-columns: 1fr; }
}
.atlas-board {
  position: relative;
  min-height: 500px;
  border-radius: 30px;
  overflow: hidden;
  border: 1px solid rgba(55,50,36,.14);
  background:
    linear-gradient(100deg, rgba(255,253,245,.84), rgba(255,253,245,.28) 62%, rgba(255,253,245,.10)),
    radial-gradient(circle at 76% 20%, rgba(197,128,50,.22), transparent 26%);
  box-shadow: 0 34px 110px rgba(39,36,25,.18);
}
.dark .atlas-board {
  border-color: rgba(229,218,188,.14);
  background:
    linear-gradient(100deg, rgba(9,13,14,.86), rgba(9,13,14,.36) 62%, rgba(9,13,14,.16)),
    radial-gradient(circle at 76% 20%, rgba(218,145,54,.16), transparent 26%);
  box-shadow: 0 34px 110px rgba(0,0,0,.38);
}
.atlas-board::before {
  content: "";
  position: absolute;
  inset: 0;
  background:
    linear-gradient(90deg, rgba(43,39,27,.08) 1px, transparent 1px),
    linear-gradient(0deg, rgba(43,39,27,.07) 1px, transparent 1px);
  background-size: 72px 72px;
  mask-image: linear-gradient(90deg, black, transparent 80%);
  pointer-events: none;
}
.board-copy {
  position: relative;
  z-index: 1;
  max-width: 760px;
  padding: 42px;
}
.board-title {
  font-size: 64px;
  line-height: .94;
  letter-spacing: 0;
  font-weight: 650;
}
.board-actions {
  position: absolute;
  left: 42px;
  right: 42px;
  bottom: 34px;
  z-index: 1;
}
.command-input {
  display: flex;
  align-items: center;
  gap: 10px;
  border-radius: 20px;
  padding: 8px;
  background: rgba(255,253,245,.72);
  border: 1px solid rgba(55,50,36,.14);
  box-shadow: 0 18px 50px rgba(44,40,28,.14);
  backdrop-filter: blur(14px);
}
.dark .command-input {
  background: rgba(9,13,14,.72);
  border-color: rgba(229,218,188,.14);
}
.search-field {
  min-width: 0;
  flex: 1;
}
.field-label {
  display: block;
  margin-bottom: 5px;
  font-size: 10px;
  letter-spacing: .14em;
  text-transform: uppercase;
  color: rgb(104 98 79);
}
.dark .field-label { color: rgb(188 178 148); }
.scope-select {
  min-width: 172px;
  border-radius: 14px;
  border: 1px solid rgba(55,50,36,.12);
  background: rgba(255,255,255,.44);
  padding: 11px 36px 11px 12px;
  font-size: 13px;
  outline: none;
}
.dark .scope-select {
  background: rgba(255,255,255,.06);
  border-color: rgba(229,218,188,.12);
}
.stat-strip {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 1px;
  overflow: hidden;
  border-radius: 22px;
  border: 1px solid rgba(55,50,36,.12);
  background: rgba(55,50,36,.12);
}
.stat-tile {
  padding: 18px;
  background: rgba(255,253,245,.58);
  backdrop-filter: blur(12px);
}
.dark .stat-strip { background: rgba(229,218,188,.12); }
.dark .stat-tile { background: rgba(9,13,14,.58); }
.endpoint-row {
  display: grid;
  grid-template-columns: 70px minmax(0, 1fr) 150px;
  gap: 14px;
  align-items: center;
  padding: 14px 0;
  border-top: 1px solid rgba(55,50,36,.10);
}
.dark .endpoint-row { border-color: rgba(229,218,188,.10); }
.ops-shell {
  display: grid;
  gap: 16px;
}
.ops-header {
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  gap: 16px;
  align-items: start;
  padding: 8px 2px 2px;
}
.ops-title {
  font-size: 34px;
  line-height: 1.1;
  font-weight: 650;
  letter-spacing: 0;
  color: rgb(28 25 23);
}
.dark .ops-title { color: rgb(245 245 244); }
.ops-subtitle {
  margin-top: 7px;
  max-width: 720px;
  color: rgb(91 86 70);
  font-size: 14px;
  line-height: 1.6;
}
.dark .ops-subtitle { color: rgb(188 178 148); }
.ops-status {
  display: inline-flex;
  align-items: center;
  gap: 9px;
  height: 38px;
  padding: 0 14px;
  border-radius: 999px;
  border: 1px solid rgba(55,50,36,.12);
  background: rgba(255,253,245,.62);
  font-size: 13px;
}
.dark .ops-status {
  background: rgba(9,13,14,.58);
  border-color: rgba(229,218,188,.12);
}
.ops-search {
  position: relative;
  overflow: hidden;
  border-radius: 24px;
  border: 1px solid rgba(55,50,36,.13);
  background:
    linear-gradient(100deg, rgba(255,253,245,.84), rgba(255,253,245,.48)),
    url('/assets/memory-atlas.png');
  background-size: auto, cover;
  background-position: center;
  box-shadow: 0 24px 80px rgba(39,36,25,.12);
}
.ops-search::before {
  content: "";
  position: absolute;
  inset: 0;
  background: linear-gradient(90deg, rgba(255,253,245,.88), rgba(255,253,245,.52) 62%, rgba(255,253,245,.30));
  pointer-events: none;
}
.dark .ops-search {
  border-color: rgba(229,218,188,.13);
  background:
    linear-gradient(100deg, rgba(9,13,14,.88), rgba(9,13,14,.52)),
    url('/assets/memory-atlas.png');
  background-size: auto, cover;
  background-position: center;
}
.dark .ops-search::before {
  background: linear-gradient(90deg, rgba(9,13,14,.92), rgba(9,13,14,.62) 62%, rgba(9,13,14,.42));
}
.ops-search-inner {
  position: relative;
  z-index: 1;
  display: grid;
  grid-template-columns: minmax(0, 1fr);
  gap: 16px;
  padding: 22px;
}
.ops-search form {
  max-width: 100%;
}
.ops-search .command-input {
  display: grid;
  grid-template-columns: minmax(220px, 1fr) minmax(190px, 260px) auto;
  align-items: end;
}
.ops-search .command-input button {
  min-height: 48px;
  white-space: nowrap;
}
.ops-kpis {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 1px;
  overflow: hidden;
  border-radius: 18px;
  border: 1px solid rgba(55,50,36,.11);
  background: rgba(55,50,36,.11);
}
.ops-kpi {
  min-height: 92px;
  padding: 16px;
  background: rgba(255,253,245,.62);
}
.dark .ops-kpis { background: rgba(229,218,188,.11); }
.dark .ops-kpi { background: rgba(9,13,14,.60); }
.ops-grid {
  display: grid;
  grid-template-columns: minmax(0, 1.08fr) minmax(320px, .72fr);
  gap: 16px;
}
.work-panel {
  border-radius: 20px;
  border: 1px solid rgba(55,50,36,.12);
  background: rgba(255,253,245,.58);
  backdrop-filter: blur(12px);
  overflow: hidden;
}
.dark .work-panel {
  border-color: rgba(229,218,188,.12);
  background: rgba(9,13,14,.54);
}
.work-panel-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 16px 18px;
  border-bottom: 1px solid rgba(55,50,36,.10);
}
.dark .work-panel-head { border-color: rgba(229,218,188,.10); }
.activity-stream a {
  display: block;
  padding: 16px 18px;
  border-top: 1px solid rgba(55,50,36,.10);
}
.activity-stream a:first-child { border-top: 0; }
.dark .activity-stream a { border-color: rgba(229,218,188,.10); }
.health-list {
  display: grid;
  gap: 1px;
  background: rgba(55,50,36,.10);
}
.health-list > * {
  background: rgba(255,253,245,.52);
}
.dark .health-list { background: rgba(229,218,188,.10); }
.dark .health-list > * { background: rgba(9,13,14,.48); }
.inspector {
  border-radius: 30px;
  background: rgba(255,253,245,.58);
  border: 1px solid rgba(55,50,36,.13);
  box-shadow: 0 34px 100px rgba(39,36,25,.12);
  backdrop-filter: blur(14px);
  overflow: hidden;
}
.dark .inspector {
  background: rgba(9,13,14,.56);
  border-color: rgba(229,218,188,.13);
  box-shadow: 0 34px 100px rgba(0,0,0,.32);
}
.inspector-row {
  display: grid;
  grid-template-columns: 1fr auto;
  gap: 16px;
  padding: 18px 20px;
  border-top: 1px solid rgba(55,50,36,.10);
}
.dark .inspector-row {
  border-color: rgba(229,218,188,.10);
}
.ledger {
  border-radius: 24px;
  overflow: hidden;
  background: rgba(255,253,245,.52);
  border: 1px solid rgba(55,50,36,.12);
  backdrop-filter: blur(12px);
}
.dark .ledger {
  background: rgba(9,13,14,.48);
  border-color: rgba(229,218,188,.12);
}
.ledger-head,
.ledger-row {
  display: grid;
  grid-template-columns: minmax(0, 1.3fr) 110px 94px;
  gap: 16px;
  align-items: center;
  padding: 14px 18px;
}
.ledger-head {
  font-size: 10px;
  letter-spacing: .16em;
  text-transform: uppercase;
  color: rgb(99 100 78);
  background: rgba(255,255,255,.28);
}
.ledger-row {
  border-top: 1px solid rgba(55,50,36,.10);
  font-size: 14px;
}
.ledger-row:hover {
  background: rgba(255,255,255,.30);
}
.dark .ledger-head { background: rgba(255,255,255,.04); color: rgb(184 176 148); }
.dark .ledger-row { border-color: rgba(229,218,188,.10); }
.memory-feed a {
  display: block;
  padding: 16px 0;
  border-top: 1px solid rgba(55,50,36,.10);
}
.dark .memory-feed a { border-color: rgba(229,218,188,.10); }
.search-result mark {
  border-radius: 4px;
  padding: 0 .16em;
  background: rgba(245, 158, 11, .28);
  color: inherit;
  box-shadow: inset 0 -0.38em rgba(245, 158, 11, .22);
}
.dark .search-result mark {
  background: rgba(251, 191, 36, .24);
  box-shadow: inset 0 -0.38em rgba(251, 191, 36, .18);
}
@media (max-width: 640px) {
  .atlas-board {
    min-height: 640px;
    border-radius: 22px;
  }
  .board-copy { padding: 28px; }
  .board-title { font-size: 52px; }
  .board-actions {
    left: 18px;
    right: 18px;
    bottom: 22px;
  }
  .command-input {
    align-items: stretch;
    flex-direction: column;
    border-radius: 18px;
  }
  .scope-select {
    width: 100%;
    min-width: 0;
  }
  .stat-strip {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
  .endpoint-row {
    grid-template-columns: 54px minmax(0, 1fr);
  }
  .endpoint-row span:last-child {
    grid-column: 2;
  }
  .ledger-head,
  .ledger-row {
    grid-template-columns: minmax(0, 1fr) 70px 52px;
    gap: 10px;
    padding-left: 14px;
    padding-right: 14px;
  }
  .ops-header,
  .ops-search-inner,
  .ops-grid {
    grid-template-columns: 1fr;
  }
  .ops-header {
    align-items: start;
  }
  .ops-title {
    font-size: 28px;
  }
  .ops-search .command-input {
    grid-template-columns: 1fr;
  }
  .ops-kpis {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }
  .top-actions {
    min-width: 0;
  }
}

/* ─────────────────────────────────────────────────────────────────────────────────
   Collections viewer — editorial layout, no emoji, single accent.
   Tokens: --ink (text), --ink-muted, --accent (#A66A2D), --paper, --line.
   ───────────────────────────────────────────────────────────────────────────────── */
:root {
  --ink:        rgb(40 38 32);
  --ink-muted:  rgb(120 113 96);
  --ink-faint:  rgb(168 159 138);
  --paper:      rgba(255, 252, 246, 0.76);
  --paper-2:    rgba(252, 248, 238, 0.62);
  --line:       rgba(53, 49, 38, 0.10);
  --line-soft:  rgba(53, 49, 38, 0.06);
  --accent:     rgb(166, 106, 45);     /* book/playbook */
  --accent-2:   rgb(85, 113, 78);      /* project (deep olive, not emerald) */
  --accent-3:   rgb(140, 132, 112);    /* autolog (warm grey) */
  --accent-4:   rgb(96, 92, 80);       /* archive */
}
.dark {
  --ink:        rgb(238 230 210);
  --ink-muted:  rgb(170 161 138);
  --ink-faint:  rgb(122 115 96);
  --paper:      rgba(28, 26, 22, 0.62);
  --paper-2:    rgba(34, 31, 26, 0.46);
  --line:       rgba(229, 218, 188, 0.10);
  --line-soft:  rgba(229, 218, 188, 0.06);
  --accent:     rgb(208, 152, 88);
  --accent-2:   rgb(149, 178, 138);
  --accent-3:   rgb(180, 172, 152);
  --accent-4:   rgb(170, 163, 144);
}
.num {
  font-variant-numeric: tabular-nums;
  font-feature-settings: "tnum", "lnum";
  letter-spacing: -0.01em;
}
.num-accent { color: var(--accent); }
.num-muted  { color: var(--ink-faint); }

.col-header {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  gap: 24px;
  margin: 4px 0 20px;
  padding-bottom: 18px;
  border-bottom: 1px solid var(--line);
}
.col-title {
  font-size: 26px;
  line-height: 1.2;
  font-weight: 600;
  letter-spacing: -0.018em;
  color: var(--ink);
}
.col-subtitle {
  margin-top: 6px;
  font-size: 12.5px;
  color: var(--ink-muted);
  letter-spacing: 0;
}
.col-key {
  font-style: normal;
  font-weight: 500;
  padding: 0 6px;
  border-radius: 4px;
  background: var(--line-soft);
}
.col-key-pb { color: var(--accent);   }
.col-key-pr { color: var(--accent-2); }
.col-key-al { color: var(--ink-faint);}
.col-search-link {
  font-size: 11px;
  letter-spacing: 0.16em;
  text-transform: uppercase;
  color: var(--ink-muted);
  padding-bottom: 2px;
  border-bottom: 1px solid var(--line);
  transition: color .18s ease, border-color .18s ease;
}
.col-search-link:hover {
  color: var(--accent);
  border-color: var(--accent);
}

.col-summary {
  margin-bottom: 28px;
  padding: 22px 24px 20px;
  background: var(--paper);
  border: 1px solid var(--line);
  border-radius: 6px;
  backdrop-filter: blur(8px);
  -webkit-backdrop-filter: blur(8px);
}
.col-summary-grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 0;
}
.col-metric {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 4px 20px;
  border-left: 1px solid var(--line-soft);
}
.col-metric:first-child { border-left: 0; padding-left: 4px; }
.col-metric-label {
  font-size: 10.5px;
  letter-spacing: 0.18em;
  text-transform: uppercase;
  color: var(--ink-faint);
  font-weight: 500;
}
.col-metric-value {
  font-size: 30px;
  line-height: 1;
  font-weight: 300;
  color: var(--ink);
}
.col-metric-sub {
  font-size: 11.5px;
  color: var(--ink-muted);
  letter-spacing: 0;
}
.col-summary-bar {
  margin-top: 18px;
  height: 3px;
  border-radius: 999px;
  background: var(--line-soft);
  overflow: hidden;
  position: relative;
}
.col-summary-bar-manual {
  display: block;
  height: 100%;
  background: var(--accent);
  border-radius: 999px;
  transition: width .4s ease;
}
@media (max-width: 768px) {
  .col-summary-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 16px 0; }
  .col-metric { border-left: 0; padding: 0 4px; }
}

.col-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 14px;
}
@media (max-width: 768px) { .col-grid { grid-template-columns: 1fr; } }

.col-card {
  position: relative;
  display: flex;
  align-items: stretch;
  gap: 0;
  padding: 18px 18px 18px 22px;
  background: var(--paper);
  border: 1px solid var(--line);
  border-radius: 6px;
  overflow: hidden;
  transition: border-color .18s ease, background .18s ease, transform .18s ease;
  backdrop-filter: blur(6px);
  -webkit-backdrop-filter: blur(6px);
}
.col-card:hover {
  border-color: rgba(53, 49, 38, 0.22);
  background: rgba(255, 253, 248, 0.86);
  transform: translateY(-1px);
}
.dark .col-card:hover {
  border-color: rgba(229, 218, 188, 0.20);
  background: rgba(36, 33, 28, 0.72);
}
.col-card-bar {
  position: absolute;
  left: 0; top: 0; bottom: 0;
  width: 3px;
}
.col-accent-playbook .col-card-bar { background: var(--accent);   }
.col-accent-project  .col-card-bar { background: var(--accent-2); }
.col-accent-autolog  .col-card-bar { background: var(--accent-3); }
.col-accent-archive  .col-card-bar { background: var(--accent-4); }

.col-accent-playbook .col-card-icon { color: var(--accent);   }
.col-accent-project  .col-card-icon { color: var(--accent-2); }
.col-accent-autolog  .col-card-icon { color: var(--accent-3); }
.col-accent-archive  .col-card-icon { color: var(--accent-4); }

.col-card-body {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.col-card-head {
  display: flex;
  align-items: baseline;
  gap: 10px;
  min-width: 0;
}
.col-card-icon {
  width: 16px;
  height: 16px;
  flex-shrink: 0;
  align-self: center;
  margin-top: -1px;
}
.col-card-name {
  font-size: 15px;
  font-weight: 600;
  color: var(--ink);
  letter-spacing: -0.005em;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  flex: 1;
  min-width: 0;
}
.col-card-kind {
  font-size: 9.5px;
  letter-spacing: 0.22em;
  text-transform: uppercase;
  color: var(--ink-faint);
  font-weight: 500;
  flex-shrink: 0;
}
.col-card-desc {
  font-size: 12.5px;
  line-height: 1.55;
  color: var(--ink-muted);
  letter-spacing: 0;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
.col-card-stats {
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 0;
  margin: 4px 0 0;
  padding-top: 12px;
  border-top: 1px solid var(--line-soft);
}
.col-card-stats > div {
  display: flex;
  flex-direction: column;
  gap: 2px;
  padding-left: 12px;
  border-left: 1px solid var(--line-soft);
}
.col-card-stats > div:first-child {
  border-left: 0;
  padding-left: 0;
}
.col-card-stats dt {
  font-size: 9.5px;
  letter-spacing: 0.16em;
  text-transform: uppercase;
  color: var(--ink-faint);
  font-weight: 500;
  margin: 0;
}
.col-card-stats dd {
  font-size: 17px;
  font-weight: 400;
  color: var(--ink);
  margin: 0;
  line-height: 1.1;
}
.col-card-arrow {
  width: 16px;
  height: 16px;
  align-self: center;
  margin-left: 14px;
  color: var(--ink-faint);
  flex-shrink: 0;
  transition: transform .18s ease, color .18s ease;
}
.col-card:hover .col-card-arrow {
  color: var(--accent);
  transform: translateX(2px);
}
.col-empty {
  padding: 48px 20px;
  text-align: center;
  font-size: 13px;
  color: var(--ink-muted);
  background: var(--paper);
  border: 1px dashed var(--line);
  border-radius: 6px;
}
</style>
</head>
<body class="gradient-bg text-gray-900 dark:text-gray-100 min-h-screen antialiased">
<div class="atlas-vignette"></div>
<canvas id="memory-field" class="memory-canvas"></canvas>

<header class="product-topbar hidden md:flex">
 <a href="/" class="brand-lockup">
  <svg class="atlas-logo" viewBox="0 0 64 64" fill="none" aria-hidden="true">
   <path d="M9 34c8-18 29-27 45-15" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"/>
   <path d="M12 41c9-13 25-19 39-10" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" opacity=".78"/>
   <path d="M20 28c3-6 11-9 17-6 7 3 8 12 2 17-6 5-17 2-19-7" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"/>
   <circle cx="47" cy="17" r="4" fill="currentColor" opacity=".55"/>
  </svg>
  <span>
   <span class="block text-sm font-semibold tracking-tight">Memnest</span>
   <span class="block text-[10px] uppercase tracking-[0.18em] text-stone-500 dark:text-stone-400" data-i18n="brand.subtitle">Memory atlas</span>
  </span>
 </a>
 <nav class="topnav">
  <a href="/" class="top-link __ACTIVE_DASHBOARD__" data-i18n="nav.dashboard">대시보드</a>
  <a href="/viewer/collections" class="top-link __ACTIVE_COLLECTIONS__" data-i18n="nav.collections">컬렉션</a>
  <a href="/viewer/search" class="top-link __ACTIVE_SEARCH__" data-i18n="nav.search">검색</a>
  <a href="/stats?format=html" class="top-link">System</a>
 </nav>
 <div class="top-actions">
  <div class="locale-switch" aria-label="language">
   <button type="button" data-lang-button="ko">KR</button>
   <button type="button" data-lang-button="en">EN</button>
  </div>
  <a href="/stats?format=html" class="status-pill"><span class="status-dot"></span><span data-i18n="status.ok">서비스 정상</span></a>
  <button onclick="toggleDark()" title="테마 전환" class="w-9 h-9 rounded-full flex items-center justify-center text-slate-600 dark:text-slate-300 hover:bg-white/40 dark:hover:bg-white/10 transition-colors"><span class="dark:hidden text-sm">◐</span><span class="hidden dark:inline text-sm text-amber-500">◑</span></button>
 </div>
</header>

<!-- Mobile Header -->
<div class="md:hidden fixed top-0 left-0 right-0 h-14 glass-strong z-40 flex items-center justify-between px-4">
 <div class="flex items-center gap-2"><svg class="w-7 h-7 text-stone-700 dark:text-stone-200" viewBox="0 0 64 64" fill="none"><path d="M9 34c8-18 29-27 45-15" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"/><path d="M20 28c3-6 11-9 17-6 7 3 8 12 2 17-6 5-17 2-19-7" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"/><circle cx="47" cy="17" r="4" fill="currentColor" opacity=".55"/></svg><span class="text-sm font-semibold">Memnest</span><span class="ml-2 text-xs text-stone-500" data-i18n="status.ok">서비스 정상</span></div>
 <button onclick="document.getElementById('mob').classList.toggle('hidden')" class="p-2 text-gray-500" aria-label="menu"><svg class="w-5 h-5" viewBox="0 0 24 24" fill="none"><path d="M5 7h14M5 12h14M5 17h14" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/></svg></button>
</div>
<div id="mob" class="hidden md:hidden fixed top-14 left-0 right-0 glass z-30 p-3 space-y-1">
 <a href="/" class="block px-3 py-2 rounded-md text-sm text-gray-600 dark:text-gray-400 hover:bg-white/5" data-i18n="nav.dashboard">대시보드</a>
 <a href="/viewer/collections" class="block px-3 py-2 rounded-md text-sm text-gray-600 dark:text-gray-400 hover:bg-white/5" data-i18n="nav.collections">컬렉션</a>
 <a href="/viewer/search" class="block px-3 py-2 rounded-md text-sm text-gray-600 dark:text-gray-400 hover:bg-white/5" data-i18n="nav.search">검색</a>
 <a href="/stats?format=html" class="block px-3 py-2 rounded-md text-sm text-gray-600 dark:text-gray-400 hover:bg-white/5">System</a>
 <div class="locale-switch mt-2">
  <button type="button" data-lang-button="ko">KR</button>
  <button type="button" data-lang-button="en">EN</button>
 </div>
</div>

<!-- Main Content -->
<main class="workbench relative z-10 pt-14 md:pt-0 min-h-screen">
 <div class="max-w-6xl mx-auto px-5 py-6 md:py-8">
  __CONTENT__
 </div>
</main>

<script>
function toggleDark() {
 var h = document.documentElement;
 h.classList.toggle('dark');
 localStorage.setItem('theme', h.classList.contains('dark') ? 'dark' : 'light');
}
// Only honor an explicit user choice from the theme toggle. OS-level
// prefers-color-scheme is intentionally ignored — this palette is paper-tinted
// and the dark tokens are an opt-in variant.
if (localStorage.getItem('theme') === 'dark') {
 document.documentElement.classList.add('dark');
}
var i18n = {
 ko: {
  'brand.subtitle': 'Memory atlas',
  'nav.dashboard': '대시보드',
  'nav.collections': '컬렉션',
  'nav.search': '검색',
  'status.ok': '서비스 정상',
  'dashboard.eyebrow': 'Workspace memory',
  'dashboard.title': '메모리 운영 콘솔',
  'dashboard.subtitle': '저장된 대화와 작업 기록을 검색하고, 컬렉션별 규모와 최근 입력을 확인합니다. 컬렉션은 저장 요청의 project 값으로 묶이며 값이 없으면 default로 들어갑니다.',
  'dashboard.searchTitle': '통합 검색',
  'dashboard.searchHelp': '결정, 오류, 설정, 코드 단서를 한 번에 찾습니다.',
  'field.query': '검색어',
  'field.scope': '범위',
  'placeholder.search': '예: 배포 결정, OAuth 오류, PostgreSQL 설정',
  'button.search': '검색',
  'scope.all': '전체 컬렉션',
  'metric.memories': '메모리',
  'metric.facts': '지식',
  'metric.notes': '노트',
  'metric.graph': '그래프',
  'panel.collections': '컬렉션',
  'panel.recent': '최근 입력',
  'panel.health': '시스템 상태',
  'action.open': '열기',
  'action.viewSystem': '상태와 API 보기',
  'empty.collections': '아직 수집된 컬렉션이 없습니다',
  'empty.recent': '최근 메모리가 없습니다',
  'search.title': '검색',
  'search.subtitle': '검색어를 입력하고 필요한 컬렉션만 좁혀 봅니다.',
  'search.collections': '컬렉션 보기',
  'search.empty': '검색어를 입력하세요',
  'collections.title': '컬렉션',
  'collections.subtitle': 'cwd 이름으로 자동 분류된 버킷들. playbook만 예외로, 어디서든 검색하는 cross-project 메모다.',
 'system.title': '운영 상태와 연동 경로.',
  'system.subtitle': '현재 저장 규모와 외부 연동에 필요한 경로를 확인합니다. 자동화에서 필요한 원시 응답은 디버그 링크로만 열어 둡니다.',
  'system.endpoints': '연동 경로',
  'system.debug': '디버그 JSON',
  'unit.items': '개',
  'unit.memories': '개 메모리',
  'result.order': '관련도순',
  'result.emptySuffix': '에 대한 결과가 없습니다'
 },
 en: {
  'brand.subtitle': 'Memory atlas',
  'nav.dashboard': 'Dashboard',
  'nav.collections': 'Collections',
  'nav.search': 'Search',
  'status.ok': 'Operational',
  'dashboard.eyebrow': 'Workspace memory',
  'dashboard.title': 'Memory operations',
  'dashboard.subtitle': 'Search stored conversations and work history, review collection volume, and inspect recent entries. Collections are grouped by the project value sent when memory is saved; missing values go to default.',
  'dashboard.searchTitle': 'Unified search',
  'dashboard.searchHelp': 'Find decisions, errors, settings, and code context in one pass.',
  'field.query': 'Query',
  'field.scope': 'Scope',
  'placeholder.search': 'e.g. deployment decision, OAuth error, PostgreSQL config',
  'button.search': 'Search',
  'scope.all': 'All collections',
  'metric.memories': 'Memories',
  'metric.facts': 'Facts',
  'metric.notes': 'Notes',
  'metric.graph': 'Graph',
  'panel.collections': 'Collections',
  'panel.recent': 'Recent entries',
  'panel.health': 'System health',
  'action.open': 'Open',
  'action.viewSystem': 'View status and API',
  'empty.collections': 'No collections have been captured yet',
  'empty.recent': 'No recent memories',
  'search.title': 'Search',
  'search.subtitle': 'Enter a query and narrow it to a collection when needed.',
  'search.collections': 'View collections',
  'search.empty': 'Enter a search query',
  'collections.title': 'Collections',
  'collections.subtitle': 'Buckets auto-named after your cwd. Only playbook is the exception — a cross-project notebook searched from anywhere.',
  'system.title': 'Status and integration paths.',
  'system.subtitle': 'Review current storage volume and the endpoints needed for integration. Raw responses are kept under a debug link.',
  'system.endpoints': 'Endpoint catalog',
  'system.debug': 'Debug JSON',
  'unit.items': '',
  'unit.memories': ' memories',
  'result.order': 'sorted by relevance',
  'result.emptySuffix': 'returned no results'
 }
};
function setLang(lang) {
 var selected = i18n[lang] ? lang : 'en';
 localStorage.setItem('locale', selected);
 document.documentElement.lang = selected;
 document.querySelectorAll('[data-i18n]').forEach(function(node) {
  var key = node.getAttribute('data-i18n');
  if (i18n[selected][key]) node.textContent = i18n[selected][key];
 });
 document.querySelectorAll('[data-i18n-placeholder]').forEach(function(node) {
  var key = node.getAttribute('data-i18n-placeholder');
  if (i18n[selected][key]) node.setAttribute('placeholder', i18n[selected][key]);
 });
 document.querySelectorAll('option[data-scope-count]').forEach(function(option) {
  var name = option.getAttribute('data-scope-name') || '';
  var count = option.getAttribute('data-scope-count') || '0';
  option.textContent = selected === 'ko' ? name + ' · ' + count + '개' : name + ' · ' + count + ' memories';
 });
 document.querySelectorAll('[data-memory-count]').forEach(function(node) {
  var count = node.getAttribute('data-memory-count') || '0';
  node.textContent = selected === 'ko' ? count + '개 메모리' : count + ' memories';
 });
 document.querySelectorAll('[data-result-count]').forEach(function(node) {
  var count = node.getAttribute('data-result-count') || '0';
  node.textContent = selected === 'ko' ? count + '개 결과 · 관련도순' : count + ' results · sorted by relevance';
 });
 document.querySelectorAll('[data-empty-query]').forEach(function(node) {
  var query = node.getAttribute('data-empty-query') || '';
  node.textContent = selected === 'ko' ? "'" + query + "'에 대한 결과가 없습니다" : "'" + query + "' returned no results";
 });
 document.querySelectorAll('[data-lang-button]').forEach(function(btn) {
  btn.classList.toggle('is-active', btn.getAttribute('data-lang-button') === selected);
 });
}
document.querySelectorAll('[data-lang-button]').forEach(function(btn) {
 btn.addEventListener('click', function() { setLang(btn.getAttribute('data-lang-button')); });
});
var initialLang = localStorage.getItem('locale') || (navigator.language || 'en').toLowerCase().split('-')[0];
setLang(initialLang === 'ko' ? 'ko' : 'en');
var canvas = document.getElementById('memory-field');
var ctx = canvas.getContext('2d');
var nodes = [];
var pointer = { x: -1000, y: -1000 };
function resizeField() {
 var dpr = Math.min(window.devicePixelRatio || 1, 2);
 canvas.width = Math.floor(innerWidth * dpr);
 canvas.height = Math.floor(innerHeight * dpr);
 canvas.style.width = innerWidth + 'px';
 canvas.style.height = innerHeight + 'px';
 ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
 var count = Math.max(42, Math.min(120, Math.floor(innerWidth * innerHeight / 14500)));
 nodes = Array.from({ length: count }, function(_, i) {
  return {
   x: (i * 89 % innerWidth) + Math.random() * 40,
   y: (i * 53 % innerHeight) + Math.random() * 40,
   vx: (Math.random() - .5) * .18,
   vy: (Math.random() - .5) * .18,
   r: 1.2 + Math.random() * 2.2
  };
 });
}
function drawField() {
 ctx.clearRect(0, 0, innerWidth, innerHeight);
 var dark = document.documentElement.classList.contains('dark');
 var line = dark ? 'rgba(224,214,184,.16)' : 'rgba(36,54,44,.16)';
 var dot = dark ? 'rgba(164,214,177,.55)' : 'rgba(31,101,74,.46)';
 for (var i = 0; i < nodes.length; i++) {
  var a = nodes[i];
  a.x += a.vx; a.y += a.vy;
  if (a.x < -20) a.x = innerWidth + 20;
  if (a.x > innerWidth + 20) a.x = -20;
  if (a.y < -20) a.y = innerHeight + 20;
  if (a.y > innerHeight + 20) a.y = -20;
  for (var j = i + 1; j < nodes.length; j++) {
   var b = nodes[j], dx = a.x - b.x, dy = a.y - b.y, dist = Math.sqrt(dx * dx + dy * dy);
   if (dist < 118) {
    ctx.strokeStyle = line.replace(/[\d.]+\)$/, (1 - dist / 118) * .22 + ')');
    ctx.beginPath(); ctx.moveTo(a.x, a.y); ctx.lineTo(b.x, b.y); ctx.stroke();
   }
  }
  var pdx = a.x - pointer.x, pdy = a.y - pointer.y, pd = Math.sqrt(pdx * pdx + pdy * pdy);
  if (pd < 180) {
   ctx.strokeStyle = dark ? 'rgba(242,178,90,.24)' : 'rgba(130,83,28,.22)';
   ctx.beginPath(); ctx.moveTo(a.x, a.y); ctx.lineTo(pointer.x, pointer.y); ctx.stroke();
  }
  ctx.fillStyle = dot;
  ctx.beginPath(); ctx.arc(a.x, a.y, a.r, 0, Math.PI * 2); ctx.fill();
 }
 if (!window.matchMedia('(prefers-reduced-motion: reduce)').matches) requestAnimationFrame(drawField);
}
window.addEventListener('resize', resizeField);
window.addEventListener('mousemove', function(e) { pointer.x = e.clientX; pointer.y = e.clientY; });
resizeField();
drawField();
</script>
</body>
</html>"##;

fn collection_scope_options(stats: &[CollectionStat], selected: &str) -> String {
    let all_selected = if selected == "all" { " selected" } else { "" };
    let mut options = format!(
        r#"<option value="all"{} data-i18n="scope.all">전체 컬렉션</option>"#,
        all_selected
    );
    for stat in stats {
        let selected_attr = if stat.name == selected {
            " selected"
        } else {
            ""
        };
        options.push_str(&format!(
            r#"<option value="{}"{} data-scope-name="{}" data-scope-count="{}">{} · {}개</option>"#,
            html_escape(&stat.name),
            selected_attr,
            html_escape(&stat.name),
            stat.chunk_count,
            html_escape(&stat.name),
            stat.chunk_count
        ));
    }
    options
}

pub async fn viewer_dashboard(State(system): State<Arc<RwLock<MemorySystem>>>) -> Html<String> {
    let sys = system.read().await;
    let db = sys.db.read().await;

    let total_chunks = db.chunk_count().unwrap_or(0);
    let total_facts = db.fact_count().unwrap_or(0);
    let total_notes = db.note_count().unwrap_or(0);
    let (graph_nodes, _) = db.graph_stats().unwrap_or((0, 0));

    let stats = db.collection_stats(8).unwrap_or_default();
    let recent = db.recent_chunks(6).unwrap_or_default();
    let scope_options = collection_scope_options(&stats, "all");

    let mut collections_html = String::new();
    for stat in &stats {
        collections_html.push_str(&format!(
            r#"<a href="/collection/{}?format=html" class="ledger-row text-stone-800 dark:text-stone-100 transition-colors">
             <span class="truncate font-medium">{}</span>
             <span class="text-stone-500 dark:text-stone-400">{}개</span>
             <span class="text-right text-xs text-stone-500 dark:text-stone-400" data-i18n="action.open">열기</span>
            </a>"#,
            html_escape(&stat.name),
            html_escape(&stat.name),
            stat.chunk_count
        ));
    }
    if collections_html.is_empty() {
        collections_html =
            r#"<div class="px-6 py-10 text-center text-sm text-stone-500" data-i18n="empty.collections">아직 수집된 컬렉션이 없습니다</div>"#
                .to_string();
    }

    let mut recent_html = String::new();
    for chunk in &recent {
        recent_html.push_str(&format!(
            r#"<a href="/collection/{}?format=html" class="group">
             <div class="flex items-center gap-2 text-[11px] text-stone-500 mb-2">
              <span class="eyebrow">{}</span>
              <span class="truncate">{}</span>
              <span class="ml-auto whitespace-nowrap">{}</span>
             </div>
             <div class="text-sm leading-relaxed text-stone-700 dark:text-stone-200 group-hover:text-stone-950 dark:group-hover:text-white transition-colors">{}</div>
            </a>"#,
            html_escape(&chunk.project),
            html_escape(&format!("{:?}", chunk.importance)),
            html_escape(&chunk.project),
            chunk.created_at.format("%m-%d %H:%M"),
            html_escape(&redact_text(&chunk.document)).chars().take(220).collect::<String>()
        ));
    }
    if recent_html.is_empty() {
        recent_html =
            r#"<div class="py-10 text-center text-sm text-stone-500" data-i18n="empty.recent">최근 메모리가 없습니다</div>"#
                .to_string();
    }

    let content = format!(
        r##"<div class="ops-shell">
         <header class="ops-header">
         <div>
           <div class="eyebrow mb-2" data-i18n="dashboard.eyebrow">Workspace memory</div>
           <h1 class="ops-title" data-i18n="dashboard.title">메모리 운영 콘솔</h1>
           <p class="ops-subtitle" data-i18n="dashboard.subtitle">저장된 대화와 작업 기록을 검색하고, 컬렉션별 규모와 최근 입력을 확인합니다. 컬렉션은 저장 요청의 project 값으로 묶이며 값이 없으면 default로 들어갑니다.</p>
          </div>
         </header>
         <section class="ops-search">
          <div class="ops-search-inner">
           <div>
            <div class="eyebrow mb-2" data-i18n="dashboard.searchTitle">통합 검색</div>
            <p class="text-sm leading-relaxed text-stone-600 dark:text-stone-300" data-i18n="dashboard.searchHelp">결정, 오류, 설정, 코드 단서를 한 번에 찾습니다.</p>
           </div>
           <form method="get" action="/viewer/search">
            <div class="command-input">
             <label class="search-field">
              <span class="field-label" data-i18n="field.query">검색어</span>
              <input name="q" placeholder="예: 배포 결정, OAuth 오류, PostgreSQL 설정" data-i18n-placeholder="placeholder.search" class="w-full bg-transparent px-2 py-2 text-sm outline-none placeholder:text-stone-500" required>
             </label>
             <label>
              <span class="field-label" data-i18n="field.scope">범위</span>
              <select name="project" class="scope-select">{}</select>
             </label>
             <button class="rounded-2xl bg-stone-950 text-white dark:bg-stone-100 dark:text-stone-950 px-5 py-3 text-sm font-medium" data-i18n="button.search">검색</button>
            </div>
           </form>
          </div>
         </section>
         <section class="ops-kpis">
          <div class="ops-kpi"><div class="metric-label" data-i18n="metric.memories">메모리</div><div class="text-2xl font-semibold mt-2">{}</div></div>
          <div class="ops-kpi"><div class="metric-label" data-i18n="metric.facts">지식</div><div class="text-2xl font-semibold mt-2">{}</div></div>
          <div class="ops-kpi"><div class="metric-label" data-i18n="metric.notes">노트</div><div class="text-2xl font-semibold mt-2">{}</div></div>
          <div class="ops-kpi"><div class="metric-label" data-i18n="metric.graph">그래프</div><div class="text-2xl font-semibold mt-2">{}</div></div>
         </section>
         <div class="ops-grid">
          <section class="work-panel">
           <div class="work-panel-head">
            <div>
             <div class="eyebrow mb-1" data-i18n="panel.collections">컬렉션</div>
             <div class="text-sm text-stone-500 dark:text-stone-400">project scope</div>
            </div>
            <a href="/viewer/collections" class="quiet-chip text-xs" data-i18n="action.open">열기</a>
           </div>
           <div class="ledger rounded-none border-0 bg-transparent">
            <div class="ledger-head">
             <span data-i18n="panel.collections">컬렉션</span>
             <span data-i18n="metric.memories">메모리</span>
             <span class="text-right" data-i18n="action.open">열기</span>
            </div>
            {}
           </div>
          </section>
          <aside class="work-panel">
           <div class="work-panel-head">
            <div>
             <div class="eyebrow mb-1" data-i18n="panel.recent">최근 입력</div>
             <div class="text-sm text-stone-500 dark:text-stone-400">latest captured context</div>
            </div>
            <a href="/viewer/search" class="quiet-chip text-xs" data-i18n="nav.search">검색</a>
           </div>
           <div class="activity-stream">{}</div>
           <div class="work-panel-head border-t border-black/10 dark:border-white/10">
            <div class="eyebrow" data-i18n="panel.health">시스템 상태</div>
            <a href="/stats?format=html" class="text-xs text-stone-500 hover:text-stone-950 dark:hover:text-white" data-i18n="action.viewSystem">상태와 API 보기</a>
           </div>
           <div class="health-list">
            <div class="inspector-row"><span class="metric-label">graph nodes</span><strong>{}</strong></div>
            <div class="inspector-row"><span class="metric-label">facts</span><strong>{}</strong></div>
            <div class="inspector-row"><span class="metric-label">notes</span><strong>{}</strong></div>
            <div class="inspector-row"><span class="metric-label">collections</span><strong>{}</strong></div>
           </div>
          </aside>
         </div>
        </div>"##,
        scope_options,
        total_chunks,
        total_facts,
        total_notes,
        graph_nodes,
        collections_html,
        recent_html,
        graph_nodes,
        total_facts,
        total_notes,
        stats.len()
    );

    let html = BASE_HTML
        .replace("__TITLE__", "대시보드")
        .replace("__VERSION__", env!("CARGO_PKG_VERSION"))
        .replace("__ACTIVE_DASHBOARD__", "is-active")
        .replace("__ACTIVE_COLLECTIONS__", "")
        .replace("__ACTIVE_SEARCH__", "")
        .replace("__CONTENT__", &content);

    Html(html)
}

pub async fn viewer_collections(State(system): State<Arc<RwLock<MemorySystem>>>) -> Html<String> {
    let sys = system.read().await;
    let db = sys.db.read().await;

    let stats = db.collection_stats(500).unwrap_or_default();

    // Sort: playbook first, then projects by chunk_count desc.
    let kind_rank = |k: &str| match k {
        "playbook" => 0,
        _ => 1,
    };
    let mut sorted_stats = stats.clone();
    sorted_stats.sort_by(|a, b| {
        kind_rank(&a.kind)
            .cmp(&kind_rank(&b.kind))
            .then_with(|| b.chunk_count.cmp(&a.chunk_count))
            .then_with(|| a.name.cmp(&b.name))
    });

    let mut rows = String::new();
    for stat in &sorted_stats {
        // Two kinds only:
        //   playbook  — cross-project manual notes (notebook icon, ochre bar)
        //   project   — per-cwd bucket (folder icon, olive bar)
        let (accent_class, kind_label, icon_svg) = if stat.kind == "playbook" {
            (
                "col-accent-playbook",
                "PLAYBOOK",
                r#"<path d="M5 4.75h9.25A2.75 2.75 0 0 1 17 7.5v11.75H7.5A2.5 2.5 0 0 1 5 16.75V4.75Z"/><path d="M5 4.75v12A2.5 2.5 0 0 0 7.5 19.25H17"/>"#,
            )
        } else {
            (
                "col-accent-project",
                "PROJECT",
                r#"<path d="M4.5 7.25h5.4l1.6 2h7.5a1.5 1.5 0 0 1 1.5 1.5v6.25a1.5 1.5 0 0 1-1.5 1.5h-13a1.5 1.5 0 0 1-1.5-1.5V8.75a1.5 1.5 0 0 1 1.5-1.5Z"/>"#,
            )
        };

        rows.push_str(&format!(
            r##"<a href="/collection/{name}?format=html" class="col-card {accent_class}">
             <span class="col-card-bar" aria-hidden="true"></span>
             <div class="col-card-body">
              <div class="col-card-head">
               <svg class="col-card-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round">{icon_svg}</svg>
               <span class="col-card-name">{name}</span>
               <span class="col-card-kind">{kind_label}</span>
              </div>
              <dl class="col-card-stats">
               <div><dt>Total</dt><dd class="num">{total}</dd></div>
               <div><dt>Manual</dt><dd class="num num-accent">{manual}</dd></div>
               <div><dt>Autolog</dt><dd class="num num-muted">{autolog}</dd></div>
              </dl>
             </div>
             <svg class="col-card-arrow" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="m10 7 5 5-5 5"/></svg>
            </a>"##,
            name = html_escape(&stat.name),
            accent_class = accent_class,
            kind_label = kind_label,
            icon_svg = icon_svg,
            total = stat.chunk_count,
            manual = stat.manual_count,
            autolog = stat.autolog_count,
        ));
    }
    if rows.is_empty() {
        rows = r#"<div class="col-empty">컬렉션이 없습니다</div>"#.to_string();
    }

    // Summary strip: totals + kind breakdown
    let total_collections = sorted_stats.len();
    let total_chunks: usize = sorted_stats.iter().map(|s| s.chunk_count).sum();
    let total_manual: usize = sorted_stats.iter().map(|s| s.manual_count).sum();
    let total_autolog: usize = sorted_stats.iter().map(|s| s.autolog_count).sum();
    let playbook_count = sorted_stats.iter().filter(|s| s.kind == "playbook").count();
    let project_count = total_collections.saturating_sub(playbook_count);
    let manual_pct = if total_chunks > 0 {
        (total_manual as f64 / total_chunks as f64 * 100.0).round() as usize
    } else {
        0
    };

    let summary_strip = format!(
        r##"<section class="col-summary">
         <div class="col-summary-grid">
          <div class="col-metric">
           <span class="col-metric-label">Collections</span>
           <span class="col-metric-value num">{collections}</span>
           <span class="col-metric-sub">playbook {pb} · project {pr}</span>
          </div>
          <div class="col-metric">
           <span class="col-metric-label">Manual</span>
           <span class="col-metric-value num">{manual}</span>
           <span class="col-metric-sub">직접 저장한 고신호 메모</span>
          </div>
          <div class="col-metric">
           <span class="col-metric-label">Auto</span>
           <span class="col-metric-value num num-muted">{autolog}</span>
           <span class="col-metric-sub">도구 호출이 남긴 로그</span>
          </div>
          <div class="col-metric">
           <span class="col-metric-label">Total</span>
           <span class="col-metric-value num">{total}</span>
           <span class="col-metric-sub">manual 비중 {pct}%</span>
          </div>
         </div>
         <div class="col-summary-bar" aria-hidden="true">
          <span class="col-summary-bar-manual" style="width:{pct}%"></span>
         </div>
        </section>"##,
        collections = total_collections,
        pb = playbook_count,
        pr = project_count,
        manual = total_manual,
        autolog = total_autolog,
        total = total_chunks,
        pct = manual_pct,
    );

    let content = format!(
        r##"<header class="col-header">
         <div>
          <h1 class="col-title" data-i18n="collections.title">컬렉션</h1>
          <p class="col-subtitle" data-i18n="collections.subtitle">cwd 이름으로 자동 분류된 버킷들. <em class="col-key col-key-pb">playbook</em>만 예외로, 어디서든 검색하는 cross-project 메모다.</p>
         </div>
         <a href="/viewer/search" class="col-search-link" data-i18n="nav.search">검색</a>
        </header>
        {summary}
        <div class="col-grid">
         {rows}
        </div>"##,
        summary = summary_strip,
        rows = rows,
    );

    let html = BASE_HTML
        .replace("__TITLE__", "컬렉션")
        .replace("__VERSION__", env!("CARGO_PKG_VERSION"))
        .replace("__ACTIVE_DASHBOARD__", "")
        .replace("__ACTIVE_COLLECTIONS__", "is-active")
        .replace("__ACTIVE_SEARCH__", "")
        .replace("__CONTENT__", &content);

    Html(html)
}

pub async fn viewer_search(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Query(params): Query<HashMap<String, String>>,
) -> Html<String> {
    let q = params.get("q").cloned().unwrap_or_default();
    let project = params
        .get("project")
        .cloned()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "all".to_string());
    let scope_stats = {
        let sys = system.read().await;
        let db = sys.db.read().await;
        db.collection_stats(500).unwrap_or_default()
    };
    let scope_options = collection_scope_options(&scope_stats, &project);

    let results_html = if !q.is_empty() {
        let mut items = String::new();
        let started = std::time::Instant::now();
        let results = run_hybrid_search(system.clone(), &q, &project, 20, false, true).await;

        for item in &results {
            let highlighted_document = highlight_query_html(&item.document, &q);
            items.push_str(&format!(
                r#"<article class="panel p-4 mb-3 search-result">
                 <div class="flex flex-wrap items-center gap-2 mb-2">
                  <span class="text-[10px] px-2 py-0.5 rounded-full bg-emerald-100 text-emerald-700 dark:bg-emerald-400/10 dark:text-emerald-300 font-medium">{}</span>
                  <span class="text-[10px] px-2 py-0.5 rounded-full bg-slate-100 dark:bg-white/10 text-slate-600 dark:text-slate-300">{}</span>
                  <span class="text-[10px] text-slate-400 ml-auto">{}</span>
                 </div>
                 <div class="text-xs text-slate-500 mb-2">{}</div>
                 <div class="text-sm text-slate-800 dark:text-slate-200 whitespace-pre-wrap leading-relaxed">{}</div>
                 <div class="mt-3 text-[11px] text-slate-400">score {:.4}</div>
                </article>"#,
                html_escape(&item.importance),
                html_escape(&item.chunk_type),
                html_escape(&item.timestamp),
                html_escape(&item.project),
                highlighted_document,
                item.score
            ));
        }
        if results.is_empty() {
            format!(
                r#"<div class="text-center py-12 text-slate-500" data-empty-query="{}">
                 <div class="text-sm mb-1">'{}'에 대한 결과가 없습니다</div>
                </div>"#,
                html_escape(&q),
                html_escape(&q)
            )
        } else {
            format!(
                r#"<div class="flex items-center justify-between text-xs text-slate-500 mb-3"><span data-result-count="{}">{}개 결과 · 관련도순</span><span>{}ms</span></div>{}"#,
                results.len(),
                results.len(),
                started.elapsed().as_millis(),
                items
            )
        }
    } else {
        r#"<div class="text-center py-12 text-slate-500 text-sm" data-i18n="search.empty">검색어를 입력하세요</div>"#
            .to_string()
    };

    let content = format!(
        r##"<div class="flex items-center justify-between mb-6">
         <div>
          <h1 class="text-xl font-semibold tracking-tight" data-i18n="search.title">검색</h1>
          <p class="text-sm text-slate-500 dark:text-slate-400 mt-0.5" data-i18n="search.subtitle">검색어를 입력하고 필요한 컬렉션만 좁혀 봅니다.</p>
         </div>
         <a href="/viewer/collections" class="quiet-chip text-xs" data-i18n="search.collections">컬렉션 보기</a>
        </div>
        <form method="get" action="/viewer/search" class="mb-6">
         <div class="command-input">
          <label class="search-field">
           <span class="field-label" data-i18n="field.query">검색어</span>
           <input type="text" name="q" value="{}" placeholder="예: 배포 결정, OAuth 오류, PostgreSQL 설정" data-i18n-placeholder="placeholder.search" class="w-full bg-transparent px-2 py-2 text-sm outline-none placeholder:text-stone-500" required>
          </label>
          <label>
           <span class="field-label" data-i18n="field.scope">범위</span>
           <select name="project" class="scope-select">{}</select>
          </label>
          <button type="submit" class="rounded-2xl bg-slate-950 dark:bg-white text-white dark:text-slate-950 px-5 py-3 text-sm font-medium hover:opacity-90 transition-opacity" data-i18n="button.search">검색</button>
         </div>
        </form>
        {}"##,
        html_escape(&q),
        scope_options,
        results_html
    );

    let html = BASE_HTML
        .replace("__TITLE__", "검색")
        .replace("__VERSION__", env!("CARGO_PKG_VERSION"))
        .replace("__ACTIVE_DASHBOARD__", "")
        .replace("__ACTIVE_COLLECTIONS__", "")
        .replace("__ACTIVE_SEARCH__", "is-active")
        .replace("__CONTENT__", &content);

    Html(html)
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn query_excerpt(text: &str, query: &str, max_chars: usize) -> String {
    let text_len = text.chars().count();
    if text_len <= max_chars {
        return text.to_string();
    }

    let lower_text = text.to_ascii_lowercase();
    let mut first_match = None;
    for raw in query.split_whitespace() {
        let term = raw.trim_matches(|ch: char| !ch.is_alphanumeric());
        if term.chars().count() < 2 {
            continue;
        }
        let needle = term.to_ascii_lowercase();
        if let Some(byte_start) = lower_text.find(&needle) {
            let char_start = text[..byte_start].chars().count();
            first_match =
                Some(first_match.map_or(char_start, |current: usize| current.min(char_start)));
        }
    }

    let Some(match_char) = first_match else {
        return text.chars().take(max_chars).collect();
    };

    let context_before = max_chars / 3;
    let start = match_char.saturating_sub(context_before);
    let end = (start + max_chars).min(text_len);
    let mut excerpt: String = text.chars().skip(start).take(end - start).collect();

    if start > 0 {
        excerpt.insert_str(0, "...");
    }
    if end < text_len {
        excerpt.push_str("...");
    }
    excerpt
}

fn highlight_query_html(text: &str, query: &str) -> String {
    let escaped = html_escape(text);
    let mut terms = Vec::<String>::new();

    for raw in query.split_whitespace() {
        let term = raw
            .trim_matches(|ch: char| !ch.is_alphanumeric())
            .to_string();
        let comparable = term.to_ascii_lowercase();
        if term.chars().count() < 2
            || terms
                .iter()
                .any(|existing| existing.to_ascii_lowercase() == comparable)
        {
            continue;
        }
        terms.push(term);
    }

    terms.sort_by_key(|term| std::cmp::Reverse(term.chars().count()));
    terms.truncate(12);

    if terms.is_empty() {
        return escaped;
    }

    let lower = escaped.to_ascii_lowercase();
    let mut ranges = Vec::<(usize, usize)>::new();

    for term in terms {
        let needle = html_escape(&term).to_ascii_lowercase();
        if needle.is_empty() {
            continue;
        }

        let mut offset = 0;
        while let Some(relative_start) = lower[offset..].find(&needle) {
            let start = offset + relative_start;
            let end = start + needle.len();
            ranges.push((start, end));
            offset = end;
        }
    }

    if ranges.is_empty() {
        return escaped;
    }

    ranges.sort_unstable_by_key(|(start, end)| (*start, *end));
    let mut merged = Vec::<(usize, usize)>::new();
    for (start, end) in ranges {
        if let Some((_, last_end)) = merged.last_mut() {
            if start <= *last_end {
                *last_end = (*last_end).max(end);
                continue;
            }
        }
        merged.push((start, end));
    }

    let mut output = String::with_capacity(escaped.len() + merged.len() * 32);
    let mut cursor = 0;
    for (start, end) in merged {
        output.push_str(&escaped[cursor..start]);
        output.push_str("<mark>");
        output.push_str(&escaped[start..end]);
        output.push_str("</mark>");
        cursor = end;
    }
    output.push_str(&escaped[cursor..]);
    output
}

fn keyword_match_ratio(document: &str, keywords: &[String]) -> f32 {
    if keywords.is_empty() {
        return 0.0;
    }
    let doc = document.to_lowercase();
    let matched = keywords
        .iter()
        .filter(|kw| doc.contains(&kw.to_lowercase()))
        .count();
    matched as f32 / keywords.len() as f32
}

fn keyword_bonus_from_ratio(ratio: f32, max_bonus: f32) -> f32 {
    max_bonus * ratio
}

fn importance_bonus(importance: &Importance) -> f32 {
    match importance {
        Importance::Knowledge => 0.10,
        Importance::Decision => 0.05,
        Importance::Preference => 0.08,
        Importance::Log => 0.0,
    }
}

fn type_bonus(chunk_type: &ChunkType) -> f32 {
    match chunk_type {
        ChunkType::Manual => 0.10,
        ChunkType::Filtered => 0.0,
        ChunkType::AutoLog => -0.05,
        ChunkType::Consolidated => 0.03,
    }
}

fn recency_penalty(created_at: chrono::DateTime<chrono::Utc>, rate: f32, cap: f32) -> f32 {
    let days = (chrono::Utc::now() - created_at).num_seconds().max(0) as f32 / 86_400.0;
    (days * rate).min(cap)
}

fn diversify_by_project(items: Vec<SearchResultItem>, limit: usize) -> Vec<SearchResultItem> {
    let max_per_project = (limit / 2).max(1);
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut selected = Vec::new();
    let mut deferred = Vec::new();
    for item in items {
        let count = counts.entry(item.project.clone()).or_insert(0);
        if *count < max_per_project {
            *count += 1;
            selected.push(item);
        } else {
            deferred.push(item);
        }
        if selected.len() >= limit {
            break;
        }
    }
    for item in deferred {
        if selected.len() >= limit {
            break;
        }
        selected.push(item);
    }
    selected
}

/// Maximal Marginal Relevance selection over scored candidates.
///
/// Greedily builds the result list; at each step it picks the candidate that
/// maximises `lambda * relevance - (1 - lambda) * max_sim_to_selected`, where
/// relevance is the composite score (min-max normalised across candidates) and
/// similarity is cosine over chunk embeddings. The first pick is always the
/// most-relevant candidate, so top-1 relevance is preserved; subsequent picks
/// trade a little relevance for diversity. `lambda` is expected in (0, 1);
/// callers gate on that range. Candidates must be pre-sorted by score desc.
fn mmr_select(
    mut candidates: Vec<(SearchResultItem, Vec<f32>)>,
    lambda: f32,
    n: usize,
) -> Vec<SearchResultItem> {
    let limit = n.min(candidates.len());
    if limit <= 1 {
        return candidates.into_iter().take(limit).map(|(it, _)| it).collect();
    }
    let max_rel = candidates.first().map(|(it, _)| it.score).unwrap_or(0.0);
    let min_rel = candidates.last().map(|(it, _)| it.score).unwrap_or(0.0);
    let span = max_rel - min_rel;
    let norm = |s: f32| if span.abs() <= f32::EPSILON { 1.0 } else { (s - min_rel) / span };

    let mut selected: Vec<(SearchResultItem, Vec<f32>)> = Vec::with_capacity(limit);
    selected.push(candidates.remove(0));
    while selected.len() < limit && !candidates.is_empty() {
        let mut best_idx = 0usize;
        let mut best_mmr = f32::MIN;
        for (i, (cand, emb)) in candidates.iter().enumerate() {
            // A candidate with no embedding has unknown similarity; deny it the
            // diversity bonus (treat as fully redundant) so a missing vector
            // can't masquerade as "maximally diverse" and jump genuinely
            // relevant chunks. It can still be selected on relevance alone.
            let max_sim = if emb.is_empty() {
                1.0
            } else {
                selected
                    .iter()
                    .map(|(_, sel_emb)| crate::eval::cosine(emb, sel_emb))
                    .fold(0.0_f32, f32::max)
            };
            let mmr = lambda * norm(cand.score) - (1.0 - lambda) * max_sim;
            if mmr > best_mmr {
                best_mmr = mmr;
                best_idx = i;
            }
        }
        selected.push(candidates.remove(best_idx));
    }
    selected.into_iter().map(|(it, _)| it).collect()
}

#[cfg(test)]
pub(crate) mod test_support {
    use super::*;
    use crate::config::Config;

    pub(crate) async fn build_system() -> (tempfile::TempDir, Arc<RwLock<MemorySystem>>) {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut cfg = Config::default();
        cfg.data_dir = tmp.path().to_path_buf();
        let mut sys = MemorySystem::new(cfg)
            .await
            .expect("MemorySystem::new failed (offline and no cached model?)");
        sys.secret_tools_enabled = true;
        (tmp, Arc::new(RwLock::new(sys)))
    }
}
