pub mod consolidate;
pub mod decay;

use crate::MemorySystem;
use crate::models::{ChunkType, Importance};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// TTL policy by `(chunk_type, importance)`.
///
/// Mirrors the old Factory hook policy: throwaway auto-logged chat decays
/// within a month, automatic-filter outputs are even shorter-lived, and any
/// user-curated content (Manual + Knowledge/Decision/Preference) is kept
/// forever. Returning `None` means "never expire".
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
        ChunkType::AutoLog => Some(30),
        ChunkType::Filtered => Some(7),
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

    if to_delete.is_empty() {
        return Ok(0);
    }

    let mut deleted = 0usize;
    {
        let db = sys.db.write().await;
        for id in &to_delete {
            if db.delete_chunk(id).unwrap_or(false) {
                deleted += 1;
            }
        }
    }

    // Best-effort index cleanup — failures here are logged but don't roll back
    // the DB delete, since stale index entries are harmless (search filters by
    // chunk id presence in the DB).
    if deleted > 0 {
        let _ = sys.remove_text_docs(&to_delete).await;
        let mut vector_index = sys.vector_index.write().await;
        for id in &to_delete {
            let _ = vector_index.remove(id);
        }
        let _ = vector_index.save();
    }

    tracing::info!(
        "lifecycle prune: removed {}/{} expired chunks",
        deleted,
        to_delete.len()
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
                Ok(n) if n > 0 => tracing::info!("daily TTL pruned {} chunks", n),
                Ok(_) => tracing::debug!("daily TTL: nothing to prune"),
                Err(e) => tracing::warn!("daily TTL failed: {e:#}"),
            }
            tokio::time::sleep(std::time::Duration::from_secs(86_400)).await;
        }
    });
}
