use crate::models::*;
use anyhow::Result;

pub fn consolidate_chunks(chunks: &[MemoryChunk]) -> Result<String> {
    if chunks.is_empty() {
        return Ok(String::new());
    }
    // Simple keyword-based summary
    let all_text: String = chunks
        .iter()
        .map(|c| c.document.clone())
        .collect::<Vec<_>>()
        .join(" ");
    let keywords = crate::search::extract_keywords(&all_text, 3);
    let top = keywords.into_iter().take(5).collect::<Vec<_>>().join(", ");
    Ok(format!(
        "[Consolidated] 관련 작업 {}건: {}",
        chunks.len(),
        top
    ))
}
