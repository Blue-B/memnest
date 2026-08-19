pub mod hybrid;

use anyhow::Result;
use hnsw::distance::Cosine;
use hnsw::persist;
use hnsw::{Builder, Hnsw};
use std::collections::HashSet;
use std::path::Path;
use tantivy::collector::TopDocs;
use tantivy::query::QueryParser;
use tantivy::schema::{Field, STORED, STRING, Schema, TEXT, TantivyDocument, Value};
use tantivy::{Index, IndexReader, IndexWriter, Term, doc};

pub struct VectorIndex {
    dim: usize,
    data_dir: std::path::PathBuf,
    index: Hnsw<Cosine>,
    ids: Vec<String>,
    deleted: HashSet<usize>,
}

impl VectorIndex {
    pub fn persisted(data_dir: &Path) -> bool {
        data_dir.join("vectors").join("index.hnsw").exists()
            && data_dir.join("vectors").join("ids.json").exists()
    }

    pub fn new(data_dir: &Path) -> Result<Self> {
        let dir = data_dir.join("vectors");
        std::fs::create_dir_all(&dir)?;

        let index_path = dir.join("index.hnsw");
        let ids_path = dir.join("ids.json");
        let deleted_path = dir.join("deleted.json");

        if index_path.exists() {
            let index = persist::load(&index_path, Cosine)
                .map_err(|e| anyhow::anyhow!("failed to load HNSW index: {}", e))?;
            let ids: Vec<String> = if ids_path.exists() {
                let file = std::fs::File::open(&ids_path)?;
                serde_json::from_reader(file).unwrap_or_default()
            } else {
                Vec::new()
            };
            let deleted: HashSet<usize> = if deleted_path.exists() {
                let file = std::fs::File::open(&deleted_path)?;
                serde_json::from_reader(file).unwrap_or_default()
            } else {
                HashSet::new()
            };
            let dim = if ids.is_empty() {
                768
            } else {
                index.dim().unwrap_or(768)
            };
            return Ok(Self {
                dim,
                data_dir: dir,
                index,
                ids,
                deleted,
            });
        }

        Ok(Self {
            dim: 768,
            data_dir: dir,
            index: Builder::new()
                .m(32)
                .ef_construction(400)
                .seed(42)
                .build(Cosine),
            ids: Vec::new(),
            deleted: HashSet::new(),
        })
    }

    pub fn add(&mut self, id: &str, vector: &[f32]) -> Result<()> {
        if vector.is_empty() {
            return Ok(());
        }
        if self.ids.is_empty() {
            self.dim = vector.len();
        }
        anyhow::ensure!(
            vector.len() == self.dim,
            "vector dimension mismatch: expected {}, got {}",
            self.dim,
            vector.len()
        );
        for (idx, existing_id) in self.ids.iter().enumerate() {
            if existing_id == id {
                self.deleted.insert(idx);
            }
        }
        self.index.insert(vector.to_vec());
        self.ids.push(id.to_string());
        Ok(())
    }

    pub fn search(&self, query: &[f32], k: usize) -> Result<Vec<(String, f32)>> {
        if query.is_empty() || self.ids.is_empty() {
            return Ok(vec![]);
        }
        anyhow::ensure!(
            query.len() == self.dim,
            "query dimension mismatch: expected {}, got {}",
            self.dim,
            query.len()
        );
        let ef = (k * 8).max(64);
        let mut out = Vec::new();
        for item in self.index.search(query, k * 3, ef) {
            if let Some(id) = self.ids.get(item.id)
                && !self.deleted.contains(&item.id)
            {
                out.push((id.clone(), item.distance));
                if out.len() >= k {
                    break;
                }
            }
        }
        Ok(out)
    }

    pub fn remove(&mut self, id: &str) -> Result<()> {
        for (idx, existing_id) in self.ids.iter().enumerate() {
            if existing_id == id {
                self.deleted.insert(idx);
            }
        }
        Ok(())
    }

    pub fn save(&self) -> Result<()> {
        let index_path = self.data_dir.join("index.hnsw");
        persist::save(&self.index, &index_path)
            .map_err(|e| anyhow::anyhow!("failed to save HNSW index: {}", e))?;
        let ids_path = self.data_dir.join("ids.json");
        let file = std::fs::File::create(&ids_path)?;
        serde_json::to_writer(file, &self.ids)?;
        let deleted_path = self.data_dir.join("deleted.json");
        let file = std::fs::File::create(&deleted_path)?;
        serde_json::to_writer(file, &self.deleted)?;
        Ok(())
    }

    pub fn dim(&self) -> usize {
        self.dim
    }

    pub fn len(&self) -> usize {
        self.ids.len().saturating_sub(self.deleted.len())
    }

    pub fn contains(&self, id: &str) -> bool {
        self.ids
            .iter()
            .enumerate()
            .any(|(index, existing)| existing == id && !self.deleted.contains(&index))
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Rebuild the index from scratch, excluding deleted vectors.
    pub fn rebuild(&mut self, vectors: &[(String, Vec<f32>)]) -> Result<()> {
        self.index = Builder::new()
            .m(32)
            .ef_construction(400)
            .seed(42)
            .build(Cosine);
        self.ids.clear();
        self.deleted.clear();
        for (id, vector) in vectors {
            self.index.insert(vector.clone());
            self.ids.push(id.clone());
        }
        if let Some(first) = vectors.first() {
            self.dim = first.1.len();
        }
        Ok(())
    }
}

pub struct TextIndex {
    _index_dir: std::path::PathBuf,
    index: Index,
    reader: IndexReader,
    id_field: Field,
    project_field: Field,
    body_field: Field,
    writer: Option<std::sync::Mutex<IndexWriter<TantivyDocument>>>,
}

impl TextIndex {
    pub fn persisted(data_dir: &Path) -> bool {
        data_dir.join("text_index").join("meta.json").exists()
    }

    pub fn new(data_dir: &Path) -> Result<Self> {
        let index_dir = data_dir.join("text_index");
        std::fs::create_dir_all(&index_dir)?;

        let index = if index_dir.join("meta.json").exists() {
            match Index::open_in_dir(&index_dir) {
                Ok(index) if text_schema_is_compatible(&index) => index,
                Ok(_) | Err(_) => {
                    std::fs::remove_dir_all(&index_dir)?;
                    std::fs::create_dir_all(&index_dir)?;
                    create_text_index(&index_dir)?
                }
            }
        } else {
            create_text_index(&index_dir)?
        };

        let schema = index.schema();
        let id_field = schema.get_field("id")?;
        let project_field = schema.get_field("project")?;
        let body_field = schema.get_field("body")?;
        let reader = index.reader()?;

        Ok(Self {
            _index_dir: index_dir,
            index,
            reader,
            id_field,
            project_field,
            body_field,
            writer: None,
        })
    }

    fn ensure_writer(&mut self) -> Result<()> {
        if self.writer.is_none() {
            let w = self.index.writer(50_000_000)?;
            self.writer = Some(std::sync::Mutex::new(w));
        }
        Ok(())
    }

    pub fn add(&mut self, id: &str, text: &str) -> Result<()> {
        self.add_with_project(id, "", text)
    }

    pub fn add_with_project(&mut self, id: &str, project: &str, text: &str) -> Result<()> {
        self.add_many_with_project(&[(id.to_string(), project.to_string(), text.to_string())])
    }

    pub fn add_many_with_project(&mut self, docs: &[(String, String, String)]) -> Result<()> {
        if docs.is_empty() {
            return Ok(());
        }
        self.ensure_writer()?;
        let writer = self.writer.as_ref().unwrap();
        let mut w = writer
            .lock()
            .map_err(|_| anyhow::anyhow!("writer lock poisoned"))?;
        for (id, project, text) in docs {
            w.delete_term(Term::from_field_text(self.id_field, id));
            w.add_document(doc!(
                self.id_field => id.as_str(),
                self.project_field => project.as_str(),
                self.body_field => text.as_str(),
            ))?;
        }
        w.commit()?;
        drop(w);
        self.reader.reload()?;
        Ok(())
    }

    pub fn replace_all_with_project(&mut self, docs: &[(String, String, String)]) -> Result<()> {
        // Full rebuild: drop existing writer and create fresh
        self.writer = None;
        let mut w = self.index.writer(50_000_000)?;
        w.delete_all_documents()?;
        for (id, project, text) in docs {
            w.add_document(doc!(
                self.id_field => id.as_str(),
                self.project_field => project.as_str(),
                self.body_field => text.as_str(),
            ))?;
        }
        w.commit()?;
        drop(w);
        self.reader.reload()?;
        Ok(())
    }

    pub fn search(&self, query: &str, k: usize) -> Result<Vec<(String, f32)>> {
        if query.trim().is_empty() {
            return Ok(vec![]);
        }
        let searcher = self.reader.searcher();
        let parser = QueryParser::for_index(&self.index, vec![self.body_field, self.project_field]);
        let parsed = parser.parse_query(query)?;
        let top_docs = searcher.search(&parsed, &TopDocs::with_limit(k).order_by_score())?;

        let mut results = Vec::new();
        for (score, addr) in top_docs {
            let doc: TantivyDocument = searcher.doc(addr)?;
            if let Some(id) = doc.get_first(self.id_field).and_then(|v| v.as_str()) {
                results.push((id.to_string(), score));
            }
        }
        Ok(results)
    }

    pub fn remove(&mut self, id: &str) -> Result<()> {
        self.remove_many(&[id.to_string()])
    }

    pub fn remove_many(&mut self, ids: &[String]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        self.ensure_writer()?;
        let writer = self.writer.as_ref().unwrap();
        let mut w = writer
            .lock()
            .map_err(|_| anyhow::anyhow!("writer lock poisoned"))?;
        for id in ids {
            w.delete_term(Term::from_field_text(self.id_field, id));
        }
        w.commit()?;
        drop(w);
        self.reader.reload()?;
        Ok(())
    }

    pub fn commit(&mut self) -> Result<()> {
        if let Some(writer) = &self.writer {
            let mut w = writer
                .lock()
                .map_err(|_| anyhow::anyhow!("writer lock poisoned"))?;
            w.commit()?;
            drop(w);
            self.reader.reload()?;
        }
        Ok(())
    }
}

fn create_text_index(index_dir: &Path) -> Result<Index> {
    let mut schema_builder = Schema::builder();
    schema_builder.add_text_field("id", STRING | STORED);
    schema_builder.add_text_field("project", STRING | STORED);
    schema_builder.add_text_field("body", TEXT | STORED);
    let schema = schema_builder.build();
    Index::create_in_dir(index_dir, schema).map_err(Into::into)
}

fn text_schema_is_compatible(index: &Index) -> bool {
    let schema = index.schema();
    schema.get_field("id").is_ok()
        && schema.get_field("project").is_ok()
        && schema.get_field("body").is_ok()
}

#[cfg(test)]
mod tests {
    use super::{TextIndex, VectorIndex};

    #[test]
    fn vector_index_survives_reopen() {
        let temp = tempfile::tempdir().unwrap();
        {
            let mut index = VectorIndex::new(temp.path()).unwrap();
            index.add("alpha", &[1.0, 0.0, 0.0]).unwrap();
            index.add("beta", &[0.0, 1.0, 0.0]).unwrap();
            index.save().unwrap();
        }

        let index = VectorIndex::new(temp.path()).unwrap();
        let results = index.search(&[1.0, 0.0, 0.0], 1).unwrap();
        assert_eq!(results.first().map(|(id, _)| id.as_str()), Some("alpha"));
    }

    #[test]
    fn text_index_survives_reopen() {
        let temp = tempfile::tempdir().unwrap();
        {
            let mut index = TextIndex::new(temp.path()).unwrap();
            index
                .add_with_project("alpha", "workspace-a", "durable lexical marker")
                .unwrap();
            index
                .add_with_project("beta", "workspace-b", "unrelated content")
                .unwrap();
            index.commit().unwrap();
        }

        let index = TextIndex::new(temp.path()).unwrap();
        let results = index.search("durable", 3).unwrap();
        assert_eq!(results.first().map(|(id, _)| id.as_str()), Some("alpha"));
    }

    #[test]
    fn text_index_recreates_incompatible_schema() {
        let temp = tempfile::tempdir().unwrap();
        let index_dir = temp.path().join("text_index");
        std::fs::create_dir_all(&index_dir).unwrap();
        let mut schema_builder = tantivy::schema::Schema::builder();
        schema_builder.add_text_field("id", tantivy::schema::STRING | tantivy::schema::STORED);
        schema_builder.add_text_field("body", tantivy::schema::TEXT | tantivy::schema::STORED);
        let old_schema = schema_builder.build();
        tantivy::Index::create_in_dir(&index_dir, old_schema).unwrap();

        let mut index = TextIndex::new(temp.path()).unwrap();
        index
            .add_with_project("alpha", "workspace-a", "schema migration marker")
            .unwrap();
        let results = index.search("migration", 3).unwrap();
        assert_eq!(results.first().map(|(id, _)| id.as_str()), Some("alpha"));
    }
}
