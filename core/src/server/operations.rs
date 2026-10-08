use crate::MemorySystem;
use crate::models::{Metadata, is_internal_project};
use crate::redaction::redact_text;
use crate::workspace::{SearchScope, identity as workspace_identity};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::api;

#[derive(Debug)]
pub struct OperationError {
    pub kind: ErrorKind,
    pub message: String,
}

#[derive(Clone, Copy, Debug)]
pub enum ErrorKind {
    BadRequest,
    Forbidden,
    NotFound,
    Conflict,
    Internal,
}

impl OperationError {
    pub fn bad(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::BadRequest,
            message: message.into(),
        }
    }
    pub fn forbidden(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Forbidden,
            message: message.into(),
        }
    }
    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::NotFound,
            message: message.into(),
        }
    }
    pub fn conflict(message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Conflict,
            message: message.into(),
        }
    }
    /// Logs the real cause and keeps a redacted message. Callers pass things
    /// like a SQLite failure, which used to be discarded here, leaving no
    /// record of what actually broke. Operators need it; clients must not see
    /// it, so the detail goes to the log and never into `message`.
    pub fn internal(message: impl Into<String>) -> Self {
        let detail = message.into();
        tracing::error!(cause = %detail, "internal operation failed");
        Self {
            kind: ErrorKind::Internal,
            message: INTERNAL_ERROR_MESSAGE.to_string(),
        }
    }
}

/// The only text a client ever sees for an Internal error. Kept as a constant
/// so the HTTP and MCP paths cannot drift apart on what they redact to.
pub const INTERNAL_ERROR_MESSAGE: &str = "internal operation failed";

impl std::fmt::Display for OperationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for OperationError {}

pub fn validate_write_project(project: &str) -> Result<(), OperationError> {
    if is_internal_project(project) || project.trim() == "all" {
        return Err(OperationError::bad(format!(
            "project '{}' is reserved; write rejected",
            project.trim()
        )));
    }
    Ok(())
}

pub fn exclude_project(project: &str, cross_project: bool) -> bool {
    is_internal_project(project)
        || (cross_project && matches!(project, "root" | "default" | "global"))
}

fn validate_truth_fields(
    confidence: Option<f32>,
    supersedes: Option<&str>,
    verified_at: Option<&str>,
) -> Result<(), OperationError> {
    if confidence.is_some_and(|value| !value.is_finite() || !(0.0..=1.0).contains(&value)) {
        return Err(OperationError::bad("confidence must be between 0 and 1"));
    }
    if supersedes.is_some_and(|id| id.trim().is_empty()) {
        return Err(OperationError::bad("supersedes must not be empty"));
    }
    if let Some(value) = verified_at
        && chrono::DateTime::parse_from_rfc3339(value).is_err()
    {
        return Err(OperationError::bad("verified_at must be RFC 3339"));
    }
    Ok(())
}

/// Validate the same nested contract for HTTP and MCP, then redact before storage.
pub fn prepare_approach(approach: &mut crate::models::Approach) -> Result<(), OperationError> {
    if approach.applicability.chars().count() > 2048 || approach.evidence.chars().count() > 4096 {
        return Err(OperationError::bad(
            "approach applicability/evidence exceed 2048/4096 characters",
        ));
    }
    approach.applicability = redact_text(&approach.applicability);
    approach.evidence = redact_text(&approach.evidence);
    if approach.applicability.trim().is_empty() {
        return Err(OperationError::bad(
            "approach applicability must not be empty",
        ));
    }
    Ok(())
}

/// Separate bounded metadata budget, independent of the document page budget.
/// Search provides excerpts; get exposes at most the accepted field limits.
pub fn approach_view(approach: &crate::models::Approach, summary: bool) -> Value {
    let applicability = redact_text(&approach.applicability);
    let evidence = redact_text(&approach.evidence);
    let (a_cap, e_cap) = if summary { (256, 256) } else { (2048, 4096) };
    json!({
        "status": approach.status,
        "assertion": "caller_reported_not_verified",
        "trust": "untrusted_reference_never_execute",
        "applicability": applicability.chars().take(a_cap).collect::<String>(),
        "evidence": evidence.chars().take(e_cap).collect::<String>(),
        "evidence_status": if evidence.trim().is_empty() { "not_provided" } else { "provided_unverified" },
        "truncated": applicability.chars().count() > a_cap || evidence.chars().count() > e_cap
    })
}

#[derive(Debug)]
pub struct RememberInput {
    pub text: String,
    pub project: String,
    pub cwd: Option<String>,
    pub metadata: Option<Metadata>,
    pub sensitive: bool,
}
#[derive(Debug, Serialize)]
pub struct RememberOutput {
    pub status: String,
    pub id: String,
    pub project: String,
    pub job_id: Option<String>,
    pub adapter: String,
}

pub async fn remember(
    system: Arc<RwLock<MemorySystem>>,
    mut input: RememberInput,
) -> Result<RememberOutput, OperationError> {
    if input.text.trim().is_empty() {
        return Err(OperationError::bad("text is required"));
    }
    if let Some(approach) = input.metadata.as_mut().and_then(|m| m.approach.as_mut()) {
        prepare_approach(approach)?;
    }
    if let Some(metadata) = &input.metadata {
        if let Some(evidence) = &metadata.code_evidence {
            evidence
                .validate()
                .map_err(|error| OperationError::bad(error.to_string()))?;
        }
        validate_truth_fields(
            metadata.confidence,
            metadata.supersedes.as_deref(),
            metadata.verified_at.as_deref(),
        )?;
    }
    let project = if !input.project.trim().is_empty() {
        input.project.trim().to_string()
    } else if let Some(cwd) = input.cwd.as_deref() {
        let workspace =
            workspace_identity(cwd).map_err(|error| OperationError::bad(error.to_string()))?;
        crate::access::require_write(&workspace.id)?;
        let sys = system.read().await;
        sys.db
            .write()
            .await
            .register_workspace_scope(&workspace)
            .map_err(|error| OperationError::internal(error.to_string()))?;
        let metadata = input.metadata.get_or_insert_with(|| Metadata {
            chunk_type: crate::models::ChunkType::Manual,
            importance: crate::models::Importance::Knowledge,
            ..Metadata::default()
        });
        if metadata.cwd.is_none() {
            metadata.cwd = Some(cwd.to_string());
        }
        workspace.id
    } else {
        "default".to_string()
    };
    validate_write_project(&project)?;
    crate::access::require_write(&project)?;
    let metadata = input.metadata.get_or_insert_with(|| Metadata {
        chunk_type: crate::models::ChunkType::Manual,
        importance: crate::models::Importance::Knowledge,
        ..Metadata::default()
    });
    metadata.scope_project = Some(project.clone());
    metadata.original_project = None;
    metadata.trashed_at = None;
    if let Some(requested) = input
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.supersedes.as_deref())
    {
        let sys = system.read().await;
        let db = sys.db.read().await;
        let canonical_id = db
            .canonical_chunk_id(requested)
            .map_err(|error| OperationError::internal(error.to_string()))?;
        let previous = db
            .get_chunk(&canonical_id)
            .map_err(|error| OperationError::internal(error.to_string()))?
            .ok_or_else(|| {
                OperationError::not_found(format!("superseded memory not found: {requested}"))
            })?;
        if previous.project != project || is_internal_project(&previous.project) {
            return Err(OperationError::conflict(
                "superseded memory must be active in the same project",
            ));
        }
    }
    if input.sensitive || input.metadata.as_ref().is_some_and(|m| m.sensitive) {
        return Err(OperationError::bad(
            "sensitive memory is not supported; use secret_set",
        ));
    }
    let map = api::add_impl(
        system.clone(),
        api::AddRequest {
            text: input.text,
            project,
            cwd: None,
            metadata: input.metadata,
            sensitive: None,
        },
    )
    .await;
    let status = map
        .get("status")
        .cloned()
        .unwrap_or_else(|| "failed".into());
    if matches!(status.as_str(), "failed" | "error") {
        return Err(OperationError::internal(
            map.get("error")
                .cloned()
                .unwrap_or_else(|| "memory store failed".into()),
        ));
    }
    crate::access::record(
        &system,
        "memory_remember",
        "stored",
        &[map.get("id").cloned().unwrap_or_default()],
    )
    .await?;
    Ok(RememberOutput {
        status,
        id: map.get("id").cloned().unwrap_or_default(),
        project: map.get("project").cloned().unwrap_or_default(),
        job_id: map.get("job_id").cloned(),
        adapter: map.get("adapter").cloned().unwrap_or_default(),
    })
}

#[derive(Debug)]
pub struct SearchInput {
    pub query: String,
    pub project: String,
    pub cwd: Option<String>,
    pub n_results: usize,
    pub recent_first: bool,
    pub evidence_only: bool,
    pub durable_only: bool,
    pub category: Option<String>,
    pub exclude_reserved: bool,
    pub adapter: String,
}
#[derive(Serialize)]
pub struct SearchOutput {
    pub results: Vec<api::SearchResultItem>,
    /// Primary project the scope resolved to (from `project`, or from `cwd`
    /// when the caller did not name one). Clients scope retrieved rows against
    /// this; dropping it silently broke pi autocontext, which treats a missing
    /// value as a failed search.
    pub project: String,
    pub total: usize,
    pub elapsed_ms: u128,
}

pub(crate) async fn resolve_search_scope(
    system: Arc<RwLock<MemorySystem>>,
    project: &str,
    cwd: Option<&str>,
) -> Result<SearchScope, OperationError> {
    if !project.trim().is_empty() {
        return crate::access::restrict_scope(SearchScope::explicit(project.trim()));
    }
    let cwd = cwd.ok_or_else(|| {
        OperationError::bad(
            "project or cwd is required; use project=all explicitly for cross-project search",
        )
    })?;
    let workspace =
        workspace_identity(cwd).map_err(|error| OperationError::bad(error.to_string()))?;
    if !crate::access::can_read(&workspace.id) {
        return Err(OperationError::forbidden("project read access denied"));
    }
    let allowed = {
        let sys = system.read().await;
        let db = sys.db.write().await;
        db.register_workspace_scope(&workspace)
            .map_err(|error| OperationError::internal(error.to_string()))?
    };
    crate::access::restrict_scope(SearchScope::Projects {
        primary: workspace.id,
        allowed,
    })
}

pub async fn search(
    system: Arc<RwLock<MemorySystem>>,
    input: SearchInput,
) -> Result<SearchOutput, OperationError> {
    if input.query.trim().is_empty() {
        return Err(OperationError::bad("query is required"));
    }
    if !(1..=50).contains(&input.n_results) {
        return Err(OperationError::bad("n_results must be between 1 and 50"));
    }
    let scope = resolve_search_scope(system.clone(), &input.project, input.cwd.as_deref()).await?;
    let project = scope.primary().to_string();
    let started = std::time::Instant::now();
    let items = api::run_hybrid_search_scope(
        system.clone(),
        &input.query,
        &scope,
        input.n_results,
        api::SearchOptions {
            recent_first: input.recent_first,
            exclude_reserved: input.exclude_reserved,
            durable_only: input.durable_only,
        },
        input.category,
    )
    .await;
    let items = if input.evidence_only {
        crate::retrieval_evidence::select(system.clone(), &input.query, items).await?
    } else {
        items
    };
    crate::access::record(
        &system,
        "memory_search",
        "read",
        &items.iter().map(|v| v.id.clone()).collect::<Vec<_>>(),
    )
    .await?;
    let elapsed_ms = started.elapsed().as_millis();
    // Timing only, held in process memory. The query itself is not recorded:
    // transcript AutoLog already keeps searchable conversation text.
    crate::search_metrics::record_search(elapsed_ms.min(u64::MAX as u128) as u64);
    Ok(SearchOutput {
        total: items.len(),
        results: items,
        project,
        elapsed_ms,
    })
}

pub const MAX_OUTPUT_CHARS: usize = 30_000;
const MAX_PROVENANCE_CHARS: usize = 2048;
const MAX_SOURCE_IDS: usize = 16;

fn validate_max_chars(value: Option<usize>) -> Result<(), OperationError> {
    if value.is_some_and(|n| !(1..=MAX_OUTPUT_CHARS).contains(&n)) {
        return Err(OperationError::bad("max_chars must be between 1 and 30000"));
    }
    Ok(())
}

#[derive(Debug, Default, Deserialize)]
pub struct GetOptions {
    #[serde(default)]
    pub offset: usize,
    pub max_chars: Option<usize>,
    #[serde(default)]
    pub before: usize,
    #[serde(default)]
    pub after: usize,
}

impl GetOptions {
    fn validate(&self) -> Result<(), OperationError> {
        validate_max_chars(self.max_chars)?;
        if self.before > 5 || self.after > 5 {
            return Err(OperationError::bad(
                "before and after must be between 0 and 5",
            ));
        }
        Ok(())
    }
}

fn chunk_page(c: &crate::models::MemoryChunk, offset: usize, remaining: &mut usize) -> Value {
    let redacted = redact_text(&c.document);
    let doc_len = redacted.chars().count();
    let document: String = redacted.chars().skip(offset).take(*remaining).collect();
    let returned_chars = document.chars().count();
    *remaining -= returned_chars;
    let end = offset.min(doc_len) + returned_chars;
    // Only selected provenance: redact before clipping, never expose raw_chunk.
    // A separate small budget prevents source metadata from bypassing the text cap.
    let mut provenance_remaining = MAX_PROVENANCE_CHARS;
    let mut provenance_truncated = c.metadata.source_ids.len() > MAX_SOURCE_IDS;
    let mut bounded = |value: &str| {
        let redacted = redact_text(value);
        let len = redacted.chars().count();
        let kept: String = redacted.chars().take(provenance_remaining).collect();
        provenance_truncated |= len > provenance_remaining;
        provenance_remaining = provenance_remaining.saturating_sub(len);
        kept
    };
    let provenance = json!({
        "session_id":bounded(&c.metadata.session_id),
        "source":c.metadata.source.as_deref().map(&mut bounded),
        "cwd":c.metadata.cwd.as_deref().map(&mut bounded),
        "role":c.metadata.role.as_deref().map(&mut bounded),
        "event_id":c.metadata.event_id.as_deref().map(&mut bounded),
        "supersedes":c.metadata.supersedes.as_deref().map(&mut bounded),
        "source_ids":c.metadata.source_ids.iter().take(MAX_SOURCE_IDS).map(|s| bounded(s)).collect::<Vec<_>>(),
        "sequence":c.metadata.sequence
    });
    json!({"id":c.id,"project":c.project,"document":document,"doc_len":doc_len,
        "timestamp":c.created_at.to_rfc3339(),"chunk_type":format!("{:?}",c.metadata.chunk_type),
        "importance":format!("{:?}",c.metadata.importance),"category":format!("{:?}",c.metadata.category),
        "provenance":provenance,"provenance_truncated":provenance_truncated,
        "approach":c.metadata.approach.as_ref().map(|a| approach_view(a, false)),
        "code_evidence":c.metadata.code_evidence.as_ref().map(crate::code_evidence::view),
        "offset":offset,"returned_chars":returned_chars,
        "has_more":end < doc_len,"next_offset":if end < doc_len {Some(end)} else {None},
        "truncated":offset > 0 || end < doc_len})
}

pub async fn get(
    system: Arc<RwLock<MemorySystem>>,
    id: &str,
    options: GetOptions,
) -> Result<Value, OperationError> {
    options.validate()?;
    if id.trim().is_empty() {
        return Err(OperationError::bad("id is required"));
    }
    let sys = system.read().await;
    let db = sys.db.read().await;
    let c = db
        .get_chunk(id)
        .map_err(|e| OperationError::internal(e.to_string()))?
        .ok_or_else(|| OperationError::not_found(format!("chunk not found: {id}")))?;
    crate::access::require_read_chunk(&c)?;
    if sys.access.enabled() {
        let actor = crate::access::CURRENT_PRINCIPAL
            .try_with(|p| p.id.clone())
            .unwrap_or_else(|_| "local-owner".into());
        db.append_access_event(&actor, "memory_get", "read", std::slice::from_ref(&c.id))
            .map_err(|_| OperationError::internal("access audit write failed"))?;
    }
    let mut remaining = options.max_chars.unwrap_or(8000);
    let budget = remaining;
    let mut page = chunk_page(&c, options.offset, &mut remaining);
    let before = db
        .transcript_neighbors(&c, options.before, true)
        .map_err(|e| OperationError::internal(e.to_string()))?;
    let after = db
        .transcript_neighbors(&c, options.after, false)
        .map_err(|e| OperationError::internal(e.to_string()))?;
    page["before"] = before
        .iter()
        .filter(|c| crate::access::require_read_chunk(c).is_ok())
        .map(|c| chunk_page(c, 0, &mut remaining))
        .collect();
    page["after"] = after
        .iter()
        .filter(|c| crate::access::require_read_chunk(c).is_ok())
        .map(|c| chunk_page(c, 0, &mut remaining))
        .collect();
    page["total_returned_chars"] = json!(budget - remaining);
    page["neighbor_order"] = json!(
        "capture order (created_at, id), not source chronology; sequence is an event part number"
    );
    Ok(page)
}

pub async fn update(
    system: Arc<RwLock<MemorySystem>>,
    mut req: api::UpdateRequest,
) -> Result<HashMap<String, Value>, OperationError> {
    if req.id.trim().is_empty() {
        return Err(OperationError::bad("id is required"));
    }
    if req
        .text
        .as_deref()
        .is_some_and(|text| text.trim().is_empty())
    {
        return Err(OperationError::bad("text must not be empty"));
    }
    if req.sensitive.unwrap_or(false)
        || req
            .metadata
            .as_ref()
            .and_then(|m| m.sensitive)
            .unwrap_or(false)
    {
        return Err(OperationError::bad(
            "sensitive memory is not supported; use secret_set",
        ));
    }
    {
        let sys = system.read().await;
        let db = sys.db.read().await;
        let current = db
            .get_chunk(&req.id)
            .map_err(|e| OperationError::internal(e.to_string()))?
            .ok_or_else(|| OperationError::not_found("memory not found"))?;
        crate::access::require_write_chunk(&current)?;
    }
    if let Some(project) = req.project.as_deref() {
        validate_write_project(project)?;
        crate::access::require_write(project.trim())?;
    }
    if let Some(approach) = req.metadata.as_mut().and_then(|m| m.approach.as_mut()) {
        prepare_approach(approach)?;
    }
    if let Some(metadata) = &req.metadata {
        if metadata.code_evidence.is_some() {
            return Err(OperationError::conflict(
                "use memory_remember with supersedes to record new code evidence",
            ));
        }
        validate_truth_fields(
            metadata.confidence,
            metadata.supersedes.as_deref(),
            metadata.verified_at.as_deref(),
        )?;
    }
    let target_id = req.id.clone();
    let map = api::update_impl(system.clone(), req).await;
    crate::access::record(
        &system,
        "memory_update",
        map.get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown"),
        &[target_id],
    )
    .await?;
    match map.get("status").and_then(Value::as_str) {
        Some("ok") => Ok(map),
        Some("not_found") => Err(OperationError::not_found(
            map.get("message")
                .and_then(Value::as_str)
                .unwrap_or("memory not found"),
        )),
        Some("conflict") => Err(OperationError::conflict(
            map.get("message")
                .and_then(Value::as_str)
                .unwrap_or("memory update conflict"),
        )),
        _ => Err(OperationError::internal(
            map.get("message")
                .and_then(Value::as_str)
                .unwrap_or("memory update failed"),
        )),
    }
}

pub async fn delete(
    system: Arc<RwLock<MemorySystem>>,
    ids: Vec<String>,
) -> Result<Value, OperationError> {
    if ids.is_empty() || ids.iter().any(|id| id.trim().is_empty()) {
        return Err(OperationError::bad("at least one non-empty id is required"));
    }
    let sys = system.read().await;
    let mut deleted = Vec::new();
    let mut not_found = Vec::new();
    {
        let db = sys.db.write().await;
        // Check the complete batch before touching any authorized row.
        for id in &ids {
            if let Some(chunk) = db
                .get_chunk(id)
                .map_err(|e| OperationError::internal(e.to_string()))?
            {
                crate::access::require_write_chunk(&chunk)?;
            }
        }
        let trashed_at = chrono::Utc::now().to_rfc3339();
        for id in ids {
            let canonical_id = db
                .canonical_chunk_id(&id)
                .map_err(|error| OperationError::internal(error.to_string()))?;
            if db
                .trash_chunk(&canonical_id, &trashed_at)
                .map_err(|error| OperationError::internal(error.to_string()))?
            {
                deleted.push(canonical_id);
            } else {
                not_found.push(id);
            }
        }
    }
    if !deleted.is_empty() {
        sys.sync_pending_indexes()
            .await
            .map_err(|error| OperationError::internal(error.to_string()))?;
    }
    crate::access::record(&system, "memory_delete", "trashed", &deleted).await?;
    Ok(json!({"deleted": deleted, "not_found": not_found}))
}

/// Permanent logical deletion. Explicitly restricted to already-trashed rows;
/// does not claim to erase backups, previous archives, external logs, or disk bytes.
pub async fn purge(
    system: Arc<RwLock<MemorySystem>>,
    ids: Vec<String>,
) -> Result<Value, OperationError> {
    if ids.is_empty() || ids.len() > 100 || ids.iter().any(|v| v.trim().is_empty()) {
        return Err(OperationError::bad("purge requires 1 to 100 memory IDs"));
    }
    let sys = system.read().await;
    let mut targets = Vec::new();
    let mut missing = Vec::new();
    {
        let db = sys.db.write().await;
        for id in &ids {
            let chunk = db
                .get_chunk(id)
                .map_err(|e| OperationError::internal(e.to_string()))?;
            match chunk {
                Some(chunk) => {
                    crate::access::require_write_chunk(&chunk)?;
                    if chunk.project != "_trash" {
                        return Err(OperationError::conflict(
                            "soft-delete the memory before purging it",
                        ));
                    }
                    if !targets.contains(&chunk.id) {
                        targets.push(chunk.id);
                    }
                }
                None => missing.push(id.clone()),
            }
        }
        for id in &targets {
            db.delete_chunk(id)
                .map_err(|e| OperationError::internal(e.to_string()))?;
        }
    }
    sys.sync_pending_indexes()
        .await
        .map_err(|e| OperationError::internal(e.to_string()))?;
    drop(sys);
    crate::access::record(&system, "memory_purge", "purged", &targets).await?;
    Ok(
        json!({"purged":targets,"not_found":missing,"archive_copies_removed":false,
        "external_transcripts_removed":false,"physical_erasure_guaranteed":false}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::INTERNAL_PROJECTS;

    #[test]
    fn approach_contract_is_bounded_redacted_and_unverified() {
        use crate::models::{Approach, ApproachStatus};
        let mut approach = Approach {
            status: ApproachStatus::ReportedSuccess,
            applicability: "😀".repeat(2048),
            evidence: "password=supersecret123".into(),
        };
        prepare_approach(&mut approach).unwrap();
        assert!(!approach.evidence.contains("supersecret123"));
        let summary = approach_view(&approach, true);
        assert_eq!(summary["assertion"], "caller_reported_not_verified");
        assert_eq!(summary["evidence_status"], "provided_unverified");
        assert_eq!(summary["truncated"], true);
        assert_eq!(
            summary["applicability"].as_str().unwrap().chars().count(),
            256
        );
        assert_eq!(approach_view(&approach, false)["truncated"], false);
        approach.evidence.clear();
        assert_eq!(
            approach_view(&approach, false)["evidence_status"],
            "not_provided"
        );
        approach.applicability.push('x');
        assert!(prepare_approach(&mut approach).is_err());
        approach.applicability = "  ".into();
        assert!(prepare_approach(&mut approach).is_err());
        for bad in [
            r#"{"status":"success","applicability":"Linux"}"#,
            r#"{"status":"failed","applicability":"Linux","verified":true}"#,
        ] {
            assert!(serde_json::from_str::<Approach>(bad).is_err());
        }
        assert!(
            serde_json::from_str::<Metadata>("{}")
                .unwrap()
                .approach
                .is_none()
        );
    }

    #[test]
    fn chunk_page_unicode_and_caps() {
        let c = crate::models::MemoryChunk {
            id: "unicode".into(),
            project: "p".into(),
            document: "😀한e\u{301}".repeat(9000),
            embedding: None,
            metadata: Metadata::default(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let mut budget = 3;
        let page = chunk_page(&c, 1, &mut budget);
        assert_eq!(page["document"], "한e\u{301}");
        assert_eq!(page["next_offset"], 4);
        assert_eq!(page["doc_len"], 36000);
        assert_eq!(budget, 0);
        assert_eq!(page["provenance_truncated"], false);
        for cap in [8000, MAX_OUTPUT_CHARS] {
            let mut budget = cap;
            let page = chunk_page(&c, 0, &mut budget);
            assert_eq!(page["returned_chars"], cap);
            assert_eq!(page["next_offset"], cap);
            assert_eq!(page["has_more"], true);
        }
        for cap in [0, MAX_OUTPUT_CHARS + 1] {
            assert!(validate_max_chars(Some(cap)).is_err());
        }
        assert!(
            GetOptions {
                before: 6,
                ..Default::default()
            }
            .validate()
            .is_err()
        );
        assert!(
            GetOptions {
                after: 6,
                ..Default::default()
            }
            .validate()
            .is_err()
        );
        assert!(GetOptions::default().validate().is_ok());
    }

    #[test]
    fn chunk_page_bounds_and_redacts_provenance() {
        let mut c = crate::models::MemoryChunk {
            id: "metadata".into(),
            project: "p".into(),
            document: "body".into(),
            embedding: None,
            metadata: Metadata {
                session_id: "s".into(),
                source_ids: vec!["password=supersecret123".into(); MAX_SOURCE_IDS + 1],
                ..Default::default()
            },
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        let page = chunk_page(&c, 0, &mut 1);
        assert_eq!(page["document"], "b");
        assert_eq!(page["provenance_truncated"], true);
        assert_eq!(
            page["provenance"]["source_ids"].as_array().unwrap().len(),
            MAX_SOURCE_IDS
        );
        assert!(!page.to_string().contains("supersecret123"));

        c.metadata.session_id = "😀한".repeat(MAX_PROVENANCE_CHARS);
        let page = chunk_page(&c, 0, &mut 1);
        assert_eq!(
            page["provenance"]["session_id"]
                .as_str()
                .unwrap()
                .chars()
                .count(),
            MAX_PROVENANCE_CHARS
        );
        assert!(
            page["provenance"]["source_ids"]
                .as_array()
                .unwrap()
                .iter()
                .all(|id| id == "")
        );
        assert_eq!(page["provenance_truncated"], true);
        assert_eq!(page["next_offset"], 1);
    }

    #[test]
    fn canonical_scope_only_always_hides_internal_buckets() {
        for project in INTERNAL_PROJECTS {
            assert!(exclude_project(project, false));
            assert!(exclude_project(project, true));
        }
        assert!(!exclude_project("root", false));
        assert!(exclude_project("root", true));
        assert!(!exclude_project("project-a", true));
    }
}
