pub mod config;
pub mod crypto;
pub mod doctor;
pub mod embedding;
pub mod eval;
pub mod facts;
pub mod hook;
pub mod index;
pub mod lifecycle;
pub mod models;
pub mod redaction;
pub mod search;
pub mod search_metrics;
pub mod server;
pub mod storage;
pub mod watch;
pub mod workspace;

use anyhow::{Context, Result};
use fs2::FileExt;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct MemorySystem {
    pub config: config::Config,
    pub db: Arc<RwLock<storage::Database>>,
    pub embedder: Arc<embedding::Embedder>,
    pub vector_index: Arc<RwLock<index::VectorIndex>>,
    pub text_index: Arc<RwLock<Option<index::TextIndex>>>,
    pub lifecycle_status: Arc<RwLock<lifecycle::LifecycleStatus>>,
    pub vault_enabled: bool,
    pub secret_tools_enabled: bool,
    index_sync: tokio::sync::Mutex<()>,
    _writer_lock: std::fs::File,
}

impl MemorySystem {
    pub async fn new(config: config::Config) -> Result<Self> {
        std::fs::create_dir_all(&config.data_dir)?;
        let writer_lock = acquire_writer_lock(&config.data_dir)?;
        // Always derive an encryption key: env var takes precedence, otherwise
        // a per-install random key is created under data_dir/master.key (0600).
        // This means PAT/API key storage works out of the box.
        let master_key = crypto::resolve_master_key(&config.data_dir)?;
        crypto::init_crypto(Some(&master_key))?;
        let database = storage::Database::new(&config.data_dir).await?;
        let encrypted_values = database.encrypted_vault_values()?;
        if let Err(error) = crypto::validate_ciphertexts(&master_key, &encrypted_values) {
            crypto::disable_crypto()?;
            return Err(error).context("vault key validation failed");
        }
        let db = Arc::new(RwLock::new(database));
        let model_cache = config.data_dir.join("models");
        std::fs::create_dir_all(&model_cache)?;
        let embedder = Arc::new(embedding::Embedder::new(
            &config.embed_model,
            config.embed_dim,
            &model_cache,
        )?);
        let forced_rebuild = std::env::var("MEMNEST_REBUILD_INDEXES")
            .map(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES"))
            .unwrap_or(false);
        let vector_persisted = index::VectorIndex::persisted(&config.data_dir);
        let text_persisted = index::TextIndex::persisted(&config.data_dir);
        let mut rebuild_indexes = forced_rebuild
            || !vector_persisted
            || !text_persisted
            || db.read().await.index_rebuild_required()?;
        let mut vector = match index::VectorIndex::new(&config.data_dir) {
            Ok(index) => index,
            Err(error) => {
                tracing::warn!("rebuilding unreadable HNSW index: {error:#}");
                rebuild_indexes = true;
                index::VectorIndex::fresh(&config.data_dir)?
            }
        };
        if text_persisted && index::TextIndex::new(&config.data_dir).is_err() {
            tracing::warn!("rebuilding unreadable Tantivy index");
            rebuild_indexes = true;
        }
        if rebuild_indexes {
            let db_guard = db.read().await;
            let chunks: Vec<_> = db_guard
                .get_all_chunks_unbounded()?
                .into_iter()
                .filter(|chunk| !models::is_internal_project(&chunk.project))
                .collect();
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
            vector.save()?;
            let text_docs: Vec<(String, String, String)> = chunks
                .into_iter()
                .map(|chunk| (chunk.id, chunk.project, chunk.document))
                .collect();
            index::TextIndex::rebuild_atomic(&config.data_dir, &text_docs)?;
            db_guard.complete_index_rebuild()?;
        }
        let vector_index = Arc::new(RwLock::new(vector));
        let text_index = Arc::new(RwLock::new(None));

        Ok(Self {
            config,
            db,
            embedder,
            vector_index,
            text_index,
            lifecycle_status: Arc::new(RwLock::new(lifecycle::LifecycleStatus::default())),
            vault_enabled: true,
            secret_tools_enabled: std::env::var("MEMNEST_EXPOSE_SECRET_TOOLS")
                .is_ok_and(|value| matches!(value.as_str(), "1" | "true" | "TRUE" | "yes" | "YES")),
            index_sync: tokio::sync::Mutex::new(()),
            _writer_lock: writer_lock,
        })
    }

    pub async fn save(&self) -> Result<()> {
        let vector = self.vector_index.write().await;
        vector.save()?;
        Ok(())
    }

    pub async fn sync_pending_indexes(&self) -> Result<()> {
        let _sync = self.index_sync.lock().await;
        let (pending, upserts) = {
            let db = self.db.read().await;
            let pending = db.pending_index_ops()?;
            let mut upserts = Vec::new();
            for (id, operation, _) in &pending {
                if operation == "upsert"
                    && let Some(chunk) = db.get_chunk(id)?
                    && !models::is_internal_project(&chunk.project)
                {
                    upserts.push(chunk);
                }
            }
            (pending, upserts)
        };
        if pending.is_empty() {
            return Ok(());
        }
        let ids: Vec<String> = pending.iter().map(|(id, _, _)| id.clone()).collect();
        let completed: Vec<(String, i64)> = pending
            .into_iter()
            .map(|(id, _, generation)| (id, generation))
            .collect();
        self.remove_text_docs(&ids).await?;
        let text_docs: Vec<(String, String, String)> = upserts
            .iter()
            .map(|chunk| {
                (
                    chunk.id.clone(),
                    chunk.project.clone(),
                    chunk.document.clone(),
                )
            })
            .collect();
        self.add_text_docs(&text_docs).await?;
        {
            let mut vector = self.vector_index.write().await;
            for id in &ids {
                vector.remove(id)?;
            }
            for chunk in &upserts {
                if let Some(embedding) = &chunk.embedding {
                    vector.add(&chunk.id, embedding)?;
                }
            }
            vector.save()?;
        }
        self.db.read().await.complete_index_ops(&completed)?;
        Ok(())
    }

    pub async fn text_search(&self, query: &str, k: usize) -> Result<Vec<(String, f32)>> {
        self.text_search_projects(query, &[], k).await
    }

    pub async fn text_search_projects(
        &self,
        query: &str,
        projects: &[String],
        k: usize,
    ) -> Result<Vec<(String, f32)>> {
        let mut guard = self.text_index.write().await;
        let text = ensure_text_index(&mut guard, &self.config.data_dir)?;
        text.search_projects(query, projects, k)
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
}

pub fn acquire_writer_lock(data_dir: &Path) -> Result<std::fs::File> {
    let parent = data_dir
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let parent = std::fs::canonicalize(parent)?;
    let name = data_dir
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("memnest");
    let lock_path = parent.join(format!(".{name}.writer.lock"));
    let lock = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(lock_path)?;
    lock.try_lock_exclusive()
        .with_context(|| format!("another memnest writer already owns {}", data_dir.display()))?;
    Ok(lock)
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

    #[test]
    fn data_directory_has_one_writer() {
        let temp = tempfile::tempdir().unwrap();
        let first = acquire_writer_lock(temp.path()).unwrap();
        let error = acquire_writer_lock(temp.path()).unwrap_err();
        assert!(error.to_string().contains("already owns"));
        drop(first);
        acquire_writer_lock(temp.path()).unwrap();
    }

    #[tokio::test]
    async fn startup_repairs_queued_sqlite_write_without_vector_duplication() {
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
            assert_eq!(system.db.read().await.pending_index_ops().unwrap().len(), 1);
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
        assert!(
            second_restart
                .db
                .read()
                .await
                .pending_index_ops()
                .unwrap()
                .is_empty()
        );
    }
}
