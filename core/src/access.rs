//! Optional named principals with project-scoped bearer tokens.
//! Protects API clients, not an operator who can read the local data directory.
use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::sync::Arc;

use anyhow::{Result, ensure};
use axum::extract::{MatchedPath, Request, State};
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::Response;
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::sync::{Mutex, RwLock};

use crate::MemorySystem;
use crate::models::{MemoryChunk, is_internal_project};
use crate::server::operations::OperationError;
use crate::workspace::SearchScope;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Principal {
    pub id: String,
    token_sha256: String,
    #[serde(default)]
    pub admin: bool,
    #[serde(default)]
    pub read_projects: Vec<String>,
    #[serde(default)]
    pub write_projects: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Policy {
    principals: Vec<Principal>,
    /// Maximum age from the stored creation timestamp. Overrides pinned/type exemptions.
    #[serde(default)]
    retention_days: HashMap<String, i64>,
}

#[derive(Default)]
pub struct AccessControl {
    principals: Vec<Principal>,
    pub retention_days: HashMap<String, i64>,
    // ponytail: serialize policy-enabled HTTP requests to keep permission checks
    // and mutations in one boundary; replace with transactional row guards when
    // measured team throughput needs concurrency. Legacy local mode is unchanged.
    pub request_lock: Mutex<()>,
}

tokio::task_local! { pub static CURRENT_PRINCIPAL: Principal; }

fn valid_project(value: &str) -> bool {
    !value.trim().is_empty()
        && value.len() <= 256
        && value.trim() == value
        && !is_internal_project(value)
        && value != "all"
        && !value.chars().any(char::is_control)
        && crate::redaction::redact_text(value) == value
}

impl AccessControl {
    pub fn load() -> Result<Self> {
        let Some(path) = std::env::var_os("MEMNEST_ACCESS_POLICY") else {
            return Ok(Self::default());
        };
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(512 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 512 * 1024, "access policy exceeds 512 KiB");
        Self::from_json(&bytes)
    }

    fn from_json(bytes: &[u8]) -> Result<Self> {
        let policy: Policy = serde_json::from_slice(bytes)
            .map_err(|_| anyhow::anyhow!("invalid access policy JSON"))?;
        ensure!(
            !policy.principals.is_empty() && policy.principals.len() <= 128,
            "access policy requires 1 to 128 principals"
        );
        ensure!(
            policy.principals.iter().any(|p| p.admin),
            "access policy requires an administrator"
        );
        let mut ids = HashSet::new();
        let mut hashes = HashSet::new();
        for p in &policy.principals {
            ensure!(
                !p.id.is_empty()
                    && p.id.len() <= 64
                    && p.id
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.')),
                "invalid principal ID"
            );
            ensure!(
                p.token_sha256.len() == 64
                    && p.token_sha256
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
                "invalid principal token digest"
            );
            ensure!(
                ids.insert(&p.id) && hashes.insert(&p.token_sha256),
                "duplicate principal ID or token digest"
            );
            ensure!(
                p.read_projects.len() <= 256
                    && p.write_projects.len() <= 256
                    && p.read_projects
                        .iter()
                        .chain(&p.write_projects)
                        .all(|v| valid_project(v)),
                "invalid project grants"
            );
            ensure!(
                p.admin || p.write_projects.iter().all(|v| p.read_projects.contains(v)),
                "write grants require matching read grants"
            );
        }
        ensure!(
            policy.retention_days.len() <= 256
                && policy
                    .retention_days
                    .iter()
                    .all(|(p, d)| valid_project(p) && (1..=3650).contains(d)),
            "invalid project retention policy"
        );
        Ok(Self {
            principals: policy.principals,
            retention_days: policy.retention_days,
            ..Self::default()
        })
    }

    pub fn enabled(&self) -> bool {
        !self.principals.is_empty()
    }

    pub fn authenticate(&self, header: &str) -> Option<Principal> {
        let token = header.strip_prefix("Bearer ")?;
        if token.len() < 32 || token.len() > 4096 {
            return None;
        }
        let digest = format!("{:x}", Sha256::digest(token.as_bytes()));
        self.principals
            .iter()
            .find(|p| {
                p.token_sha256
                    .bytes()
                    .zip(digest.bytes())
                    .fold(0u8, |v, (a, b)| v | (a ^ b))
                    == 0
            })
            .cloned()
    }
}

pub fn is_admin() -> bool {
    // Stdio/library callers own the local store and retain the original trusted-owner contract.
    CURRENT_PRINCIPAL.try_with(|p| p.admin).unwrap_or(true)
}

pub fn require_admin() -> Result<(), OperationError> {
    if is_admin() {
        Ok(())
    } else {
        Err(OperationError::forbidden("administrator access required"))
    }
}

pub fn can_read(project: &str) -> bool {
    CURRENT_PRINCIPAL
        .try_with(|p| p.admin || p.read_projects.iter().any(|v| v == project))
        .unwrap_or(true)
}

pub fn require_write(project: &str) -> Result<(), OperationError> {
    if CURRENT_PRINCIPAL
        .try_with(|p| p.admin || p.write_projects.iter().any(|v| v == project))
        .unwrap_or(true)
    {
        Ok(())
    } else {
        Err(OperationError::forbidden("project write access denied"))
    }
}

fn scope_project(chunk: &MemoryChunk) -> Option<&str> {
    if !is_internal_project(&chunk.project) {
        return Some(&chunk.project);
    }
    // Legacy hidden rows without server-written scope are administrator-only.
    chunk
        .metadata
        .scope_project
        .as_deref()
        .filter(|p| valid_project(p))
}

pub fn require_read_chunk(chunk: &MemoryChunk) -> Result<(), OperationError> {
    if is_admin() || scope_project(chunk).is_some_and(can_read) {
        Ok(())
    } else {
        Err(OperationError::not_found("memory not found"))
    }
}

pub fn require_write_chunk(chunk: &MemoryChunk) -> Result<(), OperationError> {
    if is_admin() {
        return Ok(());
    }
    let project =
        scope_project(chunk).ok_or_else(|| OperationError::not_found("memory not found"))?;
    require_read_chunk(chunk)?;
    require_write(project)
}

pub fn restrict_scope(scope: SearchScope) -> Result<SearchScope, OperationError> {
    if is_admin() {
        return Ok(scope);
    }
    let grants = CURRENT_PRINCIPAL.with(|p| p.read_projects.clone());
    if grants.is_empty() {
        return Err(OperationError::forbidden("no project read grants"));
    }
    match scope {
        SearchScope::All => Ok(SearchScope::Projects {
            primary: "all".into(),
            allowed: grants,
        }),
        SearchScope::Projects { primary, allowed } => {
            if !can_read(&primary) {
                return Err(OperationError::forbidden("project read access denied"));
            }
            Ok(SearchScope::Projects {
                primary,
                allowed: allowed.into_iter().filter(|v| grants.contains(v)).collect(),
            })
        }
    }
}

fn operation(request: &Request) -> String {
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str())
        .unwrap_or("unknown");
    let method = match request.method().as_str() {
        "GET" | "POST" | "DELETE" | "PUT" | "PATCH" | "HEAD" | "OPTIONS" => {
            request.method().as_str()
        }
        _ => "OTHER",
    };
    format!("{method} {route}")
}

pub async fn record(
    system: &Arc<RwLock<MemorySystem>>,
    action: &str,
    outcome: &str,
    ids: &[String],
) -> Result<(), OperationError> {
    let sys = system.read().await;
    if !sys.access.enabled() {
        return Ok(());
    }
    let actor = CURRENT_PRINCIPAL
        .try_with(|p| p.id.clone())
        .unwrap_or_else(|_| "local-owner".into());
    sys.db
        .read()
        .await
        .append_access_event(&actor, action, outcome, ids)
        .map_err(|_| OperationError::internal("access audit write failed"))
}

pub async fn middleware(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let access = system.read().await.access.clone();
    let header = request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !access.enabled() {
        if let Some(token) = crate::server::auth_token() {
            let expected = format!("Bearer {token}");
            if header != expected {
                return Err(StatusCode::UNAUTHORIZED);
            }
        }
        return Ok(next.run(request).await);
    }
    let Some(principal) = access.authenticate(header) else {
        // No credentials, query text, response body, or secret names in the audit trail.
        let sys = system.read().await;
        sys.db
            .read()
            .await
            .append_access_event("unauthenticated", &operation(&request), "denied", &[])
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        return Err(StatusCode::UNAUTHORIZED);
    };
    let _guard = access.request_lock.lock().await;
    let action = operation(&request);
    CURRENT_PRINCIPAL
        .scope(principal, async {
            record(&system, &action, "started", &[])
                .await
                .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
            let response = next.run(request).await;
            record(
                &system,
                &action,
                &format!("http_{}", response.status().as_u16()),
                &[],
            )
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
            Ok(response)
        })
        .await
}

pub async fn audit(
    State(system): State<Arc<RwLock<MemorySystem>>>,
) -> Result<axum::Json<serde_json::Value>, StatusCode> {
    require_admin().map_err(|_| StatusCode::FORBIDDEN)?;
    let sys = system.read().await;
    let events = sys
        .db
        .read()
        .await
        .access_events(200)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(axum::Json(
        json!({"events":events,"limit":200,"order":"newest_first","contents_logged":false}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> AccessControl {
        AccessControl::from_json(serde_json::to_string(&json!({"principals":[
            {"id":"admin","admin":true,"token_sha256":format!("{:x}",Sha256::digest(b"owner".repeat(8)))},
            {"id":"reader","token_sha256":format!("{:x}",Sha256::digest(b"reader".repeat(8))),"read_projects":["alpha"]},
            {"id":"writer","token_sha256":format!("{:x}",Sha256::digest(b"writer".repeat(8))),"read_projects":["alpha"],"write_projects":["alpha"]}
        ]})).unwrap().as_bytes()).unwrap()
    }

    #[tokio::test]
    async fn grants_are_exact_and_cross_project_search_is_restricted() {
        let policy = policy();
        assert!(policy.authenticate("Bearer wrong").is_none());
        let reader = policy
            .authenticate(&format!("Bearer {}", "reader".repeat(8)))
            .unwrap();
        CURRENT_PRINCIPAL
            .scope(reader, async {
                assert!(can_read("alpha"));
                assert!(!can_read("beta"));
                assert!(require_write("alpha").is_err());
                assert!(require_admin().is_err());
                assert_eq!(
                    restrict_scope(SearchScope::All)
                        .unwrap()
                        .projects()
                        .unwrap(),
                    ["alpha"]
                );
                assert!(restrict_scope(SearchScope::explicit("beta")).is_err());
            })
            .await;
        let writer = policy
            .authenticate(&format!("Bearer {}", "writer".repeat(8)))
            .unwrap();
        CURRENT_PRINCIPAL
            .scope(writer, async {
                assert!(require_write("alpha").is_ok());
                assert!(require_write("beta").is_err());
            })
            .await;
        assert!(is_admin());
    }

    #[test]
    fn invalid_policy_fails_closed() {
        assert!(AccessControl::from_json(br#"{}"#).is_err());
        assert!(AccessControl::from_json(br#"{"principals":[]}"#).is_err());
        let valid = policy();
        assert!(valid.enabled());
        // A reader-only policy cannot lock its administrator out.
        let reader = json!({"principals":[{"id":"reader","token_sha256":"0".repeat(64),"read_projects":["alpha"]}]});
        assert!(AccessControl::from_json(reader.to_string().as_bytes()).is_err());
    }
}
