use crate::models::*;
use crate::search::compute_decay_score;

pub fn analyze_chunk_decay(chunk: &MemoryChunk) -> f64 {
    let now = chrono::Utc::now();
    let days_old = (now - chunk.created_at).num_days() as f64;
    compute_decay_score(
        chunk.metadata.access_count,
        days_old,
        &chunk.metadata.importance,
    )
}

pub fn is_stale(chunk: &MemoryChunk, threshold: f64) -> bool {
    analyze_chunk_decay(chunk) < threshold
}
