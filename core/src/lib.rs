pub mod config;
pub mod crypto;
pub mod doctor;
pub mod embedding;
pub mod facts;
pub mod graph;
pub mod index;
pub mod lifecycle;
pub mod models;
pub mod redaction;
pub mod search;
pub mod server;
pub mod storage;

use anyhow::Result;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct MemorySystem {
    pub config: config::Config,
    pub db: Arc<RwLock<storage::Database>>,
    pub embedder: Arc<embedding::Embedder>,
    pub vector_index: Arc<RwLock<index::VectorIndex>>,
    pub text_index: Arc<RwLock<Option<index::TextIndex>>>,
    pub graph: Arc<RwLock<graph::KnowledgeGraph>>,
}

impl MemorySystem {
    pub async fn new(config: config::Config) -> Result<Self> {
        // Always derive an encryption key: env var takes precedence, otherwise
        // a per-install random key is created under data_dir/master.key (0600).
        // This means PAT/API key storage works out of the box.
        let master_key = crypto::resolve_master_key(&config.data_dir).ok();
        crypto::init_crypto(master_key.as_deref())?;
        let db = Arc::new(RwLock::new(storage::Database::new(&config.data_dir).await?));
        let model_cache = config.data_dir.join("models");
        std::fs::create_dir_all(&model_cache)?;
        let embedder = Arc::new(embedding::Embedder::new(
            &config.embed_model,
            config.embed_dim,
            &model_cache,
        )?);
        let rebuild_indexes = std::env::var("PALIMPSEST_REBUILD_INDEXES")
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false);
        let vector_persisted = index::VectorIndex::persisted(&config.data_dir);
        let text_persisted = index::TextIndex::persisted(&config.data_dir);
        let mut vector = index::VectorIndex::new(&config.data_dir)?;
        if rebuild_indexes || !vector_persisted || !text_persisted {
            let db_guard = db.read().await;
            let chunks = db_guard.get_all_chunks(100_000)?;
            if rebuild_indexes || !vector_persisted {
                let vectors: Vec<(String, Vec<f32>)> = chunks
                    .iter()
                    .filter_map(|chunk| {
                        chunk
                            .embedding
                            .as_ref()
                            .map(|embedding| (chunk.id.clone(), embedding.clone()))
                    })
                    .collect();
                vector.rebuild(&vectors)?;
                if !vectors.is_empty() {
                    vector.save()?;
                }
            }
            if rebuild_indexes || !text_persisted {
                let mut text = index::TextIndex::new(&config.data_dir)?;
                let text_docs: Vec<(String, String, String)> = chunks
                    .into_iter()
                    .map(|chunk| (chunk.id, chunk.project, chunk.document))
                    .collect();
                text.replace_all_with_project(&text_docs)?;
            }
        }
        let vector_index = Arc::new(RwLock::new(vector));
        let text_index = Arc::new(RwLock::new(None));
        let graph = Arc::new(RwLock::new(graph::KnowledgeGraph::new(&config.data_dir)?));

        Ok(Self {
            config,
            db,
            embedder,
            vector_index,
            text_index,
            graph,
        })
    }

    pub async fn save(&self) -> Result<()> {
        let vector = self.vector_index.write().await;
        vector.save()?;
        Ok(())
    }

    pub async fn text_search(&self, query: &str, k: usize) -> Result<Vec<(String, f32)>> {
        let mut guard = self.text_index.write().await;
        let text = ensure_text_index(&mut guard, &self.config.data_dir)?;
        text.search(query, k)
    }

    pub async fn add_text_doc(&self, id: &str, project: &str, text_body: &str) -> Result<()> {
        let mut guard = self.text_index.write().await;
        let text = ensure_text_index(&mut guard, &self.config.data_dir)?;
        text.add_with_project(id, project, text_body)
    }

    pub async fn add_text_docs(&self, docs: &[(String, String, String)]) -> Result<()> {
        let mut guard = self.text_index.write().await;
        let text = ensure_text_index(&mut guard, &self.config.data_dir)?;
        text.add_many_with_project(docs)
    }

    pub async fn remove_text_docs(&self, ids: &[String]) -> Result<()> {
        let mut guard = self.text_index.write().await;
        let text = ensure_text_index(&mut guard, &self.config.data_dir)?;
        text.remove_many(ids)
    }

    /// Re-add already-migrated chunks to the text index so the `project` field
    /// reflects the new bucket after a session fork. Removing first is implied
    /// by `add_many_with_project` because it `delete_term`s by id before write.
    pub async fn reindex_after_fork(
        &self,
        chunks: &[crate::models::MemoryChunk],
    ) -> Result<()> {
        if chunks.is_empty() {
            return Ok(());
        }
        let docs: Vec<(String, String, String)> = chunks
            .iter()
            .map(|chunk| (chunk.id.clone(), chunk.project.clone(), chunk.document.clone()))
            .collect();
        self.add_text_docs(&docs).await
    }
}

fn ensure_text_index<'a>(
    slot: &'a mut Option<index::TextIndex>,
    data_dir: &Path,
) -> Result<&'a mut index::TextIndex> {
    if slot.is_none() {
        *slot = Some(index::TextIndex::new(data_dir)?);
    }
    slot.as_mut()
        .ok_or_else(|| anyhow::anyhow!("text index failed to initialize"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ChunkType, Importance, MemoryChunk, Metadata};
    use chrono::Utc;

    #[tokio::test]
    async fn startup_uses_persisted_indexes_without_vector_duplication() {
        let temp = tempfile::tempdir().unwrap();
        let mut config = config::Config::default();
        config.data_dir = temp.path().to_path_buf();

        {
            let system = MemorySystem::new(config.clone()).await.unwrap();
            let chunk = MemoryChunk {
                id: "chunk-alpha".to_string(),
                project: "product".to_string(),
                document: "durable restart marker".to_string(),
                embedding: Some(vec![1.0, 0.0, 0.0]),
                metadata: Metadata {
                    chunk_type: ChunkType::Manual,
                    importance: Importance::Knowledge,
                    ..Default::default()
                },
                created_at: Utc::now(),
                updated_at: Utc::now(),
            };
            system
                .db
                .write()
                .await
                .insert_chunk(&chunk)
                .expect("insert chunk");
            system
                .add_text_doc(&chunk.id, &chunk.project, &chunk.document)
                .await
                .expect("add text");
            let mut vector_index = system.vector_index.write().await;
            vector_index
                .add(&chunk.id, chunk.embedding.as_ref().unwrap())
                .expect("add vector");
            vector_index.save().expect("save vector");
        }

        let first_restart = MemorySystem::new(config.clone()).await.unwrap();
        assert!(first_restart.text_index.read().await.is_none());
        assert_eq!(first_restart.vector_index.read().await.len(), 1);
        let text_results = first_restart
            .text_search("restart", 3)
            .await
            .expect("search text");
        assert_eq!(
            text_results.first().map(|(id, _)| id.as_str()),
            Some("chunk-alpha")
        );
        drop(first_restart);

        let second_restart = MemorySystem::new(config).await.unwrap();
        assert_eq!(second_restart.vector_index.read().await.len(), 1);
    }
}
