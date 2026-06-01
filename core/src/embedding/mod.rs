use anyhow::{Context, Result, anyhow};
use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Maximum number of cached embeddings (FIFO eviction).
/// Each f32x768 vector ≈ 3 KB, so 512 entries ≈ 1.5 MB.
const EMBED_CACHE_CAPACITY: usize = 512;

#[derive(Default)]
struct EmbedCache {
    map: HashMap<(String, &'static str), Vec<f32>>,
    order: Vec<(String, &'static str)>,
}

impl EmbedCache {
    fn get(&self, text: &str, prefix: &'static str) -> Option<Vec<f32>> {
        self.map.get(&(text.to_string(), prefix)).cloned()
    }

    fn insert(&mut self, text: String, prefix: &'static str, value: Vec<f32>) {
        let key = (text, prefix);
        if self.map.contains_key(&key) {
            return;
        }
        if self.order.len() >= EMBED_CACHE_CAPACITY {
            if let Some(oldest) = self.order.first().cloned() {
                self.map.remove(&oldest);
                self.order.remove(0);
            }
        }
        self.map.insert(key.clone(), value);
        self.order.push(key);
    }
}

pub struct Embedder {
    model: Mutex<Option<TextEmbedding>>,
    dim: usize,
    model_name: String,
    cache_dir: PathBuf,
    keep_loaded: bool,
    cache: Mutex<EmbedCache>,
}

impl Embedder {
    pub fn new(model_name: &str, dim: usize, cache_dir: &Path) -> Result<Self> {
        parse_embedding_model(model_name)?;
        Ok(Self {
            model: Mutex::new(None),
            dim,
            model_name: model_name.to_string(),
            cache_dir: cache_dir.to_path_buf(),
            // Default: keep model resident in memory after first load.
            // Set MEMNEST_EMBED_KEEP_LOADED=0 to opt out (e.g. for memory-constrained envs).
            keep_loaded: env_flag("MEMNEST_EMBED_KEEP_LOADED", true),
            cache: Mutex::new(EmbedCache::default()),
        })
    }

    pub fn dim(&self) -> usize {
        self.dim
    }

    pub fn model_name(&self) -> &str {
        &self.model_name
    }

    pub fn encode(&self, text: &str) -> Result<Vec<f32>> {
        self.encode_document(text)
    }

    pub fn encode_query(&self, text: &str) -> Result<Vec<f32>> {
        self.encode_with_prefix(text, "query: ")
    }

    pub fn encode_document(&self, text: &str) -> Result<Vec<f32>> {
        self.encode_with_prefix(text, "passage: ")
    }

    pub fn encode_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let inputs: Vec<String> = texts.iter().map(|t| format!("passage: {t}")).collect();
        let mut slot = self
            .model
            .lock()
            .map_err(|_| anyhow!("embedding model lock poisoned"))?;
        let model = self.ensure_model(&mut slot)?;
        let embeddings = model
            .embed(inputs, None)
            .context("native embedding failed")?;
        if !self.keep_loaded {
            *slot = None;
        }
        Ok(embeddings)
    }

    fn encode_with_prefix(&self, text: &str, prefix: &'static str) -> Result<Vec<f32>> {
        // Cache hit: avoid model invocation entirely.
        if let Ok(cache) = self.cache.lock()
            && let Some(cached) = cache.get(text, prefix)
        {
            return Ok(cached);
        }
        let input = format!("{prefix}{text}");
        let mut slot = self
            .model
            .lock()
            .map_err(|_| anyhow!("embedding model lock poisoned"))?;
        let model = self.ensure_model(&mut slot)?;
        let mut embeddings = model
            .embed(vec![input], None)
            .context("native embedding failed")?;
        if !self.keep_loaded {
            *slot = None;
        }
        let embedding = embeddings
            .pop()
            .ok_or_else(|| anyhow!("embedding provider returned empty result"))?;
        if let Ok(mut cache) = self.cache.lock() {
            cache.insert(text.to_string(), prefix, embedding.clone());
        }
        Ok(embedding)
    }

    fn ensure_model<'a>(
        &self,
        slot: &'a mut Option<TextEmbedding>,
    ) -> Result<&'a mut TextEmbedding> {
        if slot.is_none() {
            *slot = Some(init_model(&self.model_name, &self.cache_dir)?);
        }
        slot.as_mut()
            .ok_or_else(|| anyhow!("embedding model failed to initialize"))
    }
}

fn env_flag(name: &str, default: bool) -> bool {
    std::env::var(name)
        .map(|value| {
            matches!(
                value.as_str(),
                "1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON"
            )
        })
        .unwrap_or(default)
}

fn init_model(model_name: &str, cache_dir: &Path) -> Result<TextEmbedding> {
    let model = parse_embedding_model(model_name)?;
    let opts = InitOptions::new(model)
        .with_cache_dir(cache_dir.to_path_buf())
        .with_show_download_progress(true);
    TextEmbedding::try_new(opts).context(
        "failed to initialize embedding model. \
         If this is the first run, an internet connection is required to download the model. \
         If you are offline, run 'memnest' on a machine with internet first, then copy the model cache directory."
    )
}

fn parse_embedding_model(model_name: &str) -> Result<EmbeddingModel> {
    let normalized = model_name.trim().to_ascii_lowercase().replace('_', "-");
    match normalized.as_str() {
        "intfloat/multilingual-e5-small" | "multilingual-e5-small" | "e5-small" => {
            Ok(EmbeddingModel::MultilingualE5Small)
        }
        "intfloat/multilingual-e5-base" | "multilingual-e5-base" | "e5-base" => {
            Ok(EmbeddingModel::MultilingualE5Base)
        }
        "intfloat/multilingual-e5-large" | "multilingual-e5-large" | "e5-large" => {
            Ok(EmbeddingModel::MultilingualE5Large)
        }
        "baai/bge-m3" | "bge-m3" => Ok(EmbeddingModel::BGEM3),
        "sentence-transformers/all-minilm-l6-v2" | "all-minilm-l6-v2" => {
            Ok(EmbeddingModel::AllMiniLML6V2)
        }
        "sentence-transformers/all-minilm-l12-v2" | "all-minilm-l12-v2" => {
            Ok(EmbeddingModel::AllMiniLML12V2)
        }
        "sentence-transformers/all-mpnet-base-v2" | "all-mpnet-base-v2" => {
            Ok(EmbeddingModel::AllMpnetBaseV2)
        }
        other => Err(anyhow!(
            "unsupported embedding model '{other}'. \
             Supported: intfloat/multilingual-e5-base, intfloat/multilingual-e5-large, BAAI/bge-m3"
        )),
    }
}
