pub mod decay;

use crate::MemorySystem;
use crate::models::{ChunkType, Importance};
use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Default, Clone)]
pub struct LifecycleStatus {
    pub last_run: Option<chrono::DateTime<chrono::Utc>>,
    pub last_deleted: usize,
    pub last_error: Option<String>,
    pub ttl_autolog_days: Option<i64>,
}

/// TTL policy by `(chunk_type, importance)`.
///
/// Mirrors the old Factory hook policy: throwaway auto-logged chat decays
/// within a month, automatic-filter outputs are even shorter-lived, and any
/// user-curated content (Manual + Knowledge/Decision/Preference) is kept
/// forever. Returning `None` means "never expire".
///
/// The AutoLog window is overridable via `MEMNEST_TTL_AUTOLOG_DAYS`:
/// `0` / `off` / `unlimited` disables expiry entirely (dialogue kept forever
/// for long-range recall); any positive integer replaces the 30-day default.
/// Invalid values fall back to 30.
fn autolog_ttl_days() -> Option<i64> {
    static TTL: std::sync::OnceLock<Option<i64>> = std::sync::OnceLock::new();
    *TTL.get_or_init(|| match std::env::var("MEMNEST_TTL_AUTOLOG_DAYS") {
        Err(_) => Some(30),
        Ok(raw) => {
            let v = raw.trim();
            if v == "0" || v.eq_ignore_ascii_case("off") || v.eq_ignore_ascii_case("unlimited") {
                None
            } else {
                v.parse::<i64>().ok().filter(|d| *d > 0).or(Some(30))
            }
        }
    })
}

fn ttl_days_for(chunk_type: &ChunkType, importance: &Importance) -> Option<i64> {
    // Anything the user explicitly marked is permanent regardless of source.
    if matches!(
        importance,
        Importance::Knowledge | Importance::Decision | Importance::Preference
    ) {
        return None;
    }
    match chunk_type {
        ChunkType::Manual => None,
        ChunkType::Consolidated => None, // summaries replace raw logs, keep them
        ChunkType::AutoLog => autolog_ttl_days(),
        ChunkType::Filtered => Some(7),
    }
}

/// Append a single JSON audit line to `<data_dir>/audit.log`.
/// Failures are logged as warnings and never block the caller.
pub(crate) fn append_audit_log(
    data_dir: &Path,
    source: &str,
    deleted: usize,
    filters: serde_json::Value,
) {
    use std::io::Write;
    let ts = chrono::Utc::now().to_rfc3339();
    let line = match serde_json::to_string(&serde_json::json!({
        "ts": ts,
        "source": source,
        "deleted": deleted,
        "filters": filters,
    })) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("audit log serialize failed: {e:#}");
            return;
        }
    };
    let path = data_dir.join("audit.log");
    if let Err(e) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| writeln!(f, "{line}"))
    {
        tracing::warn!("audit log write failed: {e:#}");
    }
}

pub async fn run_lifecycle(
    system: Arc<RwLock<MemorySystem>>,
) -> Result<HashMap<String, serde_json::Value>> {
    let sys = system.read().await;
    let db = sys.db.read().await;
    let mut stats: HashMap<String, serde_json::Value> = HashMap::new();

    let mut stale_count = 0usize;
    let mut total_count = 0usize;

    if let Ok(chunks) = db.get_all_chunks(100_000) {
        for chunk in chunks {
            total_count += 1;
            let score = decay::analyze_chunk_decay(&chunk);
            if score < 0.5 {
                stale_count += 1;
            }
        }
    }

    stats.insert("total_chunks".to_string(), serde_json::json!(total_count));
    stats.insert("stale_chunks".to_string(), serde_json::json!(stale_count));
    stats.insert("status".to_string(), serde_json::json!("ok"));

    Ok(stats)
}

/// Apply the TTL policy: delete chunks whose age exceeds the limit for their
/// (chunk_type, importance) bucket. Returns the number of chunks pruned.
///
/// This is the heart of automatic decay — without it, AutoLog content piles
/// up forever (the current production state, ~537 chunks). Indexes are kept
/// in sync so search results immediately reflect the pruning.
pub async fn prune_expired(system: Arc<RwLock<MemorySystem>>) -> Result<usize> {
    let sys = system.read().await;
    let now = chrono::Utc::now();

    // Phase 1: identify victims under the read lock. We deliberately collect
    // ids first instead of deleting in-place so the second phase can drop the
    // read lock before acquiring the write locks for index removal.
    let mut to_delete: Vec<String> = Vec::new();
    {
        let db = sys.db.read().await;
        let chunks = db.get_all_chunks(1_000_000).unwrap_or_default();
        for chunk in chunks {
            if let Some(days) = ttl_days_for(&chunk.metadata.chunk_type, &chunk.metadata.importance)
            {
                let age = (now - chunk.created_at).num_days();
                if age > days {
                    to_delete.push(chunk.id);
                }
            }
        }
    }

    let deleted = if !to_delete.is_empty() {
        let mut count = 0usize;
        {
            let db = sys.db.write().await;
            for id in &to_delete {
                if db.delete_chunk(id).unwrap_or(false) {
                    count += 1;
                }
            }
        }
        // Best-effort index cleanup — failures here are logged but don't roll back
        // the DB delete, since stale index entries are harmless (search filters by
        // chunk id presence in the DB).
        if count > 0 {
            let _ = sys.remove_text_docs(&to_delete).await;
            let mut vector_index = sys.vector_index.write().await;
            for id in &to_delete {
                let _ = vector_index.remove(id);
            }
            let _ = vector_index.save();
        }
        tracing::info!(
            "lifecycle prune: removed {}/{} expired chunks",
            count,
            to_delete.len()
        );
        count
    } else {
        0
    };

    append_audit_log(&sys.config.data_dir, "ttl", deleted, serde_json::Value::Null);

    {
        let mut status = sys.lifecycle_status.write().await;
        status.last_run = Some(chrono::Utc::now());
        status.last_deleted = deleted;
        status.last_error = None;
        status.ttl_autolog_days = autolog_ttl_days();
    }

    Ok(deleted)
}

/// Spawn the periodic lifecycle worker. Runs once at startup (after a short
/// grace period so the server can finish warming up) and then every 24h.
///
/// We use a long-lived `tokio::spawn` rather than cron because memnest is
/// a single binary with no external scheduler — the goal is zero-config
/// maintenance.
pub fn spawn_periodic_lifecycle(system: Arc<RwLock<MemorySystem>>) {
    tokio::spawn(async move {
        // Give the rest of the boot sequence a head start before scanning.
        tokio::time::sleep(std::time::Duration::from_secs(120)).await;
        loop {
            match prune_expired(system.clone()).await {
                Ok(n) if n > 0 => tracing::info!("daily TTL pruned {} chunks", n),
                Ok(_) => tracing::info!("daily TTL: nothing to prune"),
                Err(e) => {
                    tracing::warn!("daily TTL failed: {e:#}");
                    let sys = system.read().await;
                    let mut status = sys.lifecycle_status.write().await;
                    status.last_error = Some(format!("{e:#}"));
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(86_400)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    #[tokio::test]
    async fn lifecycle_status_ok_and_err_paths() {
        let status = Arc::new(RwLock::new(LifecycleStatus::default()));

        // Verify defaults
        {
            let s = status.read().await;
            assert!(s.last_run.is_none());
            assert_eq!(s.last_deleted, 0);
            assert!(s.last_error.is_none());
        }

        // Simulate success update (mirrors what prune_expired does)
        {
            let mut s = status.write().await;
            s.last_run = Some(chrono::Utc::now());
            s.last_deleted = 7;
            s.last_error = None;
            s.ttl_autolog_days = autolog_ttl_days();
        }
        {
            let s = status.read().await;
            assert!(s.last_run.is_some());
            assert_eq!(s.last_deleted, 7);
            assert!(s.last_error.is_none());
        }

        // Simulate error update (mirrors what spawn_periodic_lifecycle error arm does)
        {
            let mut s = status.write().await;
            s.last_error = Some("simulated db failure".to_string());
        }
        {
            let s = status.read().await;
            assert_eq!(s.last_error.as_deref(), Some("simulated db failure"));
            // last_run still set; error does not clear it
            assert!(s.last_run.is_some());
        }
    }

    #[test]
    fn audit_log_line_has_required_fields() {
        let dir = tempfile::tempdir().unwrap();
        append_audit_log(dir.path(), "api", 5, serde_json::json!({"project": "test"}));
        let content = std::fs::read_to_string(dir.path().join("audit.log")).unwrap();
        let v: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
        assert!(v["ts"].is_string(), "ts missing");
        assert_eq!(v["source"], "api");
        assert_eq!(v["deleted"], 5);
        assert!(v.get("filters").is_some(), "filters missing");
    }

    #[test]
    fn audit_log_ttl_source_and_null_filters() {
        let dir = tempfile::tempdir().unwrap();
        append_audit_log(dir.path(), "ttl", 0, serde_json::Value::Null);
        let content = std::fs::read_to_string(dir.path().join("audit.log")).unwrap();
        let v: serde_json::Value = serde_json::from_str(content.trim()).unwrap();
        assert_eq!(v["source"], "ttl");
        assert_eq!(v["deleted"], 0);
        assert!(v["filters"].is_null());
    }
}
