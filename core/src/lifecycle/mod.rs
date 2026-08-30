use crate::MemorySystem;
use crate::models::{ChunkType, Importance, Metadata};
use anyhow::Result;
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

fn parse_autolog_ttl(raw: Option<&str>) -> Option<i64> {
    match raw {
        None => Some(30),
        Some(value)
            if value.trim() == "0"
                || value.trim().eq_ignore_ascii_case("off")
                || value.trim().eq_ignore_ascii_case("unlimited") =>
        {
            None
        }
        Some(value) => value
            .trim()
            .parse::<i64>()
            .ok()
            .filter(|days| *days > 0)
            .or(Some(30)),
    }
}

fn autolog_ttl_days() -> Option<i64> {
    parse_autolog_ttl(std::env::var("MEMNEST_TTL_AUTOLOG_DAYS").ok().as_deref())
}

fn is_transcript_autolog(metadata: &Metadata) -> bool {
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

/// New preservation-first transcript events are permanent. Legacy AutoLog
/// keeps its existing configurable 30-day default so old tool noise does not
/// silently become permanent.
fn ttl_days_for(metadata: &Metadata) -> Option<i64> {
    if matches!(
        metadata.importance,
        Importance::Knowledge | Importance::Decision | Importance::Preference
    ) || is_transcript_autolog(metadata)
    {
        return None;
    }
    match metadata.chunk_type {
        ChunkType::Manual | ChunkType::Consolidated => None,
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

/// Apply the TTL policy: soft-delete chunks whose age exceeds the limit for
/// their (chunk_type, importance) bucket by moving them to `_trash`.
/// Returns the number of chunks moved to trash.
///
/// Chunks in `_trash` are kept for 30 days then hard-deleted by
/// `prune_trash`. This two-phase approach gives a recovery window.
pub async fn prune_expired(system: Arc<RwLock<MemorySystem>>) -> Result<usize> {
    let sys = system.read().await;
    let now = chrono::Utc::now();
    let now_str = now.to_rfc3339();

    // Phase 1: collect victim ids under the read lock.
    let mut to_trash: Vec<String> = Vec::new();
    {
        let db = sys.db.read().await;
        let chunks = db.get_all_chunks(1_000_000).unwrap_or_default();
        for chunk in chunks {
            if chunk.metadata.pinned {
                continue;
            }
            // Skip chunks already in _trash (avoid re-trashing).
            if chunk.project == "_trash" {
                continue;
            }
            if let Some(days) = ttl_days_for(&chunk.metadata) {
                let age = (now - chunk.created_at).num_days();
                if age > days {
                    to_trash.push(chunk.id);
                }
            }
        }
    }

    let trashed = if !to_trash.is_empty() {
        let mut count = 0usize;
        {
            let db = sys.db.write().await;
            for id in &to_trash {
                if db.trash_chunk(id, &now_str)? {
                    count += 1;
                }
            }
        }
        if count > 0 {
            sys.sync_pending_indexes().await?;
        }
        tracing::info!(
            "lifecycle prune: trashed {}/{} expired chunks",
            count,
            to_trash.len()
        );
        count
    } else {
        0
    };

    append_audit_log(
        &sys.config.data_dir,
        "ttl",
        trashed,
        serde_json::Value::Null,
    );

    {
        let mut status = sys.lifecycle_status.write().await;
        status.last_run = Some(chrono::Utc::now());
        status.last_deleted = trashed;
        status.last_error = None;
        status.ttl_autolog_days = autolog_ttl_days();
    }

    Ok(trashed)
}

/// Whether cold archival is enabled. `MEMNEST_ARCHIVE=0` / `off` / `false`
/// disables writes to `<data_dir>/archive/YYYY-MM.jsonl`.
fn archive_enabled() -> bool {
    match std::env::var("MEMNEST_ARCHIVE") {
        Err(_) => true,
        Ok(raw) => {
            let v = raw.trim();
            !(v == "0" || v.eq_ignore_ascii_case("off") || v.eq_ignore_ascii_case("false"))
        }
    }
}

/// Append a full chunk JSON line to `<data_dir>/archive/YYYY-MM.jsonl` before
/// hard-delete. Failures warn only — availability over completeness.
fn archive_chunk_before_delete(data_dir: &Path, chunk: &crate::models::MemoryChunk) {
    if !archive_enabled() {
        return;
    }
    use std::io::Write;
    let month = chrono::Utc::now().format("%Y-%m");
    let dir = data_dir.join("archive");
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::warn!("archive mkdir failed: {e:#}");
        return;
    }
    let path = dir.join(format!("{month}.jsonl"));
    let line = match serde_json::to_string(chunk) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("archive serialize failed for {}: {e:#}", chunk.id);
            return;
        }
    };
    if let Err(e) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .and_then(|mut f| writeln!(f, "{line}"))
    {
        tracing::warn!("archive write failed for {}: {e:#}", chunk.id);
    }
}

/// Hard-delete `_trash` rows whose `trashed_at` is older than 30 days.
/// This is the second phase of soft-delete: the trash GC provides a 30-day
/// recovery window then permanently removes the row. Pinned status is ignored
/// (trash is trash). Writes a "trash_gc" audit line. Before each hard delete
/// the full chunk is archived to `archive/YYYY-MM.jsonl` (unless
/// `MEMNEST_ARCHIVE=0`).
pub async fn prune_trash(system: Arc<RwLock<MemorySystem>>) -> Result<usize> {
    let sys = system.read().await;
    let cutoff = chrono::Utc::now() - chrono::Duration::days(30);

    let mut to_delete: Vec<crate::models::MemoryChunk> = Vec::new();
    {
        let db = sys.db.read().await;
        let trash = db
            .get_chunks_by_project("_trash", 1_000_000)
            .unwrap_or_default();
        for chunk in trash {
            let expired = chunk
                .metadata
                .trashed_at
                .as_deref()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&chrono::Utc) < cutoff)
                .unwrap_or(false);
            if expired {
                to_delete.push(chunk);
            }
        }
    }

    let deleted = if !to_delete.is_empty() {
        let mut count = 0usize;
        {
            let db = sys.db.write().await;
            for chunk in &to_delete {
                archive_chunk_before_delete(&sys.config.data_dir, chunk);
                if db.delete_chunk(&chunk.id)? {
                    count += 1;
                }
            }
        }
        if count > 0 {
            sys.sync_pending_indexes().await?;
        }
        tracing::info!("trash_gc: hard-deleted {} expired trash rows", count);
        count
    } else {
        0
    };

    append_audit_log(
        &sys.config.data_dir,
        "trash_gc",
        deleted,
        serde_json::Value::Null,
    );
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
                Ok(n) if n > 0 => tracing::info!("daily TTL trashed {} chunks", n),
                Ok(_) => tracing::info!("daily TTL: nothing to prune"),
                Err(e) => {
                    tracing::warn!("daily TTL failed: {e:#}");
                    let sys = system.read().await;
                    let mut status = sys.lifecycle_status.write().await;
                    status.last_error = Some(format!("{e:#}"));
                }
            }
            match prune_trash(system.clone()).await {
                Ok(n) if n > 0 => tracing::info!("trash_gc hard-deleted {} old rows", n),
                Ok(_) => {}
                Err(e) => tracing::warn!("trash_gc failed: {e:#}"),
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
    fn only_identified_transcript_autolog_is_permanent() {
        assert_eq!(parse_autolog_ttl(None), Some(30));
        assert_eq!(parse_autolog_ttl(Some("45")), Some(45));
        assert_eq!(parse_autolog_ttl(Some("off")), None);

        let legacy = Metadata {
            chunk_type: ChunkType::AutoLog,
            importance: Importance::Log,
            ..Default::default()
        };
        let legacy_ttl = autolog_ttl_days();
        assert_eq!(ttl_days_for(&legacy), legacy_ttl);
        assert_eq!(
            ttl_days_for(&Metadata {
                source: Some("pi.transcript".into()),
                ..legacy.clone()
            }),
            legacy_ttl
        );
        assert_eq!(
            ttl_days_for(&Metadata {
                event_id: Some("event-1".into()),
                ..legacy.clone()
            }),
            legacy_ttl
        );

        let transcript = Metadata {
            source: Some("pi.transcript".into()),
            event_id: Some("event-1".into()),
            ..legacy.clone()
        };
        assert_eq!(ttl_days_for(&transcript), None);

        let filtered = Metadata {
            chunk_type: ChunkType::Filtered,
            importance: Importance::Log,
            ..Default::default()
        };
        assert_eq!(ttl_days_for(&filtered), Some(7));
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

    #[test]
    fn trash_purge_cutoff_selects_correct_rows() {
        let now = chrono::Utc::now();
        let cutoff = now - chrono::Duration::days(30);

        let should_purge = |trashed_at_str: &str| -> bool {
            chrono::DateTime::parse_from_rfc3339(trashed_at_str)
                .map(|dt| dt.with_timezone(&chrono::Utc) < cutoff)
                .unwrap_or(false)
        };

        let old = (now - chrono::Duration::days(40)).to_rfc3339();
        let recent = (now - chrono::Duration::days(10)).to_rfc3339();
        assert!(should_purge(&old), "40-day-old trash must be purged");
        assert!(!should_purge(&recent), "10-day-old trash must survive");
    }

    #[test]
    fn archive_writes_jsonl_when_enabled() {
        let dir = tempfile::tempdir().unwrap();
        // force enable (default)
        unsafe {
            std::env::remove_var("MEMNEST_ARCHIVE");
        }
        let chunk = crate::models::MemoryChunk {
            id: "arch1".into(),
            project: "_trash".into(),
            document: "archived body".into(),
            embedding: None,
            metadata: Default::default(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        archive_chunk_before_delete(dir.path(), &chunk);
        let month = chrono::Utc::now().format("%Y-%m");
        let path = dir.path().join("archive").join(format!("{month}.jsonl"));
        let content = std::fs::read_to_string(&path).expect("archive file");
        assert!(content.contains("arch1"));
        assert!(content.contains("archived body"));
    }

    #[test]
    fn archive_disabled_by_env() {
        let dir = tempfile::tempdir().unwrap();
        unsafe {
            std::env::set_var("MEMNEST_ARCHIVE", "0");
        }
        let chunk = crate::models::MemoryChunk {
            id: "arch0".into(),
            project: "_trash".into(),
            document: "should not write".into(),
            embedding: None,
            metadata: Default::default(),
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };
        archive_chunk_before_delete(dir.path(), &chunk);
        unsafe {
            std::env::remove_var("MEMNEST_ARCHIVE");
        }
        let month = chrono::Utc::now().format("%Y-%m");
        let path = dir.path().join("archive").join(format!("{month}.jsonl"));
        assert!(
            !path.exists(),
            "archive must not exist when MEMNEST_ARCHIVE=0"
        );
    }
}
