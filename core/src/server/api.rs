use axum::{
    extract::{Path, Query, State},
    response::{Html, IntoResponse, Json, Response},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::MemorySystem;
use crate::models::*;
use crate::redaction::redact_text;

// ── Request/Response Types ───────────────────────────────────

#[derive(Deserialize)]
pub struct SearchRequest {
    pub query: String,
    #[serde(default)]
    pub project: String,
    #[serde(default = "default_n")]
    pub n_results: usize,
    #[serde(default)]
    pub recent_first: bool,
    /// Optional semantic category filter (e.g. "failure", "insight").
    #[serde(default)]
    pub category: String,
    /// Drop reserved autolog buckets (root/default/global/_superseded) from
    /// cross-project results. Pass project="root" explicitly to read them.
    #[serde(default)]
    pub exclude_reserved: bool,
    /// Safe integration label used only for local observability.
    #[serde(default = "default_http_adapter")]
    pub adapter: String,
}

fn default_n() -> usize {
    3
}
fn default_http_adapter() -> String {
    "http".to_string()
}

#[derive(Serialize)]
pub struct SearchResponse {
    pub results: Vec<SearchResultItem>,
    pub total: usize,
    pub elapsed_ms: u128,
    pub recall_id: String,
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
    pub text: String,
    #[serde(default)]
    pub project: String,
    #[serde(default)]
    pub metadata: Option<Metadata>,
    #[serde(default)]
    pub sensitive: Option<bool>,
}

#[derive(Deserialize)]
pub struct DeleteRequest {
    pub ids: Vec<String>,
}

#[derive(Deserialize)]
pub struct UpdateRequest {
    pub id: String,
    #[serde(default)]
    pub sensitive: Option<bool>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub metadata: Option<MetadataPatch>,
    #[serde(default)]
    pub chunk_type: Option<ChunkType>,
    #[serde(default)]
    pub importance: Option<Importance>,
}

#[derive(Deserialize)]
pub struct MetadataPatch {
    #[serde(default)]
    pub chunk_type: Option<ChunkType>,
    #[serde(default)]
    pub importance: Option<Importance>,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    // parent_session_id is deliberately absent: session lineage is recorded at
    // capture time and must not be rewritten through the public update path.
    // Metadata itself keeps the field so existing rows still round-trip.
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub event_id: Option<String>,
    #[serde(default)]
    pub sequence: Option<i64>,
    #[serde(default)]
    pub total: Option<i64>,
    #[serde(default)]
    pub truncated: Option<bool>,
    #[serde(default)]
    pub raw_chunk: Option<String>,
    #[serde(default)]
    pub access_count: Option<i64>,
    #[serde(default)]
    pub keywords: Option<Vec<String>>,
    #[serde(default)]
    pub sensitive: Option<bool>,
    #[serde(default)]
    pub pinned: Option<bool>,
    #[serde(default)]
    pub adapter: Option<String>,
    #[serde(default)]
    pub adapter_version: Option<String>,
    #[serde(default)]
    pub memory_kind: Option<MemoryKind>,
    #[serde(default)]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub source_ids: Option<Vec<String>>,
    #[serde(default)]
    pub supersedes: Option<String>,
    #[serde(default)]
    pub verified_at: Option<String>,
}

#[derive(Deserialize)]
pub struct FeedbackRequest {
    recall_id: String,
    #[serde(default)]
    memory_id: Option<String>,
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
    /// Required, exactly like `/search`. There is no implicit `all`: an
    /// unscoped pack would inject other projects' text into a prompt.
    #[serde(default)]
    project: String,
    #[serde(default = "default_context_results")]
    n_results: usize,
    #[serde(default = "default_context_chars")]
    max_chars: usize,
    /// Optional category filter for the retrieved memories part.
    #[serde(default)]
    category: String,
}

fn default_context_results() -> usize {
    3
}
fn default_context_chars() -> usize {
    2000
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
pub struct ContextResponse {
    pub query: String,
    pub project: String,
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
    collections: Vec<CollectionEntry>,
    age_buckets: AgeBuckets,
    disk: DiskStats,
    recommendations: Vec<String>,
    operations: OperationsSummary,
}

// ── API Handlers ─────────────────────────────────────────────

fn operation_error_response(error: super::operations::OperationError) -> Response {
    let status = match error.kind {
        super::operations::ErrorKind::BadRequest => axum::http::StatusCode::BAD_REQUEST,
        super::operations::ErrorKind::NotFound => axum::http::StatusCode::NOT_FOUND,
        super::operations::ErrorKind::Conflict => axum::http::StatusCode::CONFLICT,
        super::operations::ErrorKind::Internal => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
    };
    let message = if matches!(error.kind, super::operations::ErrorKind::Internal) {
        "internal operation failed"
    } else {
        &error.message
    };
    (
        status,
        Json(serde_json::json!({"status":"error","error":message})),
    )
        .into_response()
}

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
/// search-result excerpt. The returned document is bounded at 8,000 chars so
/// agents can read skills and lessons without silently losing content.
pub async fn get_chunk_full(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Path(id): Path<String>,
) -> Response {
    match super::operations::get(system, &id).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => operation_error_response(error),
    }
}

pub async fn search(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Json(req): Json<SearchRequest>,
) -> Response {
    let input = super::operations::SearchInput {
        query: req.query,
        project: req.project,
        n_results: req.n_results,
        recent_first: req.recent_first,
        category: (!req.category.trim().is_empty()).then_some(req.category),
        exclude_reserved: req.exclude_reserved,
        adapter: req.adapter,
    };
    match super::operations::search(system, input).await {
        Ok(out) => Json(out).into_response(),
        Err(error) => operation_error_response(error),
    }
}

pub(crate) async fn run_hybrid_search(
    system: Arc<RwLock<MemorySystem>>,
    query: &str,
    project: &str,
    n: usize,
    recent_first: bool,
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

    let vector_results = {
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
            if super::operations::exclude_project(&c.project, exclude_reserved) {
                continue;
            }
            if let Some(cf) = &cat_filter {
                // Compare against the serde (snake_case) name so multi-word
                // Categories are client-supplied semantic labels, e.g.
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
                    document: redacted.chars().take(600).collect(),
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

fn is_transcript_chunk(metadata: &Metadata) -> bool {
    metadata.chunk_type == ChunkType::AutoLog
        && metadata
            .event_id
            .as_deref()
            .is_some_and(|id| !id.is_empty())
        && metadata
            .source
            .as_deref()
            .is_some_and(|source| source.ends_with(".transcript"))
}

fn transcript_chunk_id(project: &str, metadata: &Metadata) -> Option<String> {
    if !is_transcript_chunk(metadata) {
        return None;
    }
    let mut hasher = Sha256::new();
    let sequence = metadata.sequence.unwrap_or(1).to_string();
    for field in [
        project,
        metadata.adapter.as_deref().unwrap_or_default(),
        &metadata.session_id,
        metadata.event_id.as_deref().unwrap_or_default(),
        &sequence,
    ] {
        hasher.update(field.as_bytes());
        hasher.update([0]);
    }
    let digest = format!("{:x}", hasher.finalize());
    Some(format!("transcript_{}", &digest[..32]))
}

async fn repair_transcript_indexes(sys: &MemorySystem, chunk: &MemoryChunk) -> anyhow::Result<()> {
    sys.add_text_doc(&chunk.id, &chunk.project, &chunk.document)
        .await?;
    let embedding = chunk
        .embedding
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("transcript chunk {} has no embedding", chunk.id))?;
    let mut vector_index = sys.vector_index.write().await;
    if !vector_index.contains(&chunk.id) {
        vector_index.add(&chunk.id, embedding)?;
    }
    vector_index.save()?;
    Ok(())
}

pub async fn add(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Json(req): Json<AddRequest>,
) -> Response {
    match super::operations::remember(
        system,
        super::operations::RememberInput {
            text: req.text,
            project: req.project,
            metadata: req.metadata,
            sensitive: req.sensitive.unwrap_or(false),
        },
    )
    .await
    {
        Ok(out) => (axum::http::StatusCode::CREATED, Json(out)).into_response(),
        Err(error) => operation_error_response(error),
    }
}

pub(crate) async fn add_impl(
    system: Arc<RwLock<MemorySystem>>,
    req: AddRequest,
) -> HashMap<String, String> {
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
        return map;
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

    let transcript_id = transcript_chunk_id(&project, &metadata);
    if let Some(id) = &transcript_id {
        let existing = {
            let sys = system.read().await;
            let db = sys.db.read().await;
            db.get_chunk(id)
        };
        match existing {
            Ok(Some(chunk)) => {
                let repair = {
                    let sys = system.read().await;
                    repair_transcript_indexes(&sys, &chunk).await
                };
                let mut map = HashMap::new();
                map.insert("id".to_string(), id.clone());
                map.insert("project".to_string(), project);
                match repair {
                    Ok(()) => {
                        map.insert("status".to_string(), "deduplicated".to_string());
                    }
                    Err(error) => {
                        map.insert("status".to_string(), "failed".to_string());
                        map.insert("error".to_string(), error.to_string());
                    }
                }
                return map;
            }
            Ok(None) => {}
            Err(error) => {
                let mut map = HashMap::new();
                map.insert("status".to_string(), "failed".to_string());
                map.insert("id".to_string(), id.clone());
                map.insert("project".to_string(), project);
                map.insert("error".to_string(), error.to_string());
                return map;
            }
        }
    } else {
        let sys = system.read().await;
        let db = sys.db.read().await;
        if let Ok(Some(existing_id)) = db.find_exact_duplicate(&project, &text) {
            drop(db);
            let _ = sys.db.write().await.touch_chunk(&existing_id);
            let mut map = HashMap::new();
            map.insert("status".to_string(), "deduplicated".to_string());
            map.insert("id".to_string(), existing_id);
            map.insert("project".to_string(), project);
            return map;
        }
    }

    let id = transcript_id.unwrap_or_else(|| format!("manual_{}", uuid::Uuid::new_v4().simple()));
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
    map
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

    // Transcript chunks are event records: repeated words in distinct turns
    // must remain distinct. Their deterministic id above provides retry-only
    // deduplication, so content exact/semantic dedup does not apply.
    if !is_transcript_chunk(&metadata) {
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
        if is_transcript_chunk(&chunk.metadata) {
            vector_index.save()?;
        } else {
            let _ = vector_index.save();
        }
    }
    Ok(None)
}

pub async fn delete(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Json(req): Json<DeleteRequest>,
) -> Response {
    match super::operations::delete(system, req.ids).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => operation_error_response(error),
    }
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
) -> Response {
    match super::operations::update(system, req).await {
        Ok(value) => Json(value).into_response(),
        Err(error) => operation_error_response(error),
    }
}

pub(crate) async fn update_impl(
    system: Arc<RwLock<MemorySystem>>,
    req: UpdateRequest,
) -> HashMap<String, serde_json::Value> {
    let mut out = HashMap::new();
    let id = req.id.trim();
    if id.is_empty() {
        out.insert("status".to_string(), serde_json::json!("error"));
        out.insert("message".to_string(), serde_json::json!("id is required"));
        return out;
    }

    let sys = system.read().await;
    let mut chunk = {
        let db = sys.db.read().await;
        match db.get_chunk(id) {
            Ok(Some(chunk)) => chunk,
            Ok(None) => {
                out.insert("status".to_string(), serde_json::json!("not_found"));
                out.insert("id".to_string(), serde_json::json!(id));
                return out;
            }
            Err(e) => {
                out.insert("status".to_string(), serde_json::json!("error"));
                out.insert("message".to_string(), serde_json::json!(e.to_string()));
                return out;
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
            return out;
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
                return out;
            }
            Err(e) => {
                out.insert("status".to_string(), serde_json::json!("error"));
                out.insert(
                    "message".to_string(),
                    serde_json::json!(format!("embed join: {e}")),
                );
                return out;
            }
        }
    }

    chunk.updated_at = chrono::Utc::now();
    match sys.db.write().await.insert_chunk(&chunk) {
        Ok(()) => {}
        Err(e) => {
            out.insert("status".to_string(), serde_json::json!("error"));
            out.insert("message".to_string(), serde_json::json!(e.to_string()));
            return out;
        }
    }

    if let Err(e) = sys
        .add_text_doc(&chunk.id, &chunk.project, &chunk.document)
        .await
    {
        out.insert("status".to_string(), serde_json::json!("error"));
        out.insert("message".to_string(), serde_json::json!(e.to_string()));
        return out;
    }
    if embedding_changed && let Some(embedding) = &chunk.embedding {
        let mut vector_index = sys.vector_index.write().await;
        if let Err(e) = vector_index
            .add(&chunk.id, embedding)
            .and_then(|_| vector_index.save())
        {
            out.insert("status".to_string(), serde_json::json!("error"));
            out.insert("message".to_string(), serde_json::json!(e.to_string()));
            return out;
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
    out
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
) -> Response {
    if req.project.trim().is_empty() {
        return operation_error_response(super::operations::OperationError::bad(
            "project is required; use project=all explicitly for cross-project context",
        ));
    }
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
            req.max_chars,
            cat,
        )
        .await,
    )
    .into_response()
}

/// Shared context-pack builder behind the HTTP `/context` endpoint. Retrieves
/// project-scoped memories and renders a budget-bounded prompt string. Notes
/// and facts are deliberately not part of the pack: they are global, so
/// including them would leak text across projects.
pub(crate) async fn build_context(
    system: Arc<RwLock<MemorySystem>>,
    query: &str,
    project: &str,
    n_results: usize,
    max_chars: usize,
    category: Option<String>,
) -> ContextResponse {
    let query = query.trim().to_string();
    let project = project.trim().to_string();
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
            project == "all",
            category,
        )
        .await
    };

    let prompt = render_context_prompt(&memories, max_chars);
    ContextResponse {
        query,
        project,
        memories,
        prompt,
    }
}

/// Render the context prompt, never exceeding `max_chars`. Once the budget is
/// hit the remainder is dropped and a truncation marker is appended. This keeps
/// the prompt-pack — whose whole purpose is to economise the model's context —
/// from itself blowing the window.
fn render_context_prompt(memories: &[SearchResultItem], max_chars: usize) -> String {
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

pub async fn list_collections(
    State(system): State<Arc<RwLock<MemorySystem>>>,
) -> Json<Vec<CollectionStat>> {
    let sys = system.read().await;
    let db = sys.db.read().await;
    // collection_stats already drops internal buckets; the retain is a second
    // barrier so a future storage change cannot leak trash into the listing.
    let mut stats = db.collection_stats(500).unwrap_or_default();
    stats.retain(|stat| !is_internal_project(&stat.name));
    Json(stats)
}

pub async fn collection_detail(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Path(name): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    // `_trash` and `_superseded` hold soft-deleted bodies. They are reachable
    // by id through /chunk/{id} for restore, never browsable as a collection.
    if is_internal_project(&name) {
        return (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"status":"not_found","error":"collection not found"})),
        )
            .into_response();
    }
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

        // The collection name is attacker-controlled (it is whatever project
        // string was ever written). Escape it for the body here, and again for
        // the <title> inside render_page, so it can never be parsed as markup.
        let content = format!(
            r##"<div class="flex items-center justify-between mb-6">
             <div>
              <h1 class="text-xl font-semibold tracking-tight">{}</h1>
              <p class="text-sm text-gray-500 dark:text-gray-400 mt-0.5" data-memory-count="{}">{} memories</p>
             </div>
             <a href="/" class="quiet-chip text-xs" data-i18n="nav.dashboard">Dashboard</a>
            </div>
            {}"##,
            html_escape(&name),
            chunks.len(),
            chunks.len(),
            items_html
        );

        return Html(render_page(&name, Nav::None, &content)).into_response();
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
) -> Response {
    if req.key.trim().is_empty() || req.value.is_empty() {
        return (
            axum::http::StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"status":"error","message":"key and value are required"})),
        )
            .into_response();
    }
    if !system.read().await.vault_enabled || !crate::crypto::is_enabled() {
        return (axum::http::StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({"status":"error","message":"secret vault crypto is unavailable"}))).into_response();
    }
    let secret = Secret {
        key: req.key.trim().to_string(),
        kind: req.kind,
        value: req.value,
        note: req.note,
        updated: chrono::Utc::now(),
    };
    let sys = system.read().await;
    match sys.db.write().await.insert_secret(&secret) {
        Ok(()) => (
            axum::http::StatusCode::CREATED,
            Json(serde_json::json!({"status":"ok","key":secret.key,"encryption":"aes-256-gcm"})),
        )
            .into_response(),
        Err(_error) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"status":"error","message":"secret operation failed"})),
        )
            .into_response(),
    }
}

/// GET /secrets/{key} — decrypt and return a credential value.
pub async fn get_secret(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Path(key): Path<String>,
) -> Response {
    if !system.read().await.vault_enabled || !crate::crypto::is_enabled() {
        return (axum::http::StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({"status":"error","message":"secret vault crypto is unavailable"}))).into_response();
    }
    let sys = system.read().await;
    let result = {
        let db = sys.db.read().await;
        db.get_secret(&key)
    };
    match result {
        Ok(Some(secret)) => Json(serde_json::json!({"status":"ok","key":secret.key,"kind":secret.kind,"note":secret.note,"value":secret.value,"updated":secret.updated.to_rfc3339()})).into_response(),
        Ok(None) => (axum::http::StatusCode::NOT_FOUND, Json(serde_json::json!({"status":"not_found","key":key}))).into_response(),
        Err(_error) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"status":"error","message":"secret operation failed"}))).into_response(),
    }
}

/// GET /secrets — list metadata only. Values are never returned by this endpoint.
pub async fn list_secrets(State(system): State<Arc<RwLock<MemorySystem>>>) -> Response {
    if !system.read().await.vault_enabled || !crate::crypto::is_enabled() {
        return (axum::http::StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({"status":"error","message":"secret vault crypto is unavailable"}))).into_response();
    }
    let sys = system.read().await;
    let db = sys.db.read().await;
    let secrets = match db.list_secret_meta() {
        Ok(secrets) => secrets,
        Err(_) => {
            return (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"status":"error","message":"secret operation failed"})),
            )
                .into_response();
        }
    };
    let items: Vec<HashMap<String, String>> = secrets
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
    Json(items).into_response()
}

pub async fn delete_secret(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Path(key): Path<String>,
) -> Response {
    if !system.read().await.vault_enabled || !crate::crypto::is_enabled() {
        return (axum::http::StatusCode::SERVICE_UNAVAILABLE, Json(serde_json::json!({"status":"error","message":"secret vault crypto is unavailable"}))).into_response();
    }
    let sys = system.read().await;
    let result = sys.db.write().await.delete_secret(&key);
    match result {
        Ok(true) => Json(serde_json::json!({"status":"ok","key":key})).into_response(),
        Ok(false) => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({"status":"not_found","key":key})),
        )
            .into_response(),
        Err(_error) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"status":"error","message":"secret operation failed"})),
        )
            .into_response(),
    }
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
        summary: db.operations_summary().unwrap_or_default(),
        recalls: db.recent_recall_events(limit).unwrap_or_default(),
        jobs: db.recent_processing_jobs(limit).unwrap_or_default(),
    })
}

pub async fn recall_feedback(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    Json(req): Json<FeedbackRequest>,
) -> Response {
    match super::operations::feedback(
        system,
        &req.recall_id,
        req.memory_id.as_deref(),
        &req.outcome,
        req.note.as_deref(),
    )
    .await
    {
        Ok(value) => Json(value).into_response(),
        Err(error) => operation_error_response(error),
    }
}

pub async fn stats(State(system): State<Arc<RwLock<MemorySystem>>>) -> Json<StatsResponse> {
    let sys = system.read().await;
    let db = sys.db.read().await;

    // Every figure below is user-visible, so all of them come from the
    // internal-bucket-filtered storage helpers: soft-deleted rows never
    // inflate a total, a collection row, or a cleanup recommendation.
    let total_chunks = db.chunk_count().unwrap_or(0);
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

    let root_chunks = db.chunk_count_by_project("root").unwrap_or(0);
    let recommendations =
        build_recommendations(root_chunks, db_bytes + text_index_bytes + vector_bytes);
    let operations = db.operations_summary().unwrap_or_default();

    Json(StatsResponse {
        total_chunks,
        collections,
        age_buckets,
        disk,
        recommendations,
        operations,
    })
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
.hidden { display: none; }
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
.w-5 { width: 1.25rem; }
.w-7 { width: 1.75rem; }
.w-9 { width: 2.25rem; }
.h-5 { height: 1.25rem; }
.h-7 { height: 1.75rem; }
.h-9 { height: 2.25rem; }
.h-14 { height: 3.5rem; }
.max-w-6xl { max-width: 72rem; }
.mx-auto { margin-left: auto; margin-right: auto; }
.ml-2 { margin-left: .5rem; }
.ml-auto { margin-left: auto; }
.mt-0\.5 { margin-top: .125rem; }
.mt-2 { margin-top: .5rem; }
.mt-3 { margin-top: .75rem; }
.mb-1 { margin-bottom: .25rem; }
.mb-2 { margin-bottom: .5rem; }
.mb-3 { margin-bottom: .75rem; }
.mb-6 { margin-bottom: 1.5rem; }
.p-2 { padding: .5rem; }
.p-3 { padding: .75rem; }
.p-4 { padding: 1rem; }
.px-1\.5 { padding-left: .375rem; padding-right: .375rem; }
.px-2 { padding-left: .5rem; padding-right: .5rem; }
.px-3 { padding-left: .75rem; padding-right: .75rem; }
.px-4 { padding-left: 1rem; padding-right: 1rem; }
.px-5 { padding-left: 1.25rem; padding-right: 1.25rem; }
.py-0\.5 { padding-top: .125rem; padding-bottom: .125rem; }
.py-2 { padding-top: .5rem; padding-bottom: .5rem; }
.py-3 { padding-top: .75rem; padding-bottom: .75rem; }
.py-6 { padding-top: 1.5rem; padding-bottom: 1.5rem; }
.py-12 { padding-top: 3rem; padding-bottom: 3rem; }
.pt-14 { padding-top: 3.5rem; }
.gap-2 { gap: .5rem; }
.space-y-1 > * + * { margin-top: .25rem; }
.items-center { align-items: center; }
.justify-center { justify-content: center; }
.justify-between { justify-content: space-between; }
.flex-wrap { flex-wrap: wrap; }
.overflow-x-auto { overflow-x: auto; }
.whitespace-pre-wrap { white-space: pre-wrap; }
.rounded { border-radius: .25rem; }
.rounded-md { border-radius: .375rem; }
.rounded-lg { border-radius: .5rem; }
.rounded-2xl { border-radius: 1rem; }
.rounded-full { border-radius: 9999px; }
.border { border: 1px solid rgba(55,50,36,.12); }
.border-gray-200 { border-color: rgb(229 231 235); }
.bg-transparent { background: transparent; }
.bg-white { background: rgb(255 255 255); }
.bg-gray-100 { background: rgb(243 244 246); }
.bg-slate-100 { background: rgb(241 245 249); }
.bg-slate-950, .bg-stone-950 { background: rgb(12 10 9); }
.bg-emerald-100 { background: rgb(209 250 229); }
.text-center { text-align: center; }
.text-xs { font-size: .75rem; line-height: 1rem; }
.text-sm { font-size: .875rem; line-height: 1.25rem; }
.text-xl { font-size: 1.25rem; line-height: 1.75rem; }
.text-\[10px\] { font-size: 10px; line-height: 1rem; }
.text-\[11px\] { font-size: 11px; line-height: 1rem; }
.font-medium { font-weight: 500; }
.font-semibold { font-weight: 600; }
.tracking-tight, .tracking-\[0\.18em\] { letter-spacing: 0; }
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
.text-stone-500 { color: rgb(120 113 108); }
.text-stone-700 { color: rgb(68 64 60); }
.text-emerald-700 { color: rgb(4 120 87); }
.text-amber-500 { color: rgb(245 158 11); }
.placeholder\:text-stone-500::placeholder { color: rgb(120 113 108); }
.outline-none { outline: none; }
.transition-colors { transition: color .18s ease, background-color .18s ease, border-color .18s ease; }
.transition-opacity { transition: opacity .18s ease; }
.hover\:opacity-90:hover { opacity: .9; }
.hover\:bg-white\/5:hover { background: rgba(255,255,255,.05); }
.hover\:bg-white\/40:hover { background: rgba(255,255,255,.40); }
.antialiased { -webkit-font-smoothing: antialiased; -moz-osx-font-smoothing: grayscale; }
.dark .dark\:hidden { display: none; }
.dark .dark\:inline { display: inline; }
.dark .dark\:bg-gray-800 { background: rgb(31 41 55); }
.dark .dark\:bg-gray-900 { background: rgb(17 24 39); }
.dark .dark\:bg-stone-100, .dark .dark\:bg-white { background: rgb(245 245 244); }
.dark .dark\:bg-emerald-400\/10 { background: rgba(52,211,153,.10); }
.dark .dark\:bg-white\/10 { background: rgba(255,255,255,.10); }
.dark .dark\:border-gray-800 { border-color: rgb(31 41 55); }
.dark .dark\:text-gray-100 { color: rgb(243 244 246); }
.dark .dark\:text-gray-200 { color: rgb(229 231 235); }
.dark .dark\:text-gray-400 { color: rgb(156 163 175); }
.dark .dark\:text-slate-200 { color: rgb(226 232 240); }
.dark .dark\:text-slate-300 { color: rgb(203 213 225); }
.dark .dark\:text-slate-400 { color: rgb(148 163 184); }
.dark .dark\:text-slate-950 { color: rgb(2 6 23); }
.dark .dark\:text-stone-200 { color: rgb(231 229 228); }
.dark .dark\:text-stone-400 { color: rgb(168 162 158); }
.dark .dark\:text-emerald-300 { color: rgb(110 231 183); }
.dark .dark\:hover\:bg-white\/10:hover { background: rgba(255,255,255,.10); }
@media (min-width: 768px) {
  .md\:flex { display: flex; }
  .md\:hidden { display: none; }
  .md\:pt-0 { padding-top: 0; }
  .md\:py-8 { padding-top: 2rem; padding-bottom: 2rem; }}
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
  .product-topbar { display: flex; }}
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
  .workbench { padding-top: 72px; }}
.workbench .panel {
  border-radius: 18px;
  background: rgba(255,253,245,.48);
  border: 1px solid rgba(53,49,38,.11);
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
  .command-input {
    align-items: stretch;
    flex-direction: column;
    border-radius: 18px;
  }
  .scope-select {
    width: 100%;
    min-width: 0;
  }
  .top-actions {
    min-width: 0;
  }}

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
  .console-metric:nth-child(4) { border-left: 0; }}
@media (max-width: 640px) {
  .console-search { grid-template-columns: 1fr; }
  .console-metrics { grid-template-columns: repeat(2, minmax(0, 1fr)); }
  .console-metric:nth-child(odd) { border-left: 0; }
  .console-title { font-size: 24px; }}</style>
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
  <a href="/viewer/search" class="top-link __ACTIVE_SEARCH__" data-i18n="nav.search">Search</a>
 </nav>
 <div class="top-actions">
  <div class="locale-switch" aria-label="language">
   <button type="button" data-lang-button="ko">KR</button>
   <button type="button" data-lang-button="en">EN</button>
  </div>
  <a href="/health" class="status-pill"><span class="status-dot"></span><span data-i18n="status.ok">Operational</span></a>
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
 <a href="/viewer/search" class="block px-3 py-2 rounded-md text-sm text-gray-600 dark:text-gray-400 hover:bg-white/5" data-i18n="nav.search">Search</a>
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
// memory_id is mandatory: /feedback only moves a memory's ranking when it knows
// which returned memory the verdict is about.
async function sendRecallFeedback(recallId, memoryId, outcome, button) {
 var group = button.parentNode;
 group.querySelectorAll('button').forEach(function(b) { b.disabled = true; });
 try {
  var response = await fetch('/feedback', {
   method: 'POST',
   headers: { 'content-type': 'application/json' },
   body: JSON.stringify({ recall_id: recallId, memory_id: memoryId, outcome: outcome })
  });
  if (!response.ok) throw new Error('request failed');
  var mark = document.createElement('span');
  mark.className = 'state ' + outcome;
  mark.textContent = outcome;
  group.replaceChildren(mark);
 } catch (_) {
  group.querySelectorAll('button').forEach(function(b) { b.disabled = false; });
 }
}
var i18n = {
 ko: {
  'brand.subtitle': '로컬 메모리',
  'nav.dashboard': '대시보드',
  'nav.search': '검색',
  'status.ok': '서비스 정상',
  'field.query': '검색어',
  'field.scope': '범위',
  'placeholder.search': '예: 배포 결정, OAuth 오류, PostgreSQL 설정',
  'button.search': '검색',
  'scope.all': '전체 컬렉션',
  'empty.collections': '아직 수집된 컬렉션이 없습니다',
  'empty.recent': '최근 메모리가 없습니다',
  'search.title': '검색',
  'search.subtitle': '검색어를 입력하고 필요한 컬렉션만 좁혀 봅니다.',
  'search.empty': '검색어를 입력하세요',
  'console.title': '메모리 운영 상태',
  'console.subtitle': '무엇이 저장되고 검색됐는지, 어떤 검색이 도움됨으로 표시됐는지, 처리 실패와 데이터 편중을 한 화면에서 확인합니다.',
  'console.searchPlaceholder': '결정, 오류, 설정, 작업 절차 검색',
  'console.totalMemories': '활성 메모리',
  'console.collections': '컬렉션',
  'console.searches24h': '24시간 검색',
  'console.avgLatency': '평균 검색 지연',
  'console.failedJobs': '실패 작업',
  'console.storage': '저장 공간',
  'console.rootShare': 'root 집중도',
  'console.staleRoot': '30일 초과 root 기록',
  'console.verdicts': '도움됨 / 문제',
  'console.recentSearches': '최근 검색',
  'console.viewJson': 'JSON 보기',
  'console.jobs': '처리 작업',
  'console.allStatus': '전체 상태',
  'console.distribution': '컬렉션 분포',
  'console.autologNote': 'watch 수집 항목은 `memnest watch`가 기록한 실제 대화 turn이고, 나머지는 직접 저장한 메모리입니다.',
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
  'console.colTarget': '대상',
  'console.colState': '상태',
  'console.colAdapter2': '어댑터',
  'console.colUpdated': '업데이트',
  'console.colCollection': '컬렉션',
  'console.colMemories': '메모리',
  'console.colAutolog': 'watch 수집',
  'console.colText': '텍스트',
  'console.colContent': '내용',
  'console.colProject': '프로젝트',
  'console.colImportance': '중요도',
  'console.colTime': '시간',
  'console.footModel': '임베딩 모델'
 },
 en: {
  'brand.subtitle': 'Local memory',
  'nav.dashboard': 'Dashboard',
  'nav.search': 'Search',
  'status.ok': 'Operational',
  'field.query': 'Query',
  'field.scope': 'Scope',
  'placeholder.search': 'e.g. deployment decision, OAuth error, PostgreSQL config',
  'button.search': 'Search',
  'scope.all': 'All collections',
  'empty.collections': 'No collections have been captured yet',
  'empty.recent': 'No recent memories',
  'search.title': 'Search',
  'search.subtitle': 'Enter a query and narrow it to a collection when needed.',
  'search.empty': 'Enter a search query',
  'console.title': 'Memory operations',
  'console.subtitle': 'What is stored, what was searched, and which searches were marked helpful, plus failed jobs and data skew, on one screen.',
  'console.searchPlaceholder': 'Search decisions, errors, settings, procedures',
  'console.totalMemories': 'Active memories',
  'console.collections': 'Collections',
  'console.searches24h': 'Searches, 24h',
  'console.avgLatency': 'Average latency',
  'console.failedJobs': 'Failed jobs',
  'console.storage': 'Storage',
  'console.rootShare': 'Root share',
  'console.staleRoot': 'Root records older than 30 days',
  'console.verdicts': 'Helpful / harmful',
  'console.recentSearches': 'Recent searches',
  'console.viewJson': 'View JSON',
  'console.jobs': 'Processing jobs',
  'console.allStatus': 'All status',
  'console.distribution': 'Collection distribution',
  'console.autologNote': 'Watch-captured rows are visible conversation turns filed by `memnest watch`; the rest were saved deliberately.',
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
  'console.colTarget': 'Target',
  'console.colState': 'State',
  'console.colAdapter2': 'Adapter',
  'console.colUpdated': 'Updated',
  'console.colCollection': 'Collection',
  'console.colMemories': 'Memories',
  'console.colAutolog': 'Watch-captured',
  'console.colText': 'Text',
  'console.colContent': 'Content',
  'console.colProject': 'Project',
  'console.colImportance': 'Importance',
  'console.colTime': 'Time',
  'console.footModel': 'Embedding model'
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

/// Which top-nav entry is highlighted for the page being rendered.
#[derive(Clone, Copy)]
enum Nav {
    Dashboard,
    Search,
    None,
}

/// Fill the page shell in one left-to-right pass. A single pass matters: with
/// chained `String::replace` a value substituted earlier (a collection name, say)
/// could contain a later placeholder token and get expanded a second time.
fn render_page(title: &str, nav: Nav, content: &str) -> String {
    let title = html_escape(title);
    let slots: [(&str, &str); 3] = [
        ("__TITLE__", title.as_str()),
        ("__CONTENT__", content),
        (
            "__ACTIVE_DASHBOARD__",
            if matches!(nav, Nav::Dashboard) {
                "is-active"
            } else {
                ""
            },
        ),
    ];
    let search_slot = (
        "__ACTIVE_SEARCH__",
        if matches!(nav, Nav::Search) {
            "is-active"
        } else {
            ""
        },
    );

    let mut out = String::with_capacity(BASE_HTML.len() + content.len());
    let mut rest = BASE_HTML;
    while let Some(offset) = rest.find("__") {
        out.push_str(&rest[..offset]);
        let tail = &rest[offset..];
        match slots
            .iter()
            .chain(std::iter::once(&search_slot))
            .find(|(token, _)| tail.starts_with(token))
        {
            Some((token, value)) => {
                out.push_str(value);
                rest = &tail[token.len()..];
            }
            None => {
                out.push_str("__");
                rest = &tail[2..];
            }
        }
    }
    out.push_str(rest);
    out
}

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

    // chunk_count / collection_stats / recent_chunks all exclude the internal
    // `_trash` and `_superseded` buckets, so nothing soft-deleted is counted,
    // listed, or previewed on this page.
    let total_chunks = db.chunk_count().unwrap_or(0);
    let collections = db.collection_stats(8).unwrap_or_default();
    let collection_count = db.collection_stats(500).unwrap_or_default().len();
    let recent = db.recent_chunks(6).unwrap_or_default();
    let recalls = db.recent_recall_events(8).unwrap_or_default();
    let jobs = db.recent_processing_jobs(8).unwrap_or_default();
    let operations = db.operations_summary().unwrap_or_default();
    let now = chrono::Utc::now();
    let cut30 = (now - chrono::Duration::days(30)).to_rfc3339();
    let cut90 = (now - chrono::Duration::days(90)).to_rfc3339();
    let cut180 = (now - chrono::Duration::days(180)).to_rfc3339();
    let (over_30d, _, _) = db
        .age_buckets_root(&cut30, &cut90, &cut180)
        .unwrap_or_default();
    // Counted straight from the database, not from the 8-row preview list, so
    // the share is a real fraction of every collection rather than of the page.
    let root_chunks = db.chunk_count_by_project("root").unwrap_or(0);
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
            </tr>"#,
            html_escape(&event.query),
            html_escape(&event.query),
            html_escape(&event.project),
            event.result_ids.len(),
            event.duration_ms,
            html_escape(&event.adapter),
            html_escape(&event.outcome),
            html_escape(&event.outcome),
        ));
    }
    if recall_rows.is_empty() {
        recall_rows = r#"<tr><td colspan="6" class="console-empty" data-i18n="console.noRecalls">No searches recorded yet. Candidates and results are tracked from the next search on.</td></tr>"#.to_string();
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
             <td class="mono">{}</td>
             <td class="mono">{}</td>
             <td class="mono">{:.1} MB</td>
            </tr>"#,
            url_encode(&collection.name),
            html_escape(&collection.name),
            collection.chunk_count,
            collection.autolog_count,
            collection.text_bytes as f64 / 1_048_576.0,
        ));
    }
    if collection_rows.is_empty() {
        collection_rows = r#"<tr><td colspan="4" class="console-empty" data-i18n="empty.collections">No collections have been captured yet</td></tr>"#.to_string();
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
    if recent_rows.is_empty() {
        recent_rows = r#"<tr><td colspan="4" class="console-empty" data-i18n="empty.recent">No recent memories</td></tr>"#.to_string();
    }

    let content = format!(
        r##"<div class="console">
         <header class="console-header">
          <div>
           <h1 class="console-title" data-i18n="console.title">Memory operations</h1>
           <p class="console-copy" data-i18n="console.subtitle">What is stored, what was searched, and which searches were marked helpful, plus failed jobs and data skew, on one screen.</p>
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
          <div class="console-metric"><span data-i18n="console.totalMemories">Active memories</span><strong>{}</strong></div>
          <div class="console-metric"><span data-i18n="console.collections">Collections</span><strong>{}</strong></div>
          <div class="console-metric"><span data-i18n="console.searches24h">Searches, 24h</span><strong>{}</strong></div>
          <div class="console-metric"><span data-i18n="console.avgLatency">Average latency</span><strong>{:.0} ms</strong></div>
          <div class="console-metric"><span data-i18n="console.failedJobs">Failed jobs</span><strong>{}</strong></div>
          <div class="console-metric"><span data-i18n="console.storage">Storage</span><strong>{:.1} MB</strong></div>
         </section>

         <div class="console-alerts">
          <div class="console-alert {}"><span data-i18n="console.rootShare">Root share</span> {:.1}% ({})</div>
          <div class="console-alert {}"><span data-i18n="console.staleRoot">Root records older than 30 days</span> {}</div>
          <div class="console-alert {}"><span data-i18n="console.verdicts">Helpful / harmful</span> {} / {}</div>
         </div>

         <div class="console-grid">
          <section class="console-section">
           <div class="console-section-head"><h2 data-i18n="console.recentSearches">Recent searches</h2><a href="/operations" data-i18n="console.viewJson">View JSON</a></div>
           <div class="overflow-x-auto">
            <table class="console-table">
             <thead><tr><th style="width:34%" data-i18n="console.colQuery">Query</th><th data-i18n="console.colScope">Scope</th><th data-i18n="console.colResults">Results</th><th data-i18n="console.colLatency">Latency</th><th data-i18n="console.colAdapter">Adapter</th><th data-i18n="console.colVerdict">Verdict</th></tr></thead>
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
           <div class="console-section-head"><h2 data-i18n="console.distribution">Collection distribution</h2><a href="/collections" data-i18n="console.viewJson">View JSON</a></div>
           <div class="overflow-x-auto">
            <table class="console-table">
             <thead><tr><th data-i18n="console.colCollection">Collection</th><th data-i18n="console.colMemories">Memories</th><th data-i18n="console.colAutolog">Watch-captured</th><th data-i18n="console.colText">Text</th></tr></thead>
             <tbody>{}</tbody>
            </table>
           </div>
           <p class="console-copy" data-i18n="console.autologNote">Watch-captured rows are visible conversation turns filed by `memnest watch`; the rest were saved deliberately.</p>
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
          <span data-i18n="console.footModel">Embedding model</span> {} · <a href="/health">/health</a> · <a href="/stats">/stats</a> · <a href="/operations">/operations</a>
         </footer>
        </div>"##,
        sys.config.api_port,
        html_escape(&sys.config.data_dir.display().to_string()),
        scope_options,
        total_chunks,
        collection_count,
        operations.recalls_24h,
        operations.average_recall_ms_24h,
        operations.failed_jobs,
        disk_mb,
        if root_ratio > 70.0 { "" } else { "ok" },
        root_ratio,
        root_chunks,
        if over_30d > 10_000 { "" } else { "ok" },
        over_30d,
        if operations.harmful_24h > 0 { "" } else { "ok" },
        operations.helpful_24h,
        operations.harmful_24h,
        recall_rows,
        job_rows,
        collection_rows,
        recent_rows,
        html_escape(&sys.config.embed_model),
    );

    Html(render_page("Operations", Nav::Dashboard, &content))
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

    let results_html = if q.trim().is_empty() {
        r#"<div class="text-center py-12 text-slate-500 text-sm" data-i18n="search.empty">Enter a search query</div>"#
            .to_string()
    } else {
        // Same shared path as POST /search, so the dashboard ranks results
        // exactly like every agent-facing client and records one recall event.
        let outcome = super::operations::search(
            system.clone(),
            super::operations::SearchInput {
                query: q.clone(),
                project: project.clone(),
                n_results: 20,
                recent_first: false,
                category: None,
                exclude_reserved: false,
                adapter: "dashboard".to_string(),
            },
        )
        .await;
        match outcome {
            Err(error) => format!(
                r#"<div class="text-center py-12 text-slate-500 text-sm">{}</div>"#,
                html_escape(&error.message)
            ),
            Ok(out) if out.results.is_empty() => format!(
                r#"<div class="text-center py-12 text-slate-500" data-empty-query="{}">
                 <div class="text-sm mb-1">'{}' returned no results</div>
                 <div class="text-xs">recall {}</div>
                </div>"#,
                html_escape(&q),
                html_escape(&q),
                html_escape(&out.recall_id)
            ),
            Ok(out) => {
                let mut items = String::new();
                for item in &out.results {
                    // Feedback carries the memory id, which is what
                    // set_recall_feedback needs to move that memory's ranking.
                    items.push_str(&format!(
                        r#"<article class="panel p-4 mb-3 search-result">
                         <div class="flex flex-wrap items-center gap-2 mb-2">
                          <span class="text-[10px] px-2 py-0.5 rounded-full bg-emerald-100 text-emerald-700 dark:bg-emerald-400/10 dark:text-emerald-300 font-medium">{importance}</span>
                          <span class="text-[10px] px-2 py-0.5 rounded-full bg-slate-100 dark:bg-white/10 text-slate-600 dark:text-slate-300">{chunk_type}</span>
                          <span class="text-[10px] text-slate-400 ml-auto">{timestamp}</span>
                         </div>
                         <div class="text-xs text-slate-500 mb-2">{project}</div>
                         <div class="text-sm text-slate-800 dark:text-slate-200 whitespace-pre-wrap leading-relaxed">{document}</div>
                         <div class="mt-3 flex items-center gap-2 text-[11px] text-slate-400">
                          <span>score {score:.4}</span>
                          <span class="feedback-actions ml-auto">
                           <button type="button" data-i18n="console.helpful" onclick="sendRecallFeedback('{recall}','{memory}','helpful',this)">Helpful</button>
                           <button type="button" data-i18n="console.harmful" onclick="sendRecallFeedback('{recall}','{memory}','harmful',this)">Harmful</button>
                          </span>
                         </div>
                        </article>"#,
                        importance = html_escape(&item.importance),
                        chunk_type = html_escape(&item.chunk_type),
                        timestamp = html_escape(&item.timestamp),
                        project = html_escape(&item.project),
                        document = highlight_query_html(&item.document, &q),
                        score = item.score,
                        recall = html_escape(&out.recall_id),
                        memory = html_escape(&item.id),
                    ));
                }
                format!(
                    r#"<div class="flex items-center justify-between text-xs text-slate-500 mb-3"><span data-result-count="{}">{} results · sorted by relevance</span><span>{} ms, recall {}</span></div>{}"#,
                    out.results.len(),
                    out.results.len(),
                    out.elapsed_ms,
                    html_escape(&out.recall_id),
                    items
                )
            }
        }
    };

    let content = format!(
        r##"<div class="mb-6">
         <h1 class="text-xl font-semibold tracking-tight" data-i18n="search.title">Search</h1>
         <p class="text-sm text-slate-500 dark:text-slate-400 mt-0.5" data-i18n="search.subtitle">Enter a query and narrow it to a collection when needed.</p>
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

    Html(render_page("Search", Nav::Search, &content))
}

/// Percent-encode a value for use inside a URL path segment. Only unreserved
/// characters survive, so a collection name can never break out of the path or
/// smuggle an attribute terminator into a generated link.
fn url_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
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
mod viewer_tests {
    //! Guards for the two blockers the viewer used to ship: unescaped
    //! attacker-controlled collection names, and soft-deleted rows leaking back
    //! into every count and listing.
    use super::test_support::build_system;
    use super::*;

    const SOURCE: &str = include_str!("api.rs");
    const MALICIOUS: &str = r#"<script>alert('xss')</script>"#;

    async fn body_text(response: Response) -> String {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    async fn store(system: &Arc<RwLock<MemorySystem>>, project: &str, text: &str) -> String {
        super::super::operations::remember(
            system.clone(),
            super::super::operations::RememberInput {
                text: text.to_string(),
                project: project.to_string(),
                metadata: None,
                sensitive: false,
            },
        )
        .await
        .expect("remember")
        .id
    }

    /// Collection names come from whatever a client wrote, so the HTML view has
    /// to treat them as hostile input. CSP allows inline script on this origin,
    /// which means a single unescaped `<script>` would execute.
    #[tokio::test]
    async fn malicious_collection_name_is_never_rendered_as_live_html() {
        let (_tmp, system) = build_system().await;
        // Written straight to the DB: the public write path would reject
        // nothing here, but this keeps the test independent of write policy.
        {
            let sys = system.read().await;
            let now = chrono::Utc::now();
            sys.db
                .write()
                .await
                .insert_chunk(&MemoryChunk {
                    id: "xss_probe".into(),
                    project: MALICIOUS.into(),
                    document: "harmless body".into(),
                    embedding: None,
                    metadata: Metadata::default(),
                    created_at: now,
                    updated_at: now,
                })
                .expect("insert");
        }

        let html = body_text(
            collection_detail(
                State(system.clone()),
                Path(MALICIOUS.to_string()),
                Query(HashMap::from([("format".to_string(), "html".to_string())])),
            )
            .await,
        )
        .await;

        assert!(
            !html.contains(MALICIOUS),
            "raw collection name reached the page body"
        );
        assert!(
            !html.contains("<script>alert"),
            "collection name produced a live script tag"
        );
        assert!(
            html.contains("&lt;script&gt;alert(&#39;xss&#39;)&lt;/script&gt;"),
            "escaped collection name missing from the body"
        );
        let title = html
            .lines()
            .find(|line| line.contains("<title>"))
            .expect("page has a title");
        assert!(
            title.contains("&lt;script&gt;") && !title.contains("<script>"),
            "unescaped name reached <title>: {title}"
        );

        // Generated links percent-encode the name, so it cannot terminate the
        // href attribute or the path.
        let dashboard = viewer_dashboard(State(system)).await.0;
        assert!(
            !dashboard.contains(MALICIOUS) && !dashboard.contains("<script>alert"),
            "dashboard rendered the raw collection name"
        );
        assert!(
            dashboard.contains("/collection/%3Cscript%3E"),
            "dashboard link was not percent-encoded"
        );
    }

    /// A soft-deleted memory must vanish from every user-visible surface at
    /// once: totals, the collection listing, the dashboard, and the JSON stats.
    /// It stays fetchable by id so `/restore` still works.
    #[tokio::test]
    async fn soft_deleted_memory_disappears_from_every_visible_surface() {
        let (_tmp, system) = build_system().await;
        let keep = store(&system, "survivor", "kept memory about deploy locks").await;
        let doomed_text = "doomed memory about migration rollbacks";
        let doomed = store(&system, "condemned", doomed_text).await;

        let before = {
            let sys = system.read().await;
            let db = sys.db.read().await;
            db.chunk_count().unwrap()
        };
        assert_eq!(before, 2);

        super::super::operations::delete(system.clone(), vec![doomed.clone()])
            .await
            .expect("soft delete");

        let sys_guard = system.read().await;
        let db = sys_guard.db.read().await;
        assert_eq!(db.chunk_count().unwrap(), 1, "trashed row still counted");
        let names: Vec<String> = db
            .collection_stats(500)
            .unwrap()
            .into_iter()
            .map(|stat| stat.name)
            .collect();
        assert_eq!(names, vec!["survivor".to_string()]);
        assert!(
            db.recent_chunks(10)
                .unwrap()
                .iter()
                .all(|chunk| chunk.id != doomed),
            "trashed row still in recent saves"
        );
        // Still addressable by id, so restore keeps working.
        assert_eq!(
            db.get_chunk(&doomed).unwrap().unwrap().project,
            "_trash",
            "soft delete must preserve the row"
        );
        drop(db);
        drop(sys_guard);

        let collections = list_collections(State(system.clone())).await.0;
        assert!(
            collections
                .iter()
                .all(|stat| !is_internal_project(&stat.name)),
            "internal bucket leaked into /collections"
        );

        let stats = stats(State(system.clone())).await.0;
        assert_eq!(stats.total_chunks, 1);
        assert!(
            stats
                .collections
                .iter()
                .all(|entry| !is_internal_project(&entry.name)),
            "internal bucket leaked into /stats"
        );

        let dashboard = viewer_dashboard(State(system.clone())).await.0;
        assert!(
            !dashboard.contains(doomed_text),
            "dashboard still shows the deleted body"
        );
        assert!(
            !dashboard.contains("_trash") && !dashboard.contains("condemned"),
            "dashboard still shows the trash bucket"
        );
        assert!(
            dashboard.contains("survivor"),
            "live collection went missing"
        );
        assert!(!keep.is_empty());

        // The trash bucket is not browsable, even by direct URL.
        let trash = collection_detail(
            State(system),
            Path("_trash".to_string()),
            Query(HashMap::from([("format".to_string(), "html".to_string())])),
        )
        .await;
        assert_eq!(trash.status(), axum::http::StatusCode::NOT_FOUND);
    }

    /// `/context` feeds a prompt. An unscoped pack would inject another
    /// project's text, so a missing project is a hard 400, exactly like
    /// `/search`.
    #[tokio::test]
    async fn context_requires_an_explicit_project() {
        let (_tmp, system) = build_system().await;
        let secret = "unrelated project holds the pineapple deployment token";
        store(&system, "other-team", secret).await;

        for body in [
            serde_json::json!({"query": "pineapple deployment token"}),
            serde_json::json!({"query": "pineapple deployment token", "project": "  "}),
        ] {
            let request: ContextRequest = serde_json::from_value(body).expect("deserialize");
            let response = context_pack(State(system.clone()), Json(request)).await;
            assert_eq!(
                response.status(),
                axum::http::StatusCode::BAD_REQUEST,
                "missing project must not fall back to cross-project recall"
            );
            let text = body_text(response).await;
            assert!(
                !text.contains("pineapple"),
                "400 response leaked cross-project text: {text}"
            );
        }

        // An explicit, unrelated scope also returns nothing from other-team.
        // The query itself is echoed back, so assert on the retrieved bodies.
        let request: ContextRequest = serde_json::from_value(
            serde_json::json!({"query": "pineapple deployment token", "project": "mine"}),
        )
        .expect("deserialize");
        let scoped = body_text(context_pack(State(system), Json(request)).await).await;
        assert!(
            !scoped.contains("unrelated project holds"),
            "project-scoped context leaked another project: {scoped}"
        );
        assert!(
            scoped.contains(r#""memories":[]"#),
            "expected an empty memory list: {scoped}"
        );
    }

    fn dictionary_keys(language: &str) -> std::collections::HashSet<String> {
        let opener = format!("\n {language}: {{\n");
        let start = BASE_HTML
            .find(&opener)
            .unwrap_or_else(|| panic!("no {language} dictionary"))
            + opener.len();
        let end = start
            + BASE_HTML[start..]
                .find("\n }")
                .expect("unterminated dictionary");
        BASE_HTML[start..end]
            .lines()
            .filter_map(|line| {
                let line = line.trim();
                let rest = line.strip_prefix('\'')?;
                let (key, _) = rest.split_once('\'')?;
                Some(key.to_string())
            })
            .collect()
    }

    /// Every key the markup asks for must exist in both dictionaries, or one
    /// locale silently renders the other locale's fallback text.
    #[tokio::test]
    async fn every_dom_i18n_key_exists_in_both_dictionaries() {
        let ko = dictionary_keys("ko");
        let en = dictionary_keys("en");
        assert!(!ko.is_empty() && !en.is_empty());

        // Scan the whole module source, not just BASE_HTML: most keys live in
        // the per-page content built inside the viewer handlers.
        let mut used = std::collections::HashSet::new();
        for needle in [
            concat!("data-i18n", "=\""),
            concat!("data-i18n-placeholder", "=\""),
        ] {
            let mut rest = SOURCE;
            while let Some(offset) = rest.find(needle) {
                rest = &rest[offset + needle.len()..];
                let (key, tail) = rest.split_once('"').expect("unterminated attribute");
                used.insert(key.to_string());
                rest = tail;
            }
        }
        assert!(used.len() > 20, "scan found suspiciously few keys");

        let missing_ko: Vec<_> = used.difference(&ko).cloned().collect();
        let missing_en: Vec<_> = used.difference(&en).cloned().collect();
        assert!(
            missing_ko.is_empty(),
            "keys missing from ko: {missing_ko:?}"
        );
        assert!(
            missing_en.is_empty(),
            "keys missing from en: {missing_en:?}"
        );

        // The other direction keeps the dictionaries from accumulating dead keys.
        let unused_ko: Vec<_> = ko.difference(&used).cloned().collect();
        let unused_en: Vec<_> = en.difference(&used).cloned().collect();
        assert!(unused_ko.is_empty(), "unused ko keys: {unused_ko:?}");
        assert!(unused_en.is_empty(), "unused en keys: {unused_en:?}");
    }

    /// A value substituted into the shell must not be re-scanned for
    /// placeholders, or a collection literally named `__CONTENT__` would swap
    /// the page body into the title.
    #[test]
    fn page_shell_substitutes_each_placeholder_once() {
        let page = render_page("__CONTENT__", Nav::Dashboard, "<p>body</p>");
        assert!(page.contains("<title>__CONTENT__ | Memnest</title>"));
        assert_eq!(page.matches("<p>body</p>").count(), 1);
        assert!(!page.contains("__TITLE__") && !page.contains("__ACTIVE_"));
    }

    #[test]
    fn url_encode_keeps_names_inside_the_path_segment() {
        assert_eq!(url_encode("playbook"), "playbook");
        assert_eq!(url_encode("a/b?c=d"), "a%2Fb%3Fc%3Dd");
        assert_eq!(url_encode(r#""><script>"#), "%22%3E%3Cscript%3E");
        assert_eq!(url_encode("한글"), "%ED%95%9C%EA%B8%80");
    }
}

#[cfg(test)]
mod transcript_tests {
    use super::*;

    fn metadata(event_id: &str, sequence: i64) -> Metadata {
        Metadata {
            chunk_type: ChunkType::AutoLog,
            session_id: "session-1".into(),
            source: Some("pi.transcript".into()),
            adapter: Some("pi".into()),
            event_id: Some(event_id.into()),
            sequence: Some(sequence),
            total: Some(2),
            ..Default::default()
        }
    }

    #[test]
    fn transcript_retry_id_is_stable_but_repeated_events_and_parts_are_distinct() {
        let first = transcript_chunk_id("project", &metadata("event-1", 1)).unwrap();
        assert_eq!(
            first,
            transcript_chunk_id("project", &metadata("event-1", 1)).unwrap()
        );
        assert_ne!(
            first,
            transcript_chunk_id("project", &metadata("event-1", 2)).unwrap()
        );
        assert_ne!(
            first,
            transcript_chunk_id("project", &metadata("event-2", 1)).unwrap()
        );
    }

    #[test]
    fn only_transcript_autolog_uses_event_identity_dedup() {
        let mut manual = metadata("event-1", 1);
        manual.chunk_type = ChunkType::Manual;
        assert!(transcript_chunk_id("project", &manual).is_none());
    }

    #[tokio::test]
    async fn transcript_retry_repairs_indexes_after_partial_store_failure() {
        let (_tmp, system) = super::test_support::build_system().await;
        let metadata = metadata("partial-event", 1);
        let id = transcript_chunk_id("repair", &metadata).unwrap();
        let document = "User said: searchable partial repair token";
        let embedding = {
            let sys = system.read().await;
            sys.embedder.encode_document(document).unwrap()
        };
        let now = chrono::Utc::now();
        let chunk = MemoryChunk {
            id: id.clone(),
            project: "repair".into(),
            document: document.into(),
            embedding: Some(embedding.clone()),
            metadata: metadata.clone(),
            created_at: now,
            updated_at: now,
        };
        {
            let sys = system.read().await;
            sys.db.write().await.insert_chunk(&chunk).unwrap();
            assert!(!sys.vector_index.read().await.contains(&id));
            assert!(
                sys.text_search("searchable partial repair", 5)
                    .await
                    .unwrap()
                    .is_empty()
            );
        }

        let response = add_impl(
            system.clone(),
            AddRequest {
                text: document.into(),
                project: "repair".into(),
                metadata: Some(metadata),
                sensitive: None,
            },
        )
        .await;
        assert_eq!(
            response.get("status").map(String::as_str),
            Some("deduplicated")
        );

        let sys = system.read().await;
        assert!(
            sys.text_search("searchable partial repair", 5)
                .await
                .unwrap()
                .iter()
                .any(|(result_id, _)| result_id == &id)
        );
        assert!(
            sys.vector_index
                .read()
                .await
                .search(&embedding, 5)
                .unwrap()
                .iter()
                .any(|(result_id, _)| result_id == &id)
        );
    }
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
