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
    /// Optional category filter (e.g. "failure", "insight"). The learning layer uses this.
    #[serde(default)]
    category: String,
    /// Drop reserved autolog buckets (root/default/global/_superseded) from
    /// cross-project results. Pass project="root" explicitly to read them.
    #[serde(default)]
    exclude_reserved: bool,
    /// Safe integration label used only for local observability.
    #[serde(default = "default_http_adapter")]
    adapter: String,
}

fn default_project() -> String {
    "all".to_string()
}
fn default_n() -> usize {
    3
}
fn default_http_adapter() -> String {
    "http".to_string()
}

#[derive(Serialize)]
pub struct SearchResponse {
    results: Vec<SearchResultItem>,
    total: usize,
    elapsed_ms: u128,
    recall_id: String,
}

#[derive(Serialize)]
pub struct SearchResultItem {
    pub id: String,
    pub project: String,
    pub document: String,
    /// Full (redacted) document length in chars. When this exceeds
    /// `document.chars().count()` the excerpt was clipped — fetch the rest
    /// via GET /chunk/{id} (memory_get).
    pub doc_len: usize,
    pub score: f32,
    pub timestamp: String,
    pub chunk_type: String,
    pub importance: String,
    pub category: String,
    pub memory_kind: String,
    pub confidence: Option<f32>,
    pub adapter: String,
    pub helpful_count: i64,
    pub harmful_count: i64,
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
    #[serde(default)]
    pinned: Option<bool>,
    #[serde(default)]
    adapter: Option<String>,
    #[serde(default)]
    adapter_version: Option<String>,
    #[serde(default)]
    memory_kind: Option<MemoryKind>,
    #[serde(default)]
    confidence: Option<f32>,
    #[serde(default)]
    source_ids: Option<Vec<String>>,
    #[serde(default)]
    supersedes: Option<String>,
    #[serde(default)]
    verified_at: Option<String>,
}

#[derive(Deserialize)]
pub struct FeedbackRequest {
    recall_id: String,
    outcome: String,
    #[serde(default)]
    note: Option<String>,
}

#[derive(Serialize)]
pub struct OperationsResponse {
    summary: OperationsSummary,
    recalls: Vec<RecallEvent>,
    jobs: Vec<ProcessingJob>,
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
    #[serde(default = "default_context_chars")]
    max_chars: usize,
    /// Optional category filter for the retrieved memories part.
    #[serde(default)]
    category: String,
}

fn default_context_results() -> usize {
    3
}
fn default_context_notes() -> usize {
    4
}
fn default_context_facts() -> usize {
    4
}
fn default_context_chars() -> usize {
    2000
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
    #[serde(default)]
    include_pinned: bool,
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
pub struct PruneSample {
    id: String,
    project: String,
    created_at: chrono::DateTime<chrono::Utc>,
    preview: String,
}

#[derive(Serialize)]
pub struct PruneResponse {
    matched: usize,
    deleted: usize,
    ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dry_run: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    would_delete: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sample: Option<Vec<PruneSample>>,
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
    pub query: String,
    pub project: String,
    pub notes: Vec<Note>,
    pub facts: Vec<Fact>,
    pub memories: Vec<SearchResultItem>,
    pub prompt: String,
}

#[derive(Serialize)]
pub struct LifecycleInfo {
    last_run: Option<String>,
    last_deleted: usize,
    last_error: Option<String>,
    ttl_autolog_days: Option<i64>,
    enabled: bool,
}

#[derive(Serialize)]
pub struct HealthResponse {
    status: String,
    version: String,
    dashboard_url: String,
    data_dir: String,
    embed_model: String,
    lifecycle: LifecycleInfo,
}

#[derive(Serialize)]
pub struct CollectionEntry {
    name: String,
    chunks: usize,
    text_bytes: u64,
}

#[derive(Serialize)]
pub struct AgeBuckets {
    over_30d: u64,
    over_90d: u64,
    over_180d: u64,
}

#[derive(Serialize)]
pub struct DiskStats {
    db_bytes: u64,
    text_index_bytes: u64,
    vector_bytes: u64,
}

// Thresholds for cleanup recommendations.
const THRESHOLD_ROOT_CHUNKS: usize = 50_000;
const THRESHOLD_DISK_BYTES: u64 = 2 * 1024 * 1024 * 1024; // 2 GiB

fn dir_size(path: &std::path::Path) -> u64 {
    let mut total = 0u64;
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                total += dir_size(&p);
            } else if let Ok(m) = std::fs::metadata(&p) {
                total += m.len();
            }
        }
    }
    total
}

fn build_recommendations(root_chunks: usize, total_disk: u64) -> Vec<String> {
    let mut out = Vec::new();
    if root_chunks > THRESHOLD_ROOT_CHUNKS {
        out.push(format!(
            "root project has {} chunks (threshold {}); consider pruning auto_log entries",
            root_chunks, THRESHOLD_ROOT_CHUNKS
        ));
    }
    if total_disk > THRESHOLD_DISK_BYTES {
        out.push(format!(
            "total disk usage is {} bytes (threshold 2 GiB); consider archiving old data",
            total_disk
        ));
    }
    out
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
    collections: Vec<CollectionEntry>,
    age_buckets: AgeBuckets,
    disk: DiskStats,
    recommendations: Vec<String>,
    operations: OperationsSummary,
}

// ── API Handlers ─────────────────────────────────────────────

pub async fn health(State(system): State<Arc<RwLock<MemorySystem>>>) -> Json<HealthResponse> {
    let sys = system.read().await;
    let status = sys.lifecycle_status.read().await;
    Json(HealthResponse {
        status: "ok".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        dashboard_url: format!("http://localhost:{}/", sys.config.api_port),
        data_dir: sys.config.data_dir.display().to_string(),
        embed_model: sys.config.embed_model.clone(),
        lifecycle: LifecycleInfo {
            last_run: status.last_run.map(|dt| dt.to_rfc3339()),
            last_deleted: status.last_deleted,
            last_error: status.last_error.clone(),
            ttl_autolog_days: status.ttl_autolog_days,
            enabled: sys.config.enable_lifecycle,
        },
    })
}

/// Full (redacted) document for one chunk — the escape hatch for the 600-char
/// search excerpt. Bounded at 8,000 chars like NeighborItem, and for the same
/// reason: agents read skills/lessons from this field, so a tighter clip would
/// silently drop content.
pub async fn get_chunk_full(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Path(id): Path<String>,
) -> Response {
    let sys = system.read().await;
    let db = sys.db.read().await;
    match db.get_chunk(&id) {
        Ok(Some(c)) => {
            let redacted = redact_text(&c.document);
            let doc_len = redacted.chars().count();
            Json(serde_json::json!({
                "id": c.id,
                "project": c.project,
                "document": redacted.chars().take(8000).collect::<String>(),
                "doc_len": doc_len,
                "timestamp": c.created_at.to_rfc3339(),
                "chunk_type": format!("{:?}", c.metadata.chunk_type),
                "importance": format!("{:?}", c.metadata.importance),
                "category": format!("{:?}", c.metadata.category),
            }))
            .into_response()
        }
        _ => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": format!("chunk not found: {id}")})),
        )
            .into_response(),
    }
}

pub async fn search(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Json(req): Json<SearchRequest>,
) -> Json<SearchResponse> {
    let started = std::time::Instant::now();
    let cat = if req.category.trim().is_empty() {
        None
    } else {
        Some(req.category.clone())
    };
    let items = run_hybrid_search(
        system.clone(),
        &req.query,
        &req.project,
        req.n_results,
        req.recent_first,
        false,
        req.exclude_reserved,
        cat,
    )
    .await;
    let elapsed_ms = started.elapsed().as_millis();
    let recall_id = format!("recall_{}", uuid::Uuid::new_v4().simple());
    let event = RecallEvent {
        id: recall_id.clone(),
        query: redact_text(&req.query),
        project: req.project.clone(),
        result_ids: items.iter().map(|item| item.id.clone()).collect(),
        duration_ms: elapsed_ms.min(i64::MAX as u128) as i64,
        adapter: req.adapter,
        outcome: "pending".to_string(),
        created_at: chrono::Utc::now(),
    };
    {
        let sys = system.read().await;
        let _ = sys.db.write().await.insert_recall_event(&event);
    }
    let total = items.len();
    Json(SearchResponse {
        results: items,
        total,
        elapsed_ms,
        recall_id,
    })
}

pub(crate) async fn run_hybrid_search(
    system: Arc<RwLock<MemorySystem>>,
    query: &str,
    project: &str,
    n: usize,
    recent_first: bool,
    require_visible_match: bool,
    exclude_reserved: bool,
    category: Option<String>,
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
    let cat_filter = category
        .as_ref()
        .map(|c| c.trim().to_lowercase())
        .filter(|c| !c.is_empty());
    for (id, score) in fused {
        if let Ok(Some(c)) = db.get_chunk(&id) {
            if project != "all" && c.project != project {
                continue;
            }
            if exclude_reserved
                && matches!(
                    c.project.as_str(),
                    "root" | "default" | "global" | "_superseded" | "_trash"
                )
            {
                continue;
            }
            if let Some(cf) = &cat_filter {
                // Compare against the serde (snake_case) name so multi-word
                // categories match what the learning layer sends, e.g.
                // ToolQuirk -> "tool_quirk" (NOT the CamelCase Debug form).
                let actual = serde_json::to_value(&c.metadata.category)
                    .ok()
                    .and_then(|v| v.as_str().map(str::to_string))
                    .unwrap_or_default();
                if actual != *cf {
                    continue;
                }
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
                + feedback_bonus(c.metadata.helpful_count, c.metadata.harmful_count)
                - recency_penalty(
                    c.created_at,
                    sys.config.recency_penalty_rate,
                    sys.config.recency_penalty_cap,
                );
            let embedding = c.embedding.clone().unwrap_or_default();
            let redacted = redact_text(&c.document);
            let doc_len = redacted.chars().count();
            items.push((
                SearchResultItem {
                    id: c.id,
                    project: c.project,
                    document: if require_visible_match {
                        query_excerpt(&redacted, query, 600)
                    } else {
                        redacted.chars().take(600).collect()
                    },
                    doc_len,
                    score: final_score,
                    timestamp: c.created_at.to_rfc3339(),
                    chunk_type: format!("{:?}", c.metadata.chunk_type),
                    importance: format!("{:?}", c.metadata.importance),
                    category: format!("{:?}", c.metadata.category),
                    memory_kind: serde_json::to_value(&c.metadata.memory_kind)
                        .ok()
                        .and_then(|value| value.as_str().map(str::to_string))
                        .unwrap_or_else(|| "record".to_string()),
                    confidence: c.metadata.confidence,
                    adapter: c.metadata.adapter.clone().unwrap_or_default(),
                    helpful_count: c.metadata.helpful_count,
                    harmful_count: c.metadata.harmful_count,
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

#[derive(Deserialize)]
pub struct NeighborsRequest {
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub id: String,
    #[serde(default = "default_neighbors_k")]
    pub k: usize,
    /// Cosine-distance cap; 0 means no cap. Lower = stricter near-duplicate.
    #[serde(default)]
    pub max_distance: f32,
    #[serde(default = "default_project")]
    pub project: String,
}
fn default_neighbors_k() -> usize {
    10
}

#[derive(Debug, Serialize)]
pub struct NeighborItem {
    pub id: String,
    pub project: String,
    pub document: String,
    pub distance: f32,
    pub category: String,
    pub importance: String,
    pub chunk_type: String,
}

/// Cosine nearest-neighbours of a chunk (by `id`) or of free `text`, straight
/// from the HNSW index. This is the robust primitive for the learning layer's
/// consolidation: client-side lexical similarity (trigrams) misses paraphrase
/// duplicates that the engine's embeddings catch. Self is excluded.
pub async fn neighbors(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Json(req): Json<NeighborsRequest>,
) -> Json<Vec<NeighborItem>> {
    let sys = system.read().await;
    let query_embedding: Option<Vec<f32>> = if !req.id.trim().is_empty() {
        let db = sys.db.read().await;
        db.get_chunk(&req.id)
            .ok()
            .flatten()
            .and_then(|c| c.embedding)
    } else if !req.text.trim().is_empty() {
        let embedder = sys.embedder.clone();
        let text = req.text.clone();
        tokio::task::spawn_blocking(move || embedder.encode_query(&text))
            .await
            .ok()
            .and_then(|r| r.ok())
    } else {
        None
    };
    let Some(embedding) = query_embedding else {
        return Json(Vec::new());
    };
    let k = req.k.clamp(1, 100);
    let raw = sys
        .vector_index
        .read()
        .await
        .search(&embedding, k + 1)
        .unwrap_or_default();
    let db = sys.db.read().await;
    let mut out = Vec::new();
    for (id, distance) in raw {
        if id == req.id {
            continue; // exclude self when querying by id
        }
        if req.max_distance > 0.0 && distance > req.max_distance {
            continue;
        }
        if let Ok(Some(c)) = db.get_chunk(&id) {
            if req.project != "all" && c.project != req.project {
                continue;
            }
            out.push(NeighborItem {
                id: c.id,
                project: c.project,
                // Generous limit: the learning layer rewrites/refines memories
                // and skills from this field, so truncating here would silently
                // drop content on write-back. 8000 chars covers single-sentence
                // memories and multi-step skills with headroom.
                document: redact_text(&c.document).chars().take(8000).collect(),
                distance,
                category: format!("{:?}", c.metadata.category),
                importance: format!("{:?}", c.metadata.importance),
                chunk_type: format!("{:?}", c.metadata.chunk_type),
            });
            if out.len() >= k {
                break;
            }
        }
    }
    Json(out)
}

pub async fn add(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Json(req): Json<AddRequest>,
) -> Json<HashMap<String, String>> {
    let project = if req.project.is_empty() {
        "default".to_string()
    } else {
        req.project
    };
    if matches!(project.as_str(), "_trash" | "_superseded") {
        let mut map = HashMap::new();
        map.insert("status".to_string(), "error".to_string());
        map.insert(
            "error".to_string(),
            format!("project '{project}' is reserved; write rejected"),
        );
        return Json(map);
    }
    let text = redact_text(&req.text);
    let mut metadata = req.metadata.unwrap_or(Metadata {
        chunk_type: ChunkType::Manual,
        ..Default::default()
    });
    let adapter = metadata
        .adapter
        .clone()
        .unwrap_or_else(|| "http".to_string());
    metadata.adapter.get_or_insert_with(|| adapter.clone());

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

    let id = format!("manual_{}", uuid::Uuid::new_v4().simple());
    let job_id = format!("job_{}", uuid::Uuid::new_v4().simple());
    let now = chrono::Utc::now();
    let mut job = ProcessingJob {
        id: job_id.clone(),
        operation: "embed_and_store".to_string(),
        target_id: id.clone(),
        state: "queued".to_string(),
        canonical_id: None,
        adapter: adapter.clone(),
        error: None,
        created_at: now,
        updated_at: now,
    };
    {
        let sys = system.read().await;
        let _ = sys.db.write().await.upsert_processing_job(&job);
    }

    job.state = "running".to_string();
    job.updated_at = chrono::Utc::now();
    {
        let sys = system.read().await;
        let _ = sys.db.write().await.upsert_processing_job(&job);
    }
    match persist_chunk_async(system.clone(), id.clone(), project.clone(), text, metadata).await {
        Ok(canonical_id) => {
            job.state = if canonical_id.is_some() {
                "deduplicated".to_string()
            } else {
                "succeeded".to_string()
            };
            job.canonical_id = canonical_id;
            job.error = None;
        }
        Err(error) => {
            job.state = "failed".to_string();
            job.error = Some(error.to_string());
            tracing::warn!("api add failed: {error:#}");
        }
    }
    job.updated_at = chrono::Utc::now();
    {
        let sys = system.read().await;
        let _ = sys.db.write().await.upsert_processing_job(&job);
    }

    let mut map = HashMap::new();
    map.insert("status".to_string(), job.state.clone());
    map.insert("id".to_string(), id);
    map.insert("job_id".to_string(), job_id);
    map.insert("project".to_string(), project);
    map.insert("adapter".to_string(), adapter);
    Json(map)
}

pub(crate) async fn persist_chunk_async(
    system: Arc<RwLock<MemorySystem>>,
    id: String,
    project: String,
    text: String,
    metadata: Metadata,
) -> anyhow::Result<Option<String>> {
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
        for (existing_id, distance) in &neighbors {
            if *distance >= 0.05 {
                break; // neighbours are distance-ascending
            }
            // Verify same project before suppressing — embeddings are
            // global but chunks are project-scoped, so a cross-project
            // twin at rank 1 must not shadow a same-project dup at rank 2+.
            let db = sys.db.read().await;
            if let Ok(Some(existing)) = db.get_chunk(existing_id)
                && existing.project == project
            {
                drop(db);
                let db = sys.db.write().await;
                db.touch_chunk(existing_id)?;
                db.insert_memory_alias(&id, existing_id)?;
                tracing::debug!(
                    "semantic dedup aliased {} to {} (distance={:.4})",
                    id,
                    existing_id,
                    distance
                );
                return Ok(Some(existing_id.clone()));
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
    Ok(None)
}

pub async fn delete(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Json(req): Json<DeleteRequest>,
) -> Json<HashMap<String, serde_json::Value>> {
    let sys = system.read().await;
    let now_str = chrono::Utc::now().to_rfc3339();
    let db = sys.db.write().await;

    let mut deleted = Vec::new();
    let mut not_found = Vec::new();

    for id in req.ids {
        let canonical_id = db.canonical_chunk_id(&id).unwrap_or_else(|_| id.clone());
        match db.trash_chunk(&canonical_id, &now_str) {
            Ok(true) => deleted.push(canonical_id),
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

#[derive(Deserialize)]
pub struct RestoreRequest {
    pub ids: Vec<String>,
}

pub async fn restore(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Json(req): Json<RestoreRequest>,
) -> Json<HashMap<String, serde_json::Value>> {
    let sys = system.read().await;
    let mut restored = Vec::new();
    let mut missing = Vec::new();

    for id in req.ids {
        let chunk = {
            let db = sys.db.write().await;
            db.restore_chunk(&id).unwrap_or(None)
        };
        match chunk {
            Some(chunk) => {
                let embedding = if let Some(emb) = chunk.embedding.clone() {
                    emb
                } else {
                    let embedder = sys.embedder.clone();
                    let text = chunk.document.clone();
                    match tokio::task::spawn_blocking(move || embedder.encode_document(&text)).await
                    {
                        Ok(Ok(emb)) => emb,
                        _ => {
                            missing.push(id);
                            continue;
                        }
                    }
                };
                let _ = sys
                    .add_text_doc(&chunk.id, &chunk.project, &chunk.document)
                    .await;
                {
                    let mut vector_index = sys.vector_index.write().await;
                    let _ = vector_index.add(&chunk.id, &embedding);
                    let _ = vector_index.save();
                }
                restored.push(id);
            }
            None => missing.push(id),
        }
    }

    let mut result = HashMap::new();
    result.insert("restored".to_string(), serde_json::json!(restored));
    result.insert("missing".to_string(), serde_json::json!(missing));
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
    if embedding_changed && let Some(embedding) = &chunk.embedding {
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
    if let Some(value) = patch.adapter {
        target.adapter = Some(value);
    }
    if let Some(value) = patch.adapter_version {
        target.adapter_version = Some(value);
    }
    if let Some(value) = patch.memory_kind {
        target.memory_kind = value;
    }
    if let Some(value) = patch.confidence {
        target.confidence = Some(value.clamp(0.0, 1.0));
    }
    if let Some(value) = patch.source_ids {
        target.source_ids = value;
    }
    if let Some(value) = patch.supersedes {
        target.supersedes = Some(value);
    }
    if let Some(value) = patch.verified_at {
        target.verified_at = Some(value);
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
    if let Some(value) = patch.pinned {
        target.pinned = value;
    }
}

pub async fn context_pack(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Json(req): Json<ContextRequest>,
) -> Json<ContextResponse> {
    let cat = if req.category.trim().is_empty() {
        None
    } else {
        Some(req.category.clone())
    };
    Json(
        build_context(
            system,
            &req.query,
            &req.project,
            req.n_results,
            req.max_notes,
            req.max_facts,
            req.max_chars,
            cat,
        )
        .await,
    )
}

/// Shared context-pack builder used by both the HTTP `/context` endpoint and
/// the MCP `memory_context` tool, so both return an identical prompt. Assembles
/// core notes + query-matching facts + retrieved memories and renders a
/// budget-bounded prompt string.
pub(crate) async fn build_context(
    system: Arc<RwLock<MemorySystem>>,
    query: &str,
    project: &str,
    n_results: usize,
    max_notes: usize,
    max_facts: usize,
    max_chars: usize,
    category: Option<String>,
) -> ContextResponse {
    let query = query.trim().to_string();
    let project = if project.trim().is_empty() {
        "all".to_string()
    } else {
        project.trim().to_string()
    };
    let memories = if query.is_empty() {
        Vec::new()
    } else {
        // Injection/context recall must use the SAME hybrid (semantic + lexical)
        // path as /search. The old `require_visible_match=true` disabled vector
        // search AND hard-required literal keyword overlap, so a Korean query
        // ("바이오스") never matched an English-stored memory ("BIOS") and the
        // pack came back empty. Use false so cross-language / paraphrased recall
        // actually surfaces; relevance stays bounded by distance_cutoff +
        // low_relevance_fallback + n_results + max_chars.
        run_hybrid_search(
            system.clone(),
            &query,
            &project,
            n_results.clamp(1, 20),
            false,
            false,
            project == "all",
            category,
        )
        .await
    };

    let sys = system.read().await;
    let db = sys.db.read().await;
    let mut notes = db.get_notes().unwrap_or_default();
    notes.sort_by(|a, b| b.updated.cmp(&a.updated));
    notes.truncate(max_notes.clamp(0, 50));

    let query_lower = query.to_lowercase();
    let mut facts = db.get_facts(1000).unwrap_or_default();
    if !query_lower.is_empty() {
        facts.retain(|fact| {
            format!("{} {} {}", fact.subject, fact.predicate, fact.object)
                .to_lowercase()
                .contains(&query_lower)
        });
    }
    facts.truncate(max_facts.clamp(0, 50));
    drop(db);
    drop(sys);

    let prompt = render_context_prompt(&notes, &facts, &memories, max_chars);
    ContextResponse {
        query,
        project,
        notes,
        facts,
        memories,
        prompt,
    }
}

/// Render the context prompt, never exceeding `max_chars`. Sections are added
/// in priority order (notes → facts → memories); once the budget is hit the
/// remainder is dropped and a truncation marker is appended. This keeps the
/// prompt-pack — whose whole purpose is to economise the model's context — from
/// itself blowing the window.
fn render_context_prompt(
    notes: &[Note],
    facts: &[Fact],
    memories: &[SearchResultItem],
    max_chars: usize,
) -> String {
    const OPEN: &str = "<memnest_context>";
    const CLOSE: &str = "</memnest_context>";
    const TRUNC_MARK: &str = "(context truncated to fit budget)";
    let reserved = OPEN.chars().count() + CLOSE.chars().count() + TRUNC_MARK.chars().count() + 4;
    let budget = max_chars.max(reserved);

    let mut body: Vec<String> = Vec::new();
    let mut used = reserved;
    let mut truncated = false;
    let add = |line: String, body: &mut Vec<String>, used: &mut usize| -> bool {
        let cost = line.chars().count() + 1;
        if *used + cost > budget {
            false
        } else {
            *used += cost;
            body.push(line);
            true
        }
    };

    'fill: {
        // Memories are the query-dependent section; render them first so a
        // pile of static notes/facts can never evict them under a tight budget.
        if !memories.is_empty() {
            if !add("retrieved_memories:".to_string(), &mut body, &mut used) {
                truncated = true;
                break 'fill;
            }
            for item in memories {
                if !add(
                    format!(
                        "- [{}:{} score={:.3}] {}",
                        item.project,
                        item.id,
                        item.score,
                        item.document.replace('\n', " ")
                    ),
                    &mut body,
                    &mut used,
                ) {
                    truncated = true;
                    break 'fill;
                }
            }
        }
        if !notes.is_empty() {
            if !add("core_notes:".to_string(), &mut body, &mut used) {
                truncated = true;
                break 'fill;
            }
            for note in notes {
                if !add(
                    format!("- {}: {}", note.key, redact_text(&note.value)),
                    &mut body,
                    &mut used,
                ) {
                    truncated = true;
                    break 'fill;
                }
            }
        }
        if !facts.is_empty() {
            if !add("facts:".to_string(), &mut body, &mut used) {
                truncated = true;
                break 'fill;
            }
            for fact in facts {
                if !add(
                    format!("- {} {} {}", fact.subject, fact.predicate, fact.object),
                    &mut body,
                    &mut used,
                ) {
                    truncated = true;
                    break 'fill;
                }
            }
        }
    }

    if body.is_empty() {
        body.push("(no relevant context)".to_string());
    }
    let mut out = String::with_capacity(used);
    out.push_str(OPEN);
    out.push('\n');
    out.push_str(&body.join("\n"));
    if truncated {
        out.push('\n');
        out.push_str(TRUNC_MARK);
    }
    out.push('\n');
    out.push_str(CLOSE);
    out
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
            dry_run: None,
            would_delete: None,
            sample: None,
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
    let mut victims = Vec::new();
    for chunk in chunks {
        if let Some(chunk_type) = &req.chunk_type
            && &chunk.metadata.chunk_type != chunk_type
        {
            continue;
        }
        if let Some(importance) = &req.importance
            && &chunk.metadata.importance != importance
        {
            continue;
        }
        if chunk.metadata.pinned && !req.include_pinned {
            continue;
        }

        matching_seen += 1;
        if keep_latest > 0 && matching_seen <= keep_latest {
            continue;
        }
        if let Some(cutoff) = cutoff
            && chunk.created_at > cutoff
        {
            continue;
        }
        victims.push(chunk);
    }

    let matched = victims.len();
    let ids: Vec<String> = victims.iter().map(|c| c.id.clone()).collect();

    if req.dry_run {
        let sample: Vec<PruneSample> = victims
            .iter()
            .take(20)
            .map(|c| PruneSample {
                id: c.id.clone(),
                project: c.project.clone(),
                created_at: c.created_at,
                preview: c.document.chars().take(80).collect(),
            })
            .collect();
        return Json(PruneResponse {
            matched,
            deleted: 0,
            ids: Vec::new(),
            dry_run: Some(true),
            would_delete: Some(matched),
            sample: Some(sample),
        });
    }

    let now_str = chrono::Utc::now().to_rfc3339();
    let mut deleted = 0usize;
    for id in &ids {
        if db.trash_chunk(id, &now_str).unwrap_or(false) {
            deleted += 1;
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

    crate::lifecycle::append_audit_log(
        &sys.config.data_dir,
        "api",
        deleted,
        serde_json::json!({
            "project": project,
            "chunk_type": req.chunk_type,
            "importance": req.importance,
            "keep_latest": req.keep_latest,
            "older_than_days": req.older_than_days,
        }),
    );

    Json(PruneResponse {
        matched,
        deleted,
        ids,
        dry_run: None,
        would_delete: None,
        sample: None,
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
    let last = trimmed.rsplit(['/', '\\']).next().unwrap_or("").trim();
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
            &uuid::Uuid::new_v4().to_string().replace('-', "")[..16]
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
    let mut stats = db.collection_stats(500).unwrap_or_default();
    stats.retain(|s| s.name != "_trash");
    Json(stats)
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
    if let Some(k) = req.kind.as_deref()
        && !matches!(k, "playbook" | "project" | "autolog" | "archive")
    {
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
                ChunkType::Manual => "manual",
                ChunkType::AutoLog => "autolog",
                ChunkType::Filtered => "filtered",
                ChunkType::Consolidated => "consolidated",
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
                r#"<div class="text-center py-12 text-gray-500 text-sm" data-i18n="empty.recent">No memories</div>"#
                    .to_string();
        }

        let content = format!(
            r##"<div class="flex items-center justify-between mb-6">
             <div>
              <h1 class="text-xl font-semibold tracking-tight">{}</h1>
              <p class="text-sm text-gray-500 dark:text-gray-400 mt-0.5" data-memory-count="{}">{} memories</p>
             </div>
             <a href="/viewer/collections" class="quiet-chip text-xs" data-i18n="nav.collections">Collections</a>
            </div>
            {}"##,
            name,
            chunks.len(),
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
        .map(|c| {
            let redacted = redact_text(&c.document);
            let doc_len = redacted.chars().count();
            SearchResultItem {
                id: c.id,
                project: c.project,
                document: redacted.chars().take(600).collect(),
                doc_len,
                score: 0.0,
                timestamp: c.created_at.to_rfc3339(),
                chunk_type: format!("{:?}", c.metadata.chunk_type),
                importance: format!("{:?}", c.metadata.importance),
                category: format!("{:?}", c.metadata.category),
                memory_kind: serde_json::to_value(&c.metadata.memory_kind)
                    .ok()
                    .and_then(|value| value.as_str().map(str::to_string))
                    .unwrap_or_else(|| "record".to_string()),
                confidence: c.metadata.confidence,
                adapter: c.metadata.adapter.unwrap_or_default(),
                helpful_count: c.metadata.helpful_count,
                harmful_count: c.metadata.harmful_count,
            }
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

pub async fn operations(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<OperationsResponse> {
    let limit = params
        .get("limit")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20)
        .clamp(1, 100);
    let sys = system.read().await;
    let db = sys.db.read().await;
    Json(OperationsResponse {
        summary: db.operations_summary().unwrap_or(OperationsSummary {
            recalls_24h: 0,
            helpful_24h: 0,
            harmful_24h: 0,
            queued_jobs: 0,
            running_jobs: 0,
            failed_jobs: 0,
            average_recall_ms_24h: 0.0,
        }),
        recalls: db.recent_recall_events(limit).unwrap_or_default(),
        jobs: db.recent_processing_jobs(limit).unwrap_or_default(),
    })
}

pub async fn recall_feedback(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Json(req): Json<FeedbackRequest>,
) -> Json<serde_json::Value> {
    let outcome = req.outcome.trim().to_lowercase();
    if !matches!(outcome.as_str(), "helpful" | "harmful" | "ignored") {
        return Json(serde_json::json!({
            "status": "error",
            "message": "outcome must be helpful, harmful, or ignored"
        }));
    }
    let sys = system.read().await;
    let db = sys.db.write().await;
    let note = req.note.as_deref().map(redact_text);
    match db.set_recall_feedback(&req.recall_id, &outcome, note.as_deref()) {
        Ok(None) => Json(serde_json::json!({
            "status": "not_found",
            "recall_id": req.recall_id
        })),
        Ok(Some(ids)) => Json(serde_json::json!({
            "status": "ok",
            "recall_id": req.recall_id,
            "outcome": outcome,
            "memory_ids": ids
        })),
        Err(error) => Json(serde_json::json!({
            "status": "error",
            "message": error.to_string()
        })),
    }
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

    // T2: health report fields
    let coll_stats = db.collection_stats(500).unwrap_or_default();
    let collections: Vec<CollectionEntry> = coll_stats
        .iter()
        .map(|c| CollectionEntry {
            name: c.name.clone(),
            chunks: c.chunk_count,
            text_bytes: c.text_bytes,
        })
        .collect();

    let age_buckets = {
        let now = chrono::Utc::now();
        let cut30 = (now - chrono::Duration::days(30)).to_rfc3339();
        let cut90 = (now - chrono::Duration::days(90)).to_rfc3339();
        let cut180 = (now - chrono::Duration::days(180)).to_rfc3339();
        let (o30, o90, o180) = db
            .age_buckets_root(&cut30, &cut90, &cut180)
            .unwrap_or_default();
        AgeBuckets {
            over_30d: o30,
            over_90d: o90,
            over_180d: o180,
        }
    };

    let data_dir = &sys.config.data_dir;
    let db_bytes = std::fs::metadata(data_dir.join("memory.db"))
        .map(|m| m.len())
        .unwrap_or(0);
    let text_index_bytes = dir_size(&data_dir.join("text_index"));
    let vector_bytes = dir_size(&data_dir.join("vectors"));
    let disk = DiskStats {
        db_bytes,
        text_index_bytes,
        vector_bytes,
    };

    let root_chunks = coll_stats
        .iter()
        .find(|c| c.name == "root")
        .map(|c| c.chunk_count)
        .unwrap_or(0);
    let recommendations =
        build_recommendations(root_chunks, db_bytes + text_index_bytes + vector_bytes);
    let operations = db.operations_summary().unwrap_or(OperationsSummary {
        recalls_24h: 0,
        helpful_24h: 0,
        harmful_24h: 0,
        queued_jobs: 0,
        running_jobs: 0,
        failed_jobs: 0,
        average_recall_ms_24h: 0.0,
    });

    if params.get("format") == Some(&"html".to_string()) {
        let endpoint_rows = r##"
          <div class="endpoint-row">
           <span class="metric-label">GET</span><code class="text-sm">/health</code><span class="text-xs text-stone-500">service health</span>
          </div>
          <div class="endpoint-row">
           <span class="metric-label">GET</span><code class="text-sm">/collections</code><span class="text-xs text-stone-500">collection list</span>
          </div>
          <div class="endpoint-row">
           <span class="metric-label">GET</span><code class="text-sm">/facts?n=100</code><span class="text-xs text-stone-500">stored facts</span>
          </div>
          <div class="endpoint-row">
           <span class="metric-label">POST</span><code class="text-sm">/search</code><span class="text-xs text-stone-500">memory search</span>
          </div>
        "##;
        let content = format!(
            r##"<section class="panel hero-panel p-6 md:p-8 mb-5">
             <div class="eyebrow mb-3">System</div>
             <h1 class="text-3xl md:text-5xl font-semibold tracking-tight max-w-3xl leading-tight" data-i18n="system.title">Status and integration paths.</h1>
             <p class="text-sm md:text-base text-slate-600 dark:text-slate-300 mt-4 max-w-2xl leading-relaxed" data-i18n="system.subtitle">Review current storage volume and the endpoints needed for integration. Raw responses are kept under a debug link.</p>
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
                <h2 class="text-xl font-semibold tracking-tight" data-i18n="system.endpoints">Endpoint catalog</h2>
               </div>
               <a href="/stats" class="quiet-chip text-xs" data-i18n="system.debug">Debug JSON</a>
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
               <p class="text-xs leading-relaxed text-slate-500 dark:text-slate-400" data-i18n="system.scopeHint">Collection scope comes from the request's project value. Send project=all to search everything.</p>
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
        collections,
        age_buckets,
        disk,
        recommendations,
        operations,
    })
    .into_response()
}

// ── Viewer HTML ──────────────────────────────────────────────

const BASE_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<link rel="icon" href="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'%3E%3Crect width='32' height='32' rx='5' fill='%230d5f51'/%3E%3Cpath d='M8 22V9h3l5 7 5-7h3v13h-3v-8l-5 6-5-6v8z' fill='white'/%3E%3C/svg%3E">
<title>__TITLE__ | Memnest</title>
<style>
* { box-sizing: border-box; }
body { margin: 0; font-family: "Segoe UI Variable", "Noto Sans KR", ui-sans-serif, system-ui, sans-serif; letter-spacing: 0; }
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
  background: #f5f7f8;
  background-image: linear-gradient(rgba(21, 35, 41, .035) 1px, transparent 1px);
  background-size: 100% 32px;
  color: #172126;
}
.dark .gradient-bg {
  background: #101517;
  background-image: linear-gradient(rgba(235, 244, 241, .035) 1px, transparent 1px);
  background-size: 100% 32px;
  color: #e8efec;
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
  width: 100%;
  height: 58px;
  display: none;
  align-items: center;
  justify-content: space-between;
  margin: 0;
  padding: 0 max(20px, calc((100% - 1120px) / 2));
  background: rgba(248,250,250,.96);
  border-bottom: 1px solid rgba(23,33,38,.16);
  box-shadow: none;
}
@media (min-width: 768px) {
  .product-topbar { display: flex; }
}
.dark .product-topbar {
  background: rgba(16,21,23,.96);
  border-color: rgba(232,239,236,.14);
  box-shadow: none;
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
  gap: 2px;
  padding: 0;
  background: transparent;
  border: 0;
}
.dark .topnav { background: transparent; border: 0; }
.top-link {
  height: 38px;
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 0 14px;
  border-radius: 3px;
  font-size: 13px;
  color: rgb(71 75 62);
  transition: background .18s ease, color .18s ease, transform .18s ease;
}
.top-link:hover {
  background: rgba(255,255,255,.46);
  transform: translateY(-1px);
}
.top-link.is-active {
  background: rgba(13,124,102,.1) !important;
  color: rgb(10 91 76) !important;
  box-shadow: inset 0 -2px 0 rgb(13 124 102);
  transform: none;
}
.top-link.is-active:hover {
  background: rgba(13,124,102,.1) !important;
  color: rgb(10 91 76) !important;
  transform: none;
}
.dark .top-link {
  color: rgb(214 206 181);
}
.dark .top-link:hover {
  background: rgba(255,255,255,.08);
}
.dark .top-link.is-active {
  background: rgba(111,208,189,.1) !important;
  color: rgb(111 208 189) !important;
  box-shadow: inset 0 -2px 0 rgb(111 208 189);
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

/* Operations console: dense, factual, and free of decorative AI imagery. */
.console { display: grid; gap: 18px; }
.console-header {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 24px;
  align-items: end;
  padding-bottom: 18px;
  border-bottom: 1px solid rgba(23,33,38,.18);
}
.dark .console-header { border-color: rgba(232,239,236,.16); }
.console-title { margin: 0; font-size: 28px; line-height: 1.15; letter-spacing: -.025em; }
.console-copy { margin: 6px 0 0; max-width: 64ch; color: #637077; font-size: 13px; line-height: 1.5; }
.dark .console-copy { color: #9eaaa6; }
.console-address { text-align: right; font-family: ui-monospace, "Cascadia Code", monospace; font-size: 12px; color: #637077; }
.console-address strong { display: block; margin-bottom: 4px; color: #0d7c66; font-family: inherit; font-weight: 600; }
.console-search {
  display: grid;
  grid-template-columns: minmax(220px, 1fr) minmax(160px, 240px) auto;
  gap: 8px;
  padding: 10px;
  border: 1px solid rgba(23,33,38,.18);
  background: rgba(255,255,255,.72);
}
.dark .console-search { background: rgba(22,29,31,.88); border-color: rgba(232,239,236,.16); }
.console-search input, .console-search select {
  min-height: 42px;
  border: 1px solid rgba(23,33,38,.14);
  border-radius: 4px;
  background: transparent;
  color: inherit;
  padding: 0 12px;
  outline: none;
}
.console-search input:focus, .console-search select:focus { border-color: #0d7c66; box-shadow: 0 0 0 2px rgba(13,124,102,.14); }
.console-search button { min-height: 42px; padding: 0 18px; border-radius: 4px; background: #0d5f51; color: #f3faf7; font-weight: 600; }
.console-search button:hover { background: #0a4d42; }
.console-search button:active { transform: translateY(1px); }
.console-metrics {
  display: grid;
  grid-template-columns: repeat(6, minmax(0, 1fr));
  border-top: 1px solid rgba(23,33,38,.16);
  border-bottom: 1px solid rgba(23,33,38,.16);
}
.dark .console-metrics { border-color: rgba(232,239,236,.14); }
.console-metric { padding: 14px 16px; border-left: 1px solid rgba(23,33,38,.12); }
.console-metric:first-child { border-left: 0; }
.dark .console-metric { border-color: rgba(232,239,236,.12); }
.console-metric span { display: block; color: #69767c; font-size: 11px; }
.console-metric strong { display: block; margin-top: 5px; font: 600 21px/1.2 ui-monospace, "Cascadia Code", monospace; font-variant-numeric: tabular-nums; }
.console-alerts { display: flex; flex-wrap: wrap; gap: 8px; }
.console-alert { padding: 7px 10px; border-left: 3px solid #b7791f; background: rgba(183,121,31,.09); font-size: 12px; color: #73531d; }
.console-alert.ok { border-color: #0d7c66; background: rgba(13,124,102,.08); color: #0b6655; }
.dark .console-alert { color: #e4bf7a; }
.dark .console-alert.ok { color: #6fd0bd; }
.console-grid { display: grid; grid-template-columns: minmax(0, 1.15fr) minmax(320px, .85fr); gap: 18px; }
.console-section { min-width: 0; border-top: 2px solid #27373d; }
.dark .console-section { border-color: #cad5d1; }
.console-section-head { display: flex; align-items: baseline; justify-content: space-between; gap: 12px; padding: 12px 0 9px; border-bottom: 1px solid rgba(23,33,38,.14); }
.dark .console-section-head { border-color: rgba(232,239,236,.14); }
.console-section-head h2 { margin: 0; font-size: 14px; }
.console-section-head a { color: #0d7c66; font-size: 12px; }
.console-table { width: 100%; border-collapse: collapse; table-layout: fixed; }
.console-table th { padding: 8px 6px; color: #748087; font-size: 10px; font-weight: 500; text-align: left; border-bottom: 1px solid rgba(23,33,38,.1); }
.console-table td { padding: 10px 6px; font-size: 12px; vertical-align: top; border-bottom: 1px solid rgba(23,33,38,.08); overflow: hidden; text-overflow: ellipsis; }
.dark .console-table th, .dark .console-table td { border-color: rgba(232,239,236,.09); }
.console-table .mono { font-family: ui-monospace, "Cascadia Code", monospace; font-variant-numeric: tabular-nums; }
.console-table .query { white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }
.feedback-actions { white-space: nowrap; }
.feedback-actions button { padding: 3px 6px; border: 1px solid rgba(23,33,38,.14); border-radius: 3px; background: transparent; color: inherit; font-size: 10px; }
.feedback-actions button:hover { border-color: #0d7c66; color: #0d7c66; }
.feedback-actions button:focus-visible { outline: 2px solid #0d7c66; outline-offset: 2px; }
.state { display: inline-block; padding: 2px 6px; border-radius: 3px; background: #e4e9e7; color: #45525a; font-size: 10px; }
.state.succeeded, .state.helpful { background: rgba(13,124,102,.12); color: #0b6655; }
.state.failed, .state.harmful { background: rgba(185,55,55,.12); color: #9b2c2c; }
.state.running, .state.queued, .state.pending { background: rgba(183,121,31,.12); color: #845b19; }
.dark .state { background: rgba(255,255,255,.09); color: #d5dfdc; }
.console-empty { padding: 22px 6px; color: #748087; font-size: 12px; }
.console-foot-grid { display: grid; grid-template-columns: minmax(0, 1fr) minmax(0, 1fr); gap: 18px; }
@media (max-width: 900px) {
  .console-header, .console-grid, .console-foot-grid { grid-template-columns: 1fr; }
  .console-address { text-align: left; }
  .console-metrics { grid-template-columns: repeat(3, minmax(0, 1fr)); }
  .console-metric:nth-child(4) { border-left: 0; }
}
@media (max-width: 640px) {
  .console-search { grid-template-columns: 1fr; }
  .console-metrics { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  .console-metric:nth-child(odd) { border-left: 0; }
  .console-title { font-size: 24px; }
}
</style>
</head>
<body class="gradient-bg text-gray-900 dark:text-gray-100 min-h-screen antialiased">
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
   <span class="block text-[10px] text-stone-500 dark:text-stone-400" data-i18n="brand.subtitle">Local memory</span>
  </span>
 </a>
 <nav class="topnav">
  <a href="/" class="top-link __ACTIVE_DASHBOARD__" data-i18n="nav.dashboard">Dashboard</a>
  <a href="/viewer/collections" class="top-link __ACTIVE_COLLECTIONS__" data-i18n="nav.collections">Collections</a>
  <a href="/viewer/search" class="top-link __ACTIVE_SEARCH__" data-i18n="nav.search">Search</a>
  <a href="/stats?format=html" class="top-link">System</a>
 </nav>
 <div class="top-actions">
  <div class="locale-switch" aria-label="language">
   <button type="button" data-lang-button="ko">KR</button>
   <button type="button" data-lang-button="en">EN</button>
  </div>
  <a href="/stats?format=html" class="status-pill"><span class="status-dot"></span><span data-i18n="status.ok">Operational</span></a>
  <button onclick="toggleDark()" title="toggle theme" class="w-9 h-9 rounded-full flex items-center justify-center text-slate-600 dark:text-slate-300 hover:bg-white/40 dark:hover:bg-white/10 transition-colors"><span class="dark:hidden text-sm">◐</span><span class="hidden dark:inline text-sm text-amber-500">◑</span></button>
 </div>
</header>

<!-- Mobile Header -->
<div class="md:hidden fixed top-0 left-0 right-0 h-14 glass-strong z-40 flex items-center justify-between px-4">
 <div class="flex items-center gap-2"><svg class="w-7 h-7 text-stone-700 dark:text-stone-200" viewBox="0 0 64 64" fill="none"><path d="M9 34c8-18 29-27 45-15" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"/><path d="M20 28c3-6 11-9 17-6 7 3 8 12 2 17-6 5-17 2-19-7" stroke="currentColor" stroke-width="2.2" stroke-linecap="round"/><circle cx="47" cy="17" r="4" fill="currentColor" opacity=".55"/></svg><span class="text-sm font-semibold">Memnest</span><span class="ml-2 text-xs text-stone-500" data-i18n="status.ok">Operational</span></div>
 <button onclick="document.getElementById('mob').classList.toggle('hidden')" class="p-2 text-gray-500" aria-label="menu"><svg class="w-5 h-5" viewBox="0 0 24 24" fill="none"><path d="M5 7h14M5 12h14M5 17h14" stroke="currentColor" stroke-width="1.8" stroke-linecap="round"/></svg></button>
</div>
<div id="mob" class="hidden md:hidden fixed top-14 left-0 right-0 glass z-30 p-3 space-y-1">
 <a href="/" class="block px-3 py-2 rounded-md text-sm text-gray-600 dark:text-gray-400 hover:bg-white/5" data-i18n="nav.dashboard">Dashboard</a>
 <a href="/viewer/collections" class="block px-3 py-2 rounded-md text-sm text-gray-600 dark:text-gray-400 hover:bg-white/5" data-i18n="nav.collections">Collections</a>
 <a href="/viewer/search" class="block px-3 py-2 rounded-md text-sm text-gray-600 dark:text-gray-400 hover:bg-white/5" data-i18n="nav.search">Search</a>
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
async function sendRecallFeedback(recallId, outcome, button) {
 button.disabled = true;
 try {
  var response = await fetch('/feedback', {
   method: 'POST',
   headers: { 'content-type': 'application/json' },
   body: JSON.stringify({ recall_id: recallId, outcome: outcome })
  });
  if (!response.ok) throw new Error('request failed');
  var row = button.closest('tr');
  var state = row && row.querySelector('.state');
  if (state) { state.className = 'state ' + outcome; state.textContent = outcome; }
 } catch (_) {
  button.disabled = false;
 }
}
var i18n = {
 ko: {
  'brand.subtitle': '로컬 메모리',
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
  'result.emptySuffix': '에 대한 결과가 없습니다',
  'console.title': '메모리 운영 상태',
  'console.subtitle': '무엇이 저장되고 검색됐는지, 어떤 기억이 실제 답변에 사용됐는지, 처리 실패와 데이터 편중을 한 화면에서 확인합니다.',
  'console.searchPlaceholder': '결정, 오류, 설정, 작업 절차 검색',
  'console.totalMemories': '전체 메모리',
  'console.searches24h': '24시간 검색',
  'console.avgLatency': '평균 검색 지연',
  'console.activeJobs': '처리 중 작업',
  'console.failedJobs': '실패 작업',
  'console.storage': '저장 공간',
  'console.rootShare': 'root 집중도',
  'console.staleRoot': '30일 초과 root 기록',
  'console.graphNodes': '그래프 노드 / 사실',
  'console.verdicts': '도움됨 / 문제',
  'console.recentSearches': '최근 검색과 주입 후보',
  'console.viewJson': 'JSON 보기',
  'console.jobs': '처리 작업',
  'console.allStatus': '전체 상태',
  'console.distribution': '컬렉션 분포',
  'console.manage': '관리',
  'console.recentSaves': '최근 저장',
  'console.helpful': '도움됨',
  'console.harmful': '문제',
  'console.noRecalls': '아직 기록된 검색이 없습니다. 새 검색부터 후보와 결과를 추적합니다.',
  'console.noJobs': '처리 작업이 없습니다.',
  'console.colQuery': '검색어',
  'console.colScope': '범위',
  'console.colResults': '결과',
  'console.colLatency': '지연',
  'console.colAdapter': '어댑터',
  'console.colVerdict': '판정',
  'console.colFeedback': '피드백',
  'console.colTarget': '대상',
  'console.colState': '상태',
  'console.colAdapter2': '어댑터',
  'console.colUpdated': '업데이트',
  'console.colCollection': '컬렉션',
  'console.colKind': '종류',
  'console.colMemories': '메모리',
  'console.colText': '텍스트',
  'console.colContent': '내용',
  'console.colProject': '프로젝트',
  'console.colImportance': '중요도',
  'console.colTime': '시간',
  'console.footSessions': '세션',
  'console.footNotes': '노트',
  'console.footModel': '임베딩 모델'
 },
 en: {
  'brand.subtitle': 'Local memory',
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
  'result.emptySuffix': 'returned no results',
  'console.title': 'Memory operations',
  'console.subtitle': 'What got stored, what got searched, which memories were actually used in an answer, plus failed jobs and data skew, on one screen.',
  'console.searchPlaceholder': 'Search decisions, errors, settings, procedures',
  'console.totalMemories': 'Total memories',
  'console.searches24h': 'Searches, 24h',
  'console.avgLatency': 'Average latency',
  'console.activeJobs': 'Jobs in flight',
  'console.failedJobs': 'Failed jobs',
  'console.storage': 'Storage',
  'console.rootShare': 'Root share',
  'console.staleRoot': 'Root records older than 30 days',
  'console.graphNodes': 'Graph nodes / facts',
  'console.verdicts': 'Helpful / harmful',
  'console.recentSearches': 'Recent searches and injection candidates',
  'console.viewJson': 'View JSON',
  'console.jobs': 'Processing jobs',
  'console.allStatus': 'All status',
  'console.distribution': 'Collection distribution',
  'console.manage': 'Manage',
  'console.recentSaves': 'Recently stored',
  'console.helpful': 'Helpful',
  'console.harmful': 'Harmful',
  'console.noRecalls': 'No searches recorded yet. Candidates and results are tracked from the next search on.',
  'console.noJobs': 'No processing jobs.',
  'console.colQuery': 'Query',
  'console.colScope': 'Scope',
  'console.colResults': 'Results',
  'console.colLatency': 'Latency',
  'console.colAdapter': 'Adapter',
  'console.colVerdict': 'Verdict',
  'console.colFeedback': 'Feedback',
  'console.colTarget': 'Target',
  'console.colState': 'State',
  'console.colAdapter2': 'Adapter',
  'console.colUpdated': 'Updated',
  'console.colCollection': 'Collection',
  'console.colKind': 'Kind',
  'console.colMemories': 'Memories',
  'console.colText': 'Text',
  'console.colContent': 'Content',
  'console.colProject': 'Project',
  'console.colImportance': 'Importance',
  'console.colTime': 'Time'
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

</script>
</body>
</html>"##;

fn collection_scope_options(stats: &[CollectionStat], selected: &str) -> String {
    let all_selected = if selected == "all" { " selected" } else { "" };
    let mut options = format!(
        r#"<option value="all"{} data-i18n="scope.all">All collections</option>"#,
        all_selected
    );
    for stat in stats {
        let selected_attr = if stat.name == selected {
            " selected"
        } else {
            ""
        };
        options.push_str(&format!(
            r#"<option value="{}"{} data-scope-name="{}" data-scope-count="{}">{} · {} memories</option>"#,
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
    let total_sessions = db.summary_count().unwrap_or(0);
    let total_facts = db.fact_count().unwrap_or(0);
    let total_notes = db.note_count().unwrap_or(0);
    let (graph_nodes, _) = db.graph_stats().unwrap_or((0, 0));
    let collections = db.collection_stats(8).unwrap_or_default();
    let recent = db.recent_chunks(6).unwrap_or_default();
    let recalls = db.recent_recall_events(8).unwrap_or_default();
    let jobs = db.recent_processing_jobs(8).unwrap_or_default();
    let operations = db.operations_summary().unwrap_or(OperationsSummary {
        recalls_24h: 0,
        helpful_24h: 0,
        harmful_24h: 0,
        queued_jobs: 0,
        running_jobs: 0,
        failed_jobs: 0,
        average_recall_ms_24h: 0.0,
    });
    let now = chrono::Utc::now();
    let cut30 = (now - chrono::Duration::days(30)).to_rfc3339();
    let cut90 = (now - chrono::Duration::days(90)).to_rfc3339();
    let cut180 = (now - chrono::Duration::days(180)).to_rfc3339();
    let (over_30d, _, _) = db
        .age_buckets_root(&cut30, &cut90, &cut180)
        .unwrap_or_default();
    let root_chunks = collections
        .iter()
        .find(|collection| collection.name == "root")
        .map(|collection| collection.chunk_count)
        .unwrap_or(0);
    let root_ratio = if total_chunks == 0 {
        0.0
    } else {
        root_chunks as f64 / total_chunks as f64 * 100.0
    };
    let data_dir = &sys.config.data_dir;
    let disk_bytes = std::fs::metadata(data_dir.join("memory.db"))
        .map(|metadata| metadata.len())
        .unwrap_or(0)
        + dir_size(&data_dir.join("text_index"))
        + dir_size(&data_dir.join("vectors"));
    let disk_mb = disk_bytes as f64 / 1_048_576.0;
    let scope_options = collection_scope_options(&collections, "all");

    let mut recall_rows = String::new();
    for event in &recalls {
        recall_rows.push_str(&format!(
            r#"<tr>
             <td class="query" title="{}">{}</td>
             <td>{}</td>
             <td class="mono">{}</td>
             <td class="mono">{} ms</td>
             <td>{}</td>
             <td><span class="state {}">{}</span></td>
             <td class="feedback-actions">
              <button type="button" data-i18n="console.helpful" onclick="sendRecallFeedback('{}','helpful',this)">Helpful</button>
              <button type="button" data-i18n="console.harmful" onclick="sendRecallFeedback('{}','harmful',this)">Harmful</button>
             </td>
            </tr>"#,
            html_escape(&event.query),
            html_escape(&event.query),
            html_escape(&event.project),
            event.result_ids.len(),
            event.duration_ms,
            html_escape(&event.adapter),
            html_escape(&event.outcome),
            html_escape(&event.outcome),
            html_escape(&event.id),
            html_escape(&event.id),
        ));
    }
    if recall_rows.is_empty() {
        recall_rows = r#"<tr><td colspan="7" class="console-empty" data-i18n="console.noRecalls">No searches recorded yet. Candidates and results are tracked from the next search on.</td></tr>"#.to_string();
    }

    let mut job_rows = String::new();
    for job in &jobs {
        job_rows.push_str(&format!(
            r#"<tr>
             <td class="mono" title="{}">{}</td>
             <td><span class="state {}">{}</span></td>
             <td>{}</td>
             <td class="mono">{}</td>
            </tr>"#,
            html_escape(&job.target_id),
            html_escape(&job.target_id.chars().take(18).collect::<String>()),
            html_escape(&job.state),
            html_escape(&job.state),
            html_escape(&job.adapter),
            job.updated_at.format("%m-%d %H:%M"),
        ));
    }
    if job_rows.is_empty() {
        job_rows = r#"<tr><td colspan="4" class="console-empty" data-i18n="console.noJobs">No processing jobs.</td></tr>"#
            .to_string();
    }

    let mut collection_rows = String::new();
    for collection in &collections {
        collection_rows.push_str(&format!(
            r#"<tr>
             <td><a href="/collection/{}?format=html">{}</a></td>
             <td>{}</td>
             <td class="mono">{}</td>
             <td class="mono">{:.1} MB</td>
            </tr>"#,
            html_escape(&collection.name),
            html_escape(&collection.name),
            html_escape(&collection.kind),
            collection.chunk_count,
            collection.text_bytes as f64 / 1_048_576.0,
        ));
    }

    let mut recent_rows = String::new();
    for chunk in &recent {
        recent_rows.push_str(&format!(
            r#"<tr>
             <td class="query" title="{}">{}</td>
             <td>{}</td>
             <td>{:?}</td>
             <td class="mono">{}</td>
            </tr>"#,
            html_escape(&redact_text(&chunk.document)),
            html_escape(&redact_text(&chunk.document))
                .chars()
                .take(110)
                .collect::<String>(),
            html_escape(&chunk.project),
            chunk.importance,
            chunk.created_at.format("%m-%d %H:%M"),
        ));
    }

    let active_jobs = operations.queued_jobs + operations.running_jobs;
    let content = format!(
        r##"<div class="console">
         <header class="console-header">
          <div>
           <h1 class="console-title" data-i18n="console.title">Memory operations</h1>
           <p class="console-copy" data-i18n="console.subtitle">What got stored, what got searched, which memories were actually used in an answer, plus failed jobs and data skew, on one screen.</p>
          </div>
          <div class="console-address">
           <strong data-i18n="status.ok">Operational</strong>
           http://localhost:{}/<br>
           {}
          </div>
         </header>

         <form method="get" action="/viewer/search" class="console-search">
          <input name="q" aria-label="query" data-i18n-placeholder="console.searchPlaceholder" placeholder="Search decisions, errors, settings, procedures" required>
          <select name="project" aria-label="collection scope">{}</select>
          <button type="submit" data-i18n="button.search">Search</button>
         </form>

         <section class="console-metrics" aria-label="key metrics">
          <div class="console-metric"><span data-i18n="console.totalMemories">Total memories</span><strong>{}</strong></div>
          <div class="console-metric"><span data-i18n="console.searches24h">Searches, 24h</span><strong>{}</strong></div>
          <div class="console-metric"><span data-i18n="console.avgLatency">Average latency</span><strong>{:.0} ms</strong></div>
          <div class="console-metric"><span data-i18n="console.activeJobs">Jobs in flight</span><strong>{}</strong></div>
          <div class="console-metric"><span data-i18n="console.failedJobs">Failed jobs</span><strong>{}</strong></div>
          <div class="console-metric"><span data-i18n="console.storage">Storage</span><strong>{:.1} MB</strong></div>
         </section>

         <div class="console-alerts">
          <div class="console-alert {}"><span data-i18n="console.rootShare">Root share</span> {:.1}% ({})</div>
          <div class="console-alert {}"><span data-i18n="console.staleRoot">Root records older than 30 days</span> {}</div>
          <div class="console-alert {}"><span data-i18n="console.graphNodes">Graph nodes / facts</span> {} / {}</div>
          <div class="console-alert {}"><span data-i18n="console.verdicts">Helpful / harmful</span> {} / {}</div>
         </div>

         <div class="console-grid">
          <section class="console-section">
           <div class="console-section-head"><h2 data-i18n="console.recentSearches">Recent searches and injection candidates</h2><a href="/operations" data-i18n="console.viewJson">View JSON</a></div>
           <div class="overflow-x-auto">
            <table class="console-table">
             <thead><tr><th style="width:31%" data-i18n="console.colQuery">Query</th><th data-i18n="console.colScope">Scope</th><th data-i18n="console.colResults">Results</th><th data-i18n="console.colLatency">Latency</th><th data-i18n="console.colAdapter">Adapter</th><th data-i18n="console.colVerdict">Verdict</th><th data-i18n="console.colFeedback">Feedback</th></tr></thead>
             <tbody>{}</tbody>
            </table>
           </div>
          </section>
          <section class="console-section">
           <div class="console-section-head"><h2 data-i18n="console.jobs">Processing jobs</h2><a href="/operations" data-i18n="console.allStatus">All status</a></div>
           <div class="overflow-x-auto">
            <table class="console-table">
             <thead><tr><th style="width:38%" data-i18n="console.colTarget">Target</th><th data-i18n="console.colState">State</th><th data-i18n="console.colAdapter2">Adapter</th><th data-i18n="console.colUpdated">Updated</th></tr></thead>
             <tbody>{}</tbody>
            </table>
           </div>
          </section>
         </div>

         <div class="console-foot-grid">
          <section class="console-section">
           <div class="console-section-head"><h2 data-i18n="console.distribution">Collection distribution</h2><a href="/viewer/collections" data-i18n="console.manage">Manage</a></div>
           <div class="overflow-x-auto">
            <table class="console-table">
             <thead><tr><th data-i18n="console.colCollection">Collection</th><th data-i18n="console.colKind">Kind</th><th data-i18n="console.colMemories">Memories</th><th data-i18n="console.colText">Text</th></tr></thead>
             <tbody>{}</tbody>
            </table>
           </div>
          </section>
          <section class="console-section">
           <div class="console-section-head"><h2 data-i18n="console.recentSaves">Recently stored</h2><a href="/viewer/search" data-i18n="nav.search">Search</a></div>
           <div class="overflow-x-auto">
            <table class="console-table">
             <thead><tr><th style="width:48%" data-i18n="console.colContent">Content</th><th data-i18n="console.colProject">Project</th><th data-i18n="console.colImportance">Importance</th><th data-i18n="console.colTime">Time</th></tr></thead>
             <tbody>{}</tbody>
            </table>
           </div>
          </section>
         </div>

         <footer class="console-copy">
          <span data-i18n="console.footSessions">Sessions</span> {} · <span data-i18n="console.footNotes">Notes</span> {} · <span data-i18n="console.footModel">Embedding model</span> {} · <a href="/stats">/stats</a> · <a href="/operations">/operations</a>
         </footer>
        </div>"##,
        sys.config.api_port,
        html_escape(&sys.config.data_dir.display().to_string()),
        scope_options,
        total_chunks,
        operations.recalls_24h,
        operations.average_recall_ms_24h,
        active_jobs,
        operations.failed_jobs,
        disk_mb,
        if root_ratio > 70.0 { "" } else { "ok" },
        root_ratio,
        root_chunks,
        if over_30d > 10_000 { "" } else { "ok" },
        over_30d,
        if graph_nodes == 0 { "" } else { "ok" },
        graph_nodes,
        total_facts,
        if operations.harmful_24h > 0 { "" } else { "ok" },
        operations.helpful_24h,
        operations.harmful_24h,
        recall_rows,
        job_rows,
        collection_rows,
        recent_rows,
        total_sessions,
        total_notes,
        html_escape(&sys.config.embed_model),
    );

    let html = BASE_HTML
        .replace("__TITLE__", "Operations")
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
        rows = r#"<div class="col-empty" data-i18n="empty.collections">No collections yet</div>"#.to_string();
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
           <span class="col-metric-sub">high-signal notes you saved yourself</span>
          </div>
          <div class="col-metric">
           <span class="col-metric-label">Auto</span>
           <span class="col-metric-value num num-muted">{autolog}</span>
           <span class="col-metric-sub">logs left behind by tool calls</span>
          </div>
          <div class="col-metric">
           <span class="col-metric-label">Total</span>
           <span class="col-metric-value num">{total}</span>
           <span class="col-metric-sub">manual share {pct}%</span>
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
          <h1 class="col-title" data-i18n="collections.title">Collections</h1>
          <p class="col-subtitle" data-i18n="collections.subtitle">Buckets auto-named after your cwd. Only <em class="col-key col-key-pb">playbook</em> is the exception, a cross-project notebook searched from anywhere.</p>
         </div>
         <a href="/viewer/search" class="col-search-link" data-i18n="nav.search">Search</a>
        </header>
        {summary}
        <div class="col-grid">
         {rows}
        </div>"##,
        summary = summary_strip,
        rows = rows,
    );

    let html = BASE_HTML
        .replace("__TITLE__", "Collections")
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
        let results =
            run_hybrid_search(system.clone(), &q, &project, 20, false, true, false, None).await;
        let elapsed_ms = started.elapsed().as_millis();
        let recall_id = format!("recall_{}", uuid::Uuid::new_v4().simple());
        let event = RecallEvent {
            id: recall_id.clone(),
            query: redact_text(&q),
            project: project.clone(),
            result_ids: results.iter().map(|item| item.id.clone()).collect(),
            duration_ms: elapsed_ms.min(i64::MAX as u128) as i64,
            adapter: "dashboard".to_string(),
            outcome: "pending".to_string(),
            created_at: chrono::Utc::now(),
        };
        {
            let sys = system.read().await;
            let _ = sys.db.write().await.insert_recall_event(&event);
        }

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
                 <div class="text-sm mb-1">'{}' returned no results</div>
                 <div class="text-xs">recall {}</div>
                </div>"#,
                html_escape(&q),
                html_escape(&q),
                html_escape(&recall_id)
            )
        } else {
            format!(
                r#"<div class="flex items-center justify-between text-xs text-slate-500 mb-3"><span data-result-count="{}">{} results · sorted by relevance</span><span>{} ms, recall {}</span></div>{}"#,
                results.len(),
                results.len(),
                elapsed_ms,
                html_escape(&recall_id),
                items
            )
        }
    } else {
        r#"<div class="text-center py-12 text-slate-500 text-sm" data-i18n="search.empty">Enter a search query</div>"#
            .to_string()
    };

    let content = format!(
        r##"<div class="flex items-center justify-between mb-6">
         <div>
          <h1 class="text-xl font-semibold tracking-tight" data-i18n="search.title">Search</h1>
          <p class="text-sm text-slate-500 dark:text-slate-400 mt-0.5" data-i18n="search.subtitle">Enter a query and narrow it to a collection when needed.</p>
         </div>
         <a href="/viewer/collections" class="quiet-chip text-xs" data-i18n="search.collections">View collections</a>
        </div>
        <form method="get" action="/viewer/search" class="mb-6">
         <div class="command-input">
          <label class="search-field">
           <span class="field-label" data-i18n="field.query">Query</span>
           <input type="text" name="q" value="{}" placeholder="e.g. deployment decision, OAuth error, PostgreSQL config" data-i18n-placeholder="placeholder.search" class="w-full bg-transparent px-2 py-2 text-sm outline-none placeholder:text-stone-500" required>
          </label>
          <label>
           <span class="field-label" data-i18n="field.scope">Scope</span>
           <select name="project" class="scope-select">{}</select>
          </label>
          <button type="submit" class="rounded-2xl bg-slate-950 dark:bg-white text-white dark:text-slate-950 px-5 py-3 text-sm font-medium hover:opacity-90 transition-opacity" data-i18n="button.search">Search</button>
         </div>
        </form>
        {}"##,
        html_escape(&q),
        scope_options,
        results_html
    );

    let html = BASE_HTML
        .replace("__TITLE__", "Search")
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
        if let Some((_, last_end)) = merged.last_mut()
            && start <= *last_end
        {
            *last_end = (*last_end).max(end);
            continue;
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

/// Closes the feedback loop: memories the caller marked helpful float up, ones
/// marked harmful sink, so retrieval learns from real use instead of static
/// heuristics alone. Bounded to ±0.10 (the same order as importance/type
/// bonuses) and saturating, so a proven-useful memory outranks an untested peer
/// without letting vote count overwhelm lexical/vector relevance. `k` sets how
/// many net votes are needed to reach half the cap.
fn feedback_bonus(helpful: i64, harmful: i64) -> f32 {
    let net = (helpful - harmful) as f32;
    if net == 0.0 {
        return 0.0;
    }
    const CAP: f32 = 0.10;
    const K: f32 = 3.0;
    CAP * (net / (net.abs() + K))
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
        return candidates
            .into_iter()
            .take(limit)
            .map(|(it, _)| it)
            .collect();
    }
    let max_rel = candidates.first().map(|(it, _)| it.score).unwrap_or(0.0);
    let min_rel = candidates.last().map(|(it, _)| it.score).unwrap_or(0.0);
    let span = max_rel - min_rel;
    let norm = |s: f32| {
        if span.abs() <= f32::EPSILON {
            1.0
        } else {
            (s - min_rel) / span
        }
    };

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
pub(crate) mod retrieval_eval {
    //! Offline retrieval-quality baseline over a small hand-labeled corpus.
    //!
    //! Runs the *real* `run_hybrid_search` path (BM25 + vector + RRF + composite
    //! rerank + project diversification) so it doubles as a regression guard for
    //! any later ranking change (e.g. wiring MMR or retuning composite weights).
    //! It builds a throwaway data dir; the live server's store is never touched.
    //! The embedding model is symlinked from an existing fastembed cache so the
    //! test does not trigger a fresh ~400 MB download.
    use super::*;
    use crate::config::Config;
    use crate::eval::{mrr_at_k, precision_at_1, recall_at_k};
    use crate::models::{ChunkType, Importance, MemoryChunk, Metadata};

    fn labeled_corpus() -> Vec<(&'static str, &'static str, &'static str)> {
        vec![
            (
                "d_rust_async",
                "backend",
                "Rust async runtime tokio spawns tasks and awaits futures for non-blocking IO in the HTTP server",
            ),
            (
                "d_rust_err",
                "backend",
                "Error handling in Rust uses Result and the question mark operator with anyhow context wrapping",
            ),
            (
                "d_sqlite_wal",
                "backend",
                "SQLite WAL mode improves concurrent read performance and is set via PRAGMA journal_mode=WAL",
            ),
            (
                "d_cache_lru",
                "backend",
                "An in-memory LRU cache for embedding vectors avoids recomputing the model on repeated queries",
            ),
            (
                "d_hnsw",
                "search",
                "HNSW vector index for approximate nearest neighbor cosine similarity search over embeddings",
            ),
            (
                "d_bm25",
                "search",
                "BM25 lexical ranking through a Tantivy full text search index with stemming",
            ),
            (
                "d_rrf",
                "search",
                "Reciprocal rank fusion merges a vector result list and a keyword result list into one ranking",
            ),
            (
                "d_embed",
                "search",
                "The multilingual e5 embedding model produces 768 dimensional sentence vectors",
            ),
            (
                "d_graph",
                "search",
                "A knowledge graph stores subject predicate object edges describing entity relationships",
            ),
            (
                "d_deploy_k8s",
                "ops",
                "Deploy the service to Kubernetes with a Deployment manifest and a rolling update strategy",
            ),
            (
                "d_backup",
                "ops",
                "A nightly database backup cron job uploads a gzipped dump to object storage",
            ),
            (
                "d_tls",
                "ops",
                "Configure TLS certificates with Let's Encrypt and auto renew them via certbot",
            ),
            (
                "d_metrics",
                "ops",
                "Prometheus scrapes metrics and Grafana dashboards visualize request latency and error rate",
            ),
            (
                "d_ko_login",
                "app",
                "사용자 로그인 화면에서 비밀번호 재설정 링크를 이메일로 보낸다",
            ),
            (
                "d_ko_payment",
                "app",
                "결제 모듈은 카드 승인 후 영수증을 발급하고 환불은 3일 이내에 처리한다",
            ),
            (
                "d_ko_search",
                "app",
                "검색 기능은 한국어 형태소 분석과 조사 제거로 정확도를 높인다",
            ),
        ]
    }

    fn labeled_queries() -> Vec<(&'static str, Vec<&'static str>)> {
        vec![
            (
                "how does tokio handle non blocking io",
                vec!["d_rust_async"],
            ),
            (
                "combine keyword and vector search results into one ranking",
                vec!["d_rrf"],
            ),
            (
                "approximate nearest neighbor search over embeddings",
                vec!["d_hnsw"],
            ),
            ("sqlite write ahead log journal mode", vec!["d_sqlite_wal"]),
            ("renew https certificate automatically", vec!["d_tls"]),
            (
                "how many dimensions does the embedding model output",
                vec!["d_embed"],
            ),
            (
                "deploy to kubernetes with rolling updates",
                vec!["d_deploy_k8s"],
            ),
            ("비밀번호 재설정 이메일", vec!["d_ko_login"]),
            ("환불 처리 기간", vec!["d_ko_payment"]),
            ("한국어 조사 제거 검색", vec!["d_ko_search"]),
        ]
    }

    /// Symlink an already-downloaded fastembed model into the throwaway data dir
    /// so the test reuses the live model cache instead of downloading afresh.
    fn link_model_cache(data_dir: &std::path::Path) {
        let model_dir = data_dir.join("models");
        let _ = std::fs::create_dir_all(&model_dir);
        let home = dirs::home_dir().unwrap_or_default();
        let candidates = [
            home.join(".palimpsest/models"),
            home.join(".memnest/models"),
            home.join(".factory/memories/models"),
        ];
        for cand in candidates {
            if !cand.is_dir() {
                continue;
            }
            if let Ok(entries) = std::fs::read_dir(&cand) {
                for entry in entries.flatten() {
                    let name = entry.file_name();
                    if name.to_string_lossy().starts_with("models--") {
                        let link = model_dir.join(&name);
                        if !link.exists() {
                            #[cfg(unix)]
                            let _ = std::os::unix::fs::symlink(entry.path(), &link);
                            #[cfg(windows)]
                            let _ = std::os::windows::fs::symlink_dir(entry.path(), &link);
                        }
                    }
                }
            }
            return;
        }
    }

    pub(crate) async fn build_system() -> (tempfile::TempDir, Arc<RwLock<MemorySystem>>) {
        let tmp = tempfile::tempdir().expect("tempdir");
        link_model_cache(tmp.path());
        let mut cfg = Config::default();
        cfg.data_dir = tmp.path().to_path_buf();
        let sys = MemorySystem::new(cfg)
            .await
            .expect("MemorySystem::new failed (offline and no cached model?)");
        (tmp, Arc::new(RwLock::new(sys)))
    }

    async fn ingest(system: &Arc<RwLock<MemorySystem>>, corpus: &[(&str, &str, &str)]) {
        let sys = system.read().await;
        for &(id, project, text) in corpus {
            let embedding = sys.embedder.encode_document(text).expect("encode_document");
            let now = chrono::Utc::now();
            let chunk = MemoryChunk {
                id: id.to_string(),
                project: project.to_string(),
                document: text.to_string(),
                embedding: Some(embedding.clone()),
                metadata: Metadata {
                    chunk_type: ChunkType::Manual,
                    importance: Importance::Knowledge,
                    ..Default::default()
                },
                created_at: now,
                updated_at: now,
            };
            sys.db
                .write()
                .await
                .insert_chunk(&chunk)
                .expect("insert_chunk");
            sys.add_text_doc(id, project, text)
                .await
                .expect("add_text_doc");
            sys.vector_index
                .write()
                .await
                .add(id, &embedding)
                .expect("vector add");
        }
    }

    #[tokio::test]
    async fn retrieval_baseline_recall_and_mrr() {
        let (_tmp, system) = build_system().await;
        ingest(&system, &labeled_corpus()).await;

        let queries = labeled_queries();
        let k = 5usize;
        let mut gold = Vec::new();
        let mut retrieved = Vec::new();
        for (q, g) in &queries {
            let items =
                run_hybrid_search(system.clone(), q, "all", k, false, false, false, None).await;
            gold.push(g.iter().map(|s| s.to_string()).collect::<Vec<_>>());
            retrieved.push(items.iter().map(|it| it.id.clone()).collect::<Vec<_>>());
        }

        let r1 = recall_at_k(&gold, &retrieved, 1);
        let r3 = recall_at_k(&gold, &retrieved, 3);
        let r5 = recall_at_k(&gold, &retrieved, 5);
        let mrr = mrr_at_k(&gold, &retrieved, k);
        let p1 = precision_at_1(&gold, &retrieved);

        eprintln!(
            "\n=== memnest retrieval eval (queries={}) ===",
            queries.len()
        );
        eprintln!(
            "recall@1={r1:.3}  recall@3={r3:.3}  recall@5={r5:.3}  MRR@5={mrr:.3}  P@1={p1:.3}"
        );
        for ((q, _), (g, r)) in queries.iter().zip(gold.iter().zip(retrieved.iter())) {
            let top: Vec<&String> = r.iter().take(5).collect();
            eprintln!("  q='{q}'  gold={g:?}  top5={top:?}");
        }

        // Baseline observed at recall@5 = MRR = 1.0 on this (deliberately
        // unambiguous) corpus. The floor below guards against gross ranking
        // breakage; it intentionally leaves a small margin so a single query may
        // degrade without a red build. NOTE: this corpus saturates at 1.0, so it
        // cannot yet *measure* diversity/MMR gains — P1 adds a redundant-cluster,
        // multi-relevant sub-corpus to create that headroom.
        assert!(r5 >= 0.9, "recall@5 regressed below 0.9: {r5:.3}");
        assert!(mrr >= 0.85, "MRR@5 regressed below 0.85: {mrr:.3}");
        assert!(p1 >= 0.8, "precision@1 regressed below 0.8: {p1:.3}");
    }

    /// A redundant cluster (all one project, mimicking an auto-logged store)
    /// plus one distinct follow-up carrying unique information.
    fn cluster_corpus() -> Vec<(&'static str, &'static str, &'static str)> {
        vec![
            (
                "inc_dup1",
                "incidents",
                "Deploy failed at 02:14 because the database migration lock was held by a stuck worker; we killed the stuck worker and the migration completed and the deploy succeeded",
            ),
            (
                "inc_dup2",
                "incidents",
                "The 02:14 deploy failure was caused by a stuck worker holding the database migration lock; killing the stuck worker let the migration finish and the deploy succeed",
            ),
            (
                "inc_dup3",
                "incidents",
                "Our 02:14 deploy failed because a stuck worker was holding the migration lock on the database; after we terminated the stuck worker the migration ran and the deploy went through",
            ),
            (
                "inc_distinct",
                "incidents",
                "Preventive fix after the migration lock incident: added a thirty second statement lock timeout and a liveness healthcheck that automatically restarts unresponsive workers",
            ),
        ]
    }

    async fn top_ids_and_embeddings(
        system: &Arc<RwLock<MemorySystem>>,
        query: &str,
        k: usize,
    ) -> (Vec<String>, Vec<Vec<f32>>) {
        let items =
            run_hybrid_search(system.clone(), query, "all", k, false, false, false, None).await;
        let sys = system.read().await;
        let db = sys.db.read().await;
        let mut ids = Vec::new();
        let mut embs = Vec::new();
        for it in &items {
            ids.push(it.id.clone());
            let emb = db
                .get_chunk(&it.id)
                .ok()
                .flatten()
                .and_then(|c| c.embedding)
                .unwrap_or_default();
            embs.push(emb);
        }
        (ids, embs)
    }

    // On realistic data MMR is a *safety net*: it must never make the result
    // list more redundant or less relevant than pure relevance ranking. The
    // strong de-duplication behaviour (replacing near-identical hits with
    // distinct ones) is proven deterministically in
    // `mmr_select_breaks_up_near_duplicates`; it only fires when a
    // comparably-relevant *and* embedding-distinct alternative exists, which is
    // rare among paraphrase-level chunks (relevance and embedding similarity are
    // coupled). This test guards the no-regression property on real embeddings.
    #[tokio::test]
    async fn mmr_does_not_regress_relevance_or_redundancy() {
        use crate::eval::intra_list_redundancy;
        let (_tmp, system) = build_system().await;
        ingest(&system, &labeled_corpus()).await;
        ingest(&system, &cluster_corpus()).await;

        let query = "why did the 02:14 deploy fail with a stuck worker holding the migration lock";
        let relevant: Vec<String> = ["inc_dup1", "inc_dup2", "inc_dup3", "inc_distinct"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let k = 3usize;
        let thr = 0.88f32;

        // Relevance-only: MMR with diversity weight ~0 (lambda near 1.0).
        system.write().await.config.mmr_lambda = 0.99;
        let (rel_ids, rel_emb) = top_ids_and_embeddings(&system, query, k).await;
        let red_rel = intra_list_redundancy(&rel_emb, thr);
        let rec_rel = recall_at_k(&[relevant.clone()], &[rel_ids.clone()], k);

        // Balanced MMR (production default).
        system.write().await.config.mmr_lambda = 0.5;
        let (mmr_ids, mmr_emb) = top_ids_and_embeddings(&system, query, k).await;
        let red_mmr = intra_list_redundancy(&mmr_emb, thr);
        let rec_mmr = recall_at_k(&[relevant.clone()], &[mmr_ids.clone()], k);

        eprintln!("\n=== MMR vs relevance-only (k={k}, redundancy thr={thr}) ===");
        eprintln!(
            "  relevance-only top{k}={rel_ids:?}  redundancy={red_rel:.3}  recall(relevant)={rec_rel:.3}"
        );
        eprintln!(
            "  MMR(0.5)       top{k}={mmr_ids:?}  redundancy={red_mmr:.3}  recall(relevant)={rec_mmr:.3}"
        );

        // MMR must not increase redundancy nor lose relevant recall.
        assert!(
            red_mmr <= red_rel + 1e-9,
            "MMR raised redundancy: {red_mmr:.3} > {red_rel:.3}"
        );
        assert!(
            rec_mmr >= rec_rel - 1e-9,
            "MMR hurt relevant recall: {rec_mmr:.3} < {rec_rel:.3}"
        );
    }

    #[test]
    fn mmr_select_breaks_up_near_duplicates() {
        let item = |id: &str, score: f32| SearchResultItem {
            id: id.to_string(),
            doc_len: 0,
            project: "p".to_string(),
            document: String::new(),
            score,
            timestamp: String::new(),
            chunk_type: String::new(),
            importance: String::new(),
            category: String::new(),
            memory_kind: "record".to_string(),
            confidence: None,
            adapter: String::new(),
            helpful_count: 0,
            harmful_count: 0,
        };
        let dup = vec![1.0f32, 0.0, 0.0];
        let other = vec![0.0f32, 1.0, 0.0];
        let make = || {
            vec![
                (item("a", 1.00), dup.clone()),
                (item("b", 0.99), dup.clone()),
                (item("c", 0.98), dup.clone()),
                (item("d", 0.80), other.clone()),
            ]
        };
        // Pure relevance (lambda ~1): the three near-duplicates win.
        let rel: Vec<String> = mmr_select(make(), 0.99, 3)
            .into_iter()
            .map(|i| i.id)
            .collect();
        assert_eq!(rel, vec!["a", "b", "c"]);
        // Balanced MMR breaks the dup run and surfaces the distinct doc.
        let mmr: Vec<String> = mmr_select(make(), 0.5, 3)
            .into_iter()
            .map(|i| i.id)
            .collect();
        assert!(
            mmr.contains(&"d".to_string()),
            "MMR did not surface distinct doc: {mmr:?}"
        );
    }

    /// The MCP `memory_search` tool must return the same ranking as the HTTP
    /// /search path now that it delegates to the shared core. Guards against a
    /// future revert that re-forks the ranking logic.
    #[tokio::test]
    async fn mcp_search_matches_http_ranking() {
        let (_tmp, system) = build_system().await;
        ingest(&system, &labeled_corpus()).await;
        let query = "approximate nearest neighbor search over embeddings";
        let http =
            run_hybrid_search(system.clone(), query, "all", 5, false, false, false, None).await;
        let http_ids: Vec<String> = http.iter().map(|it| it.id.clone()).collect();
        assert!(!http_ids.is_empty(), "http path returned nothing");
        let mcp_out = crate::server::mcp::memory_search(
            system.clone(),
            &serde_json::json!({"query": query, "n_results": 5}),
        )
        .await
        .expect("mcp memory_search");
        // Every HTTP id must appear in the MCP output in the same relative order.
        let mut last = 0usize;
        for id in &http_ids {
            let needle = format!("id={id}");
            let pos = mcp_out
                .find(&needle)
                .unwrap_or_else(|| panic!("MCP output missing {id}:\n{mcp_out}"));
            assert!(pos >= last, "MCP ranking order diverged at {id}");
            last = pos;
        }
    }

    /// Gives the composite re-rank measurable coverage beyond the saturated
    /// baseline: two chunks with identical text (hence identical lexical/vector
    /// relevance) but different importance/type/recency must order by the
    /// composite bonuses — the important, recent, manual chunk above the stale
    /// auto-logged one.
    #[tokio::test]
    async fn composite_ranks_important_recent_above_stale_log() {
        let (_tmp, system) = build_system().await;
        let text =
            "the widget service circuit breaker trips after five consecutive upstream timeouts";
        {
            let sys = system.read().await;
            let emb = sys.embedder.encode_document(text).expect("encode");
            let now = chrono::Utc::now();
            let mk = |id: &str,
                      imp: Importance,
                      ct: ChunkType,
                      created: chrono::DateTime<chrono::Utc>| {
                MemoryChunk {
                    id: id.to_string(),
                    project: "svc".to_string(),
                    document: text.to_string(),
                    embedding: Some(emb.clone()),
                    metadata: Metadata {
                        chunk_type: ct,
                        importance: imp,
                        ..Default::default()
                    },
                    created_at: created,
                    updated_at: created,
                }
            };
            // doc_hi carries a non-default category to verify it round-trips to
            // SearchResultItem.category (the field the learning layer filters on).
            let mut hi = mk("doc_hi", Importance::Knowledge, ChunkType::Manual, now);
            hi.metadata.category = crate::models::MemoryCategory::Insight;
            let lo = mk(
                "doc_lo",
                Importance::Log,
                ChunkType::AutoLog,
                now - chrono::Duration::days(200),
            );
            sys.db.write().await.insert_chunk(&hi).expect("insert hi");
            sys.db.write().await.insert_chunk(&lo).expect("insert lo");
            sys.add_text_doc("doc_hi", "svc", text)
                .await
                .expect("text hi");
            sys.add_text_doc("doc_lo", "svc", text)
                .await
                .expect("text lo");
            sys.vector_index
                .write()
                .await
                .add("doc_hi", &emb)
                .expect("vec hi");
            sys.vector_index
                .write()
                .await
                .add("doc_lo", &emb)
                .expect("vec lo");
        }
        let items = run_hybrid_search(
            system.clone(),
            "circuit breaker upstream timeouts widget service",
            "all",
            2,
            false,
            false,
            false,
            None,
        )
        .await;
        let ids: Vec<String> = items.iter().map(|it| it.id.clone()).collect();
        eprintln!("composite order: {ids:?}");
        assert_eq!(
            ids.first().map(String::as_str),
            Some("doc_hi"),
            "important+recent+manual should outrank stale autolog: {ids:?}"
        );
        let hi_item = items
            .iter()
            .find(|it| it.id == "doc_hi")
            .expect("doc_hi present");
        assert_eq!(
            hi_item.category, "Insight",
            "category should round-trip to results"
        );
    }

    #[test]
    fn feedback_bonus_is_bounded_and_directional() {
        assert_eq!(feedback_bonus(0, 0), 0.0);
        // Helpful raises, harmful lowers, symmetric around zero.
        assert!(feedback_bonus(1, 0) > 0.0);
        assert!(feedback_bonus(0, 1) < 0.0);
        assert!((feedback_bonus(5, 0) + feedback_bonus(0, 5)).abs() < 1e-6);
        // Net feedback drives it, and it saturates within ±0.10.
        assert_eq!(feedback_bonus(4, 1), feedback_bonus(3, 0));
        assert!(feedback_bonus(1000, 0) <= 0.10);
        assert!(feedback_bonus(1000, 0) > feedback_bonus(3, 0));
        assert!(feedback_bonus(0, 1000) >= -0.10);
    }

    /// End-to-end proof that recorded feedback re-orders retrieval: two chunks
    /// with identical text, importance, type, and recency tie on every static
    /// signal, so the only thing that can separate them is the helpful/harmful
    /// counter. The helpful one must win, the harmful one must lose.
    #[tokio::test]
    async fn feedback_reorders_otherwise_identical_memories() {
        let (_tmp, system) = build_system().await;
        let text = "the payment reconciliation job retries with exponential backoff on gateway 503";
        {
            let sys = system.read().await;
            let emb = sys.embedder.encode_document(text).expect("encode");
            let now = chrono::Utc::now();
            let mk = |id: &str, helpful: i64, harmful: i64| MemoryChunk {
                id: id.to_string(),
                project: "pay".to_string(),
                document: text.to_string(),
                embedding: Some(emb.clone()),
                metadata: Metadata {
                    chunk_type: ChunkType::Manual,
                    importance: Importance::Knowledge,
                    helpful_count: helpful,
                    harmful_count: harmful,
                    ..Default::default()
                },
                created_at: now,
                updated_at: now,
            };
            for (id, h, x) in [("doc_helpful", 5, 0), ("doc_neutral", 0, 0), ("doc_harmful", 0, 5)] {
                let chunk = mk(id, h, x);
                sys.db.write().await.insert_chunk(&chunk).expect("insert");
                sys.add_text_doc(id, "pay", text).await.expect("text");
                sys.vector_index.write().await.add(id, &emb).expect("vec");
            }
        }
        let items = run_hybrid_search(
            system.clone(),
            "payment reconciliation retries exponential backoff gateway 503",
            "all",
            3,
            false,
            false,
            false,
            None,
        )
        .await;
        let ids: Vec<String> = items.iter().map(|it| it.id.clone()).collect();
        eprintln!("feedback order: {ids:?}");
        assert_eq!(
            ids.first().map(String::as_str),
            Some("doc_helpful"),
            "helpful memory should rank first: {ids:?}"
        );
        assert_eq!(
            ids.last().map(String::as_str),
            Some("doc_harmful"),
            "harmful memory should rank last: {ids:?}"
        );
    }

    #[test]
    fn context_prompt_respects_char_budget() {
        let memories: Vec<SearchResultItem> = (0..50)
            .map(|i| SearchResultItem {
                id: format!("m{i}"),
                project: "p".to_string(),
                document: "lorem ipsum dolor sit amet ".repeat(20),
                doc_len: 0,
                score: 1.0 - i as f32 * 0.01,
                timestamp: String::new(),
                chunk_type: "Manual".to_string(),
                importance: "Knowledge".to_string(),
                category: "General".to_string(),
                memory_kind: "record".to_string(),
                confidence: None,
                adapter: String::new(),
                helpful_count: 0,
                harmful_count: 0,
            })
            .collect();
        // Tight budget: must stay within it and flag truncation.
        let max = 800usize;
        let out = render_context_prompt(&[], &[], &memories, max);
        assert!(
            out.chars().count() <= max,
            "prompt {} chars exceeds budget {max}",
            out.chars().count()
        );
        assert!(
            out.contains("(context truncated to fit budget)"),
            "expected truncation marker"
        );
        assert!(out.starts_with("<memnest_context>") && out.ends_with("</memnest_context>"));
        // Generous budget: everything fits, no truncation.
        let full = render_context_prompt(&[], &[], &memories, 100_000);
        assert!(
            !full.contains("(context truncated"),
            "unexpected truncation"
        );
        assert!(
            full.contains("m0") && full.contains("m49"),
            "missing memories"
        );

        let korean = vec![SearchResultItem {
            id: "ko".to_string(),
            project: "한국어".to_string(),
            document: "한글 메모리는 글자 수로 예산을 계산해야 합니다".to_string(),
            doc_len: 25,
            score: 1.0,
            timestamp: String::new(),
            chunk_type: "Manual".to_string(),
            importance: "Knowledge".to_string(),
            category: "General".to_string(),
            memory_kind: "record".to_string(),
            confidence: None,
            adapter: String::new(),
            helpful_count: 0,
            harmful_count: 0,
        }];
        let rendered = render_context_prompt(&[], &[], &korean, 200);
        assert!(rendered.contains("한글 메모리"));
        assert!(rendered.chars().count() <= 200);
    }

    #[test]
    fn recommendations_threshold_mapping() {
        // Below both thresholds: no recommendations.
        assert!(build_recommendations(0, 0).is_empty());
        assert!(build_recommendations(49_999, 2 * 1024 * 1024 * 1024 - 1).is_empty());
        // At exactly the threshold: no trigger (threshold is strict >).
        assert!(build_recommendations(50_000, 2 * 1024 * 1024 * 1024).is_empty());
        // Over root-chunks threshold.
        let recs = build_recommendations(50_001, 0);
        assert_eq!(recs.len(), 1);
        assert!(recs[0].contains("root"));
        // Over disk threshold.
        let recs = build_recommendations(0, 2 * 1024 * 1024 * 1024 + 1);
        assert_eq!(recs.len(), 1);
        assert!(recs[0].contains("disk"));
        // Both over threshold: two recommendations.
        let recs = build_recommendations(100_000, 3 * 1024 * 1024 * 1024);
        assert_eq!(recs.len(), 2);
    }

    #[tokio::test]
    async fn neighbors_finds_semantic_duplicates() {
        let (_tmp, system) = build_system().await;
        ingest(&system, &labeled_corpus()).await;
        let out = neighbors(
            State(system.clone()),
            Json(NeighborsRequest {
                text: String::new(),
                id: "d_hnsw".to_string(),
                k: 5,
                max_distance: 0.0,
                project: "all".to_string(),
            }),
        )
        .await
        .0;
        assert!(!out.is_empty(), "neighbors returned nothing");
        assert!(
            out.iter().all(|n| n.id != "d_hnsw"),
            "self should be excluded"
        );
        for w in out.windows(2) {
            assert!(
                w[0].distance <= w[1].distance + 1e-6,
                "distances must be ascending"
            );
        }
        // the nearest neighbours of the HNSW doc are other search-topic docs
        assert_eq!(
            out[0].project, "search",
            "nearest neighbour should be a search-topic doc: {:?}",
            out[0]
        );
    }
}
