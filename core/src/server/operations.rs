use crate::MemorySystem;
use crate::models::{Metadata, RecallEvent, is_internal_project};
use crate::redaction::redact_text;
use crate::workspace::{SearchScope, identity as workspace_identity};
use serde::Serialize;
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
    pub fn internal(_message: impl Into<String>) -> Self {
        Self {
            kind: ErrorKind::Internal,
            message: "internal operation failed".to_string(),
        }
    }
}
impl std::fmt::Display for OperationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for OperationError {}

pub fn validate_write_project(project: &str) -> Result<(), OperationError> {
    if is_internal_project(project) {
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
    if let Some(metadata) = &input.metadata {
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
        let sys = system.read().await;
        sys.db
            .write()
            .await
            .register_workspace_scope(&workspace)
            .map_err(|error| OperationError::internal(error.to_string()))?;
        let metadata = input.metadata.get_or_insert_with(Metadata::default);
        if metadata.cwd.is_none() {
            metadata.cwd = Some(cwd.to_string());
        }
        workspace.id
    } else {
        "default".to_string()
    };
    validate_write_project(&project)?;
    if input.sensitive || input.metadata.as_ref().is_some_and(|m| m.sensitive) {
        return Err(OperationError::bad(
            "sensitive memory is not supported; use secret_set",
        ));
    }
    let map = api::add_impl(
        system,
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
    pub category: Option<String>,
    pub exclude_reserved: bool,
    pub adapter: String,
}
#[derive(Serialize)]
pub struct SearchOutput {
    pub results: Vec<api::SearchResultItem>,
    pub total: usize,
    pub elapsed_ms: u128,
    pub recall_id: String,
}

pub(crate) async fn resolve_search_scope(
    system: Arc<RwLock<MemorySystem>>,
    project: &str,
    cwd: Option<&str>,
) -> Result<SearchScope, OperationError> {
    if !project.trim().is_empty() {
        return Ok(SearchScope::explicit(project.trim()));
    }
    let cwd = cwd.ok_or_else(|| {
        OperationError::bad(
            "project or cwd is required; use project=all explicitly for cross-project search",
        )
    })?;
    let workspace =
        workspace_identity(cwd).map_err(|error| OperationError::bad(error.to_string()))?;
    let allowed = {
        let sys = system.read().await;
        let db = sys.db.write().await;
        db.register_workspace_scope(&workspace)
            .map_err(|error| OperationError::internal(error.to_string()))?
    };
    Ok(SearchScope::Projects {
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
        input.recent_first,
        input.exclude_reserved,
        input.category,
    )
    .await;
    let elapsed_ms = started.elapsed().as_millis();
    let event = RecallEvent {
        id: format!("recall_{}", uuid::Uuid::new_v4().simple()),
        query: redact_text(&input.query),
        project: project.clone(),
        result_ids: items.iter().map(|item| item.id.clone()).collect(),
        duration_ms: elapsed_ms.min(i64::MAX as u128) as i64,
        adapter: input.adapter,
        outcome: "pending".into(),
        created_at: chrono::Utc::now(),
    };
    let sys = system.read().await;
    sys.db
        .write()
        .await
        .insert_recall_event(&event)
        .map_err(|e| OperationError::internal(e.to_string()))?;
    Ok(SearchOutput {
        total: items.len(),
        results: items,
        elapsed_ms,
        recall_id: event.id,
    })
}

pub async fn get(system: Arc<RwLock<MemorySystem>>, id: &str) -> Result<Value, OperationError> {
    if id.trim().is_empty() {
        return Err(OperationError::bad("id is required"));
    }
    let sys = system.read().await;
    let db = sys.db.read().await;
    let c = db
        .get_chunk(id)
        .map_err(|e| OperationError::internal(e.to_string()))?
        .ok_or_else(|| OperationError::not_found(format!("chunk not found: {id}")))?;
    let redacted = redact_text(&c.document);
    Ok(
        json!({"id":c.id,"project":c.project,"document":redacted.chars().take(8000).collect::<String>(),"doc_len":redacted.chars().count(),"timestamp":c.created_at.to_rfc3339(),"chunk_type":format!("{:?}",c.metadata.chunk_type),"importance":format!("{:?}",c.metadata.importance),"category":format!("{:?}",c.metadata.category)}),
    )
}

pub async fn update(
    system: Arc<RwLock<MemorySystem>>,
    req: api::UpdateRequest,
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
    if let Some(project) = req.project.as_deref() {
        validate_write_project(project)?;
    }
    if let Some(metadata) = &req.metadata {
        validate_truth_fields(
            metadata.confidence,
            metadata.supersedes.as_deref(),
            metadata.verified_at.as_deref(),
        )?;
    }
    let map = api::update_impl(system, req).await;
    match map.get("status").and_then(Value::as_str) {
        Some("ok") => Ok(map),
        Some("not_found") => Err(OperationError::not_found("memory not found")),
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
    Ok(json!({"deleted": deleted, "not_found": not_found}))
}

pub async fn feedback(
    system: Arc<RwLock<MemorySystem>>,
    recall_id: &str,
    memory_id: Option<&str>,
    outcome: &str,
    note: Option<&str>,
) -> Result<Value, OperationError> {
    if recall_id.trim().is_empty() {
        return Err(OperationError::bad("recall_id is required"));
    }
    let outcome = outcome.trim().to_lowercase();
    if !matches!(outcome.as_str(), "helpful" | "harmful" | "ignored") {
        return Err(OperationError::bad(
            "outcome must be helpful, harmful, or ignored",
        ));
    }
    let sys = system.read().await;
    let ids = sys
        .db
        .write()
        .await
        .set_recall_feedback(
            recall_id,
            memory_id,
            &outcome,
            note.map(redact_text).as_deref(),
        )
        .map_err(|error| {
            let message = error.to_string();
            if message.contains("was not returned by recall") {
                OperationError::conflict("memory_id was not returned by this recall")
            } else {
                OperationError::internal(message)
            }
        })?
        .ok_or_else(|| OperationError::not_found("recall event not found"))?;
    Ok(json!({"status":"ok","recall_id":recall_id,"outcome":outcome,"memory_ids":ids}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::INTERNAL_PROJECTS;

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
