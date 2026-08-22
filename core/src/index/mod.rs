pub mod hybrid;

use anyhow::Result;
use hnsw::distance::Cosine;
use hnsw::persist;
use hnsw::{Builder, Hnsw};
use std::collections::HashSet;
use std::io::Write;
use std::path::Path;
use tantivy::collector::TopDocs;
use tantivy::query::{BooleanQuery, Occur, Query, QueryParser, TermQuery};
use tantivy::schema::{
    Field, IndexRecordOption, STORED, STRING, Schema, TEXT, TantivyDocument, Value,
};
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
            anyhow::ensure!(ids_path.exists(), "HNSW ids sidecar is missing");
            let index = persist::load(&index_path, Cosine)
                .map_err(|e| anyhow::anyhow!("failed to load HNSW index: {}", e))?;
            let ids: Vec<String> = serde_json::from_reader(std::fs::File::open(&ids_path)?)?;
            anyhow::ensure!(
                ids.len() == index.len(),
                "HNSW ids sidecar has {} entries for {} vectors",
                ids.len(),
                index.len()
            );
            let deleted: HashSet<usize> = if deleted_path.exists() {
                serde_json::from_reader(std::fs::File::open(&deleted_path)?)?
            } else {
                HashSet::new()
            };
            anyhow::ensure!(
                deleted.iter().all(|index| *index < ids.len()),
                "HNSW deleted sidecar contains an invalid offset"
            );
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

        Self::fresh(data_dir)
    }

    pub fn fresh(data_dir: &Path) -> Result<Self> {
        let dir = data_dir.join("vectors");
        std::fs::create_dir_all(&dir)?;
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
        let parent = self
            .data_dir
            .parent()
            .ok_or_else(|| anyhow::anyhow!("vector index has no parent directory"))?;
        let staging = parent.join(format!(
            ".vectors.staging-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&staging)?;
        let result = (|| {
            let index_path = staging.join("index.hnsw");
            persist::save(&self.index, &index_path)
                .map_err(|e| anyhow::anyhow!("failed to save HNSW index: {}", e))?;
            std::fs::File::open(index_path)?.sync_all()?;
            write_json_synced(&staging.join("ids.json"), &self.ids)?;
            write_json_synced(&staging.join("deleted.json"), &self.deleted)?;
            replace_directory(&staging, &self.data_dir)
        })();
        if staging.exists() {
            let _ = std::fs::remove_dir_all(&staging);
        }
        result
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
            let index = Index::open_in_dir(&index_dir)?;
            anyhow::ensure!(
                text_schema_is_compatible(&index),
                "Tantivy index schema is incompatible"
            );
            index
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

    pub fn rebuild_atomic(data_dir: &Path, docs: &[(String, String, String)]) -> Result<()> {
        let target = data_dir.join("text_index");
        let staging = data_dir.join(format!(
            ".text_index.staging-{}",
            uuid::Uuid::new_v4().simple()
        ));
        std::fs::create_dir_all(&staging)?;
        let result = (|| {
            let index = create_text_index(&staging)?;
            let schema = index.schema();
            let id_field = schema.get_field("id")?;
            let project_field = schema.get_field("project")?;
            let body_field = schema.get_field("body")?;
            let mut writer = index.writer(50_000_000)?;
            for (id, project, text) in docs {
                writer.add_document(doc!(
                    id_field => id.as_str(),
                    project_field => project.as_str(),
                    body_field => text.as_str(),
                ))?;
            }
            writer.commit()?;
            drop(writer);
            replace_directory(&staging, &target)
        })();
        if staging.exists() {
            let _ = std::fs::remove_dir_all(&staging);
        }
        result
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
        self.search_projects(query, &[], k)
    }

    pub fn search_projects(
        &self,
        query: &str,
        projects: &[String],
        k: usize,
    ) -> Result<Vec<(String, f32)>> {
        if query.trim().is_empty() || k == 0 {
            return Ok(vec![]);
        }
        let searcher = self.reader.searcher();
        let parser = QueryParser::for_index(&self.index, vec![self.body_field]);
        let body = parser.parse_query(query)?;
        let scoped: Box<dyn Query> = if projects.is_empty() {
            body
        } else {
            let project_terms = projects
                .iter()
                .map(|project| {
                    (
                        Occur::Should,
                        Box::new(TermQuery::new(
                            Term::from_field_text(self.project_field, project),
                            IndexRecordOption::Basic,
                        )) as Box<dyn Query>,
                    )
                })
                .collect();
            Box::new(BooleanQuery::new(vec![
                (Occur::Must, body),
                (Occur::Must, Box::new(BooleanQuery::new(project_terms))),
            ]))
        };
        let top_docs = searcher.search(&scoped, &TopDocs::with_limit(k).order_by_score())?;

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

fn write_json_synced<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    let mut file = std::fs::File::create(path)?;
    serde_json::to_writer(&mut file, value)?;
    file.flush()?;
    file.sync_all()?;
    Ok(())
}

fn replace_directory(staging: &Path, target: &Path) -> Result<()> {
    let parent = target
        .parent()
        .ok_or_else(|| anyhow::anyhow!("index directory has no parent"))?;
    let backup = parent.join(format!(
        ".{}.backup-{}",
        target
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("index"),
        uuid::Uuid::new_v4().simple()
    ));
    let had_target = target.exists();
    if had_target {
        std::fs::rename(target, &backup)?;
    }
    if let Err(error) = std::fs::rename(staging, target) {
        if had_target {
            let _ = std::fs::rename(&backup, target);
        }
        return Err(error.into());
    }
    if had_target {
        std::fs::remove_dir_all(backup)?;
    }
    Ok(())
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
    fn vector_index_rejects_mismatched_sidecar() {
        let temp = tempfile::tempdir().unwrap();
        let mut index = VectorIndex::new(temp.path()).unwrap();
        index.add("alpha", &[1.0, 0.0, 0.0]).unwrap();
        index.save().unwrap();
        std::fs::write(temp.path().join("vectors/ids.json"), "[]").unwrap();
        assert!(VectorIndex::new(temp.path()).is_err());
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
    fn text_search_applies_project_filter_before_ranking() {
        let temp = tempfile::tempdir().unwrap();
        let mut index = TextIndex::new(temp.path()).unwrap();
        index
            .add_with_project("target", "workspace-a", "needle scoped result")
            .unwrap();
        for number in 0..30 {
            index
                .add_with_project(
                    &format!("noise-{number}"),
                    "workspace-b",
                    "needle scoped result repeated repeated",
                )
                .unwrap();
        }

        let projects = vec!["workspace-a".to_string()];
        let results = index
            .search_projects("needle scoped result", &projects, 1)
            .unwrap();
        assert_eq!(results.first().map(|(id, _)| id.as_str()), Some("target"));
    }

    #[test]
    fn text_index_rebuilds_incompatible_schema_without_silent_empty_fallback() {
        let temp = tempfile::tempdir().unwrap();
        let index_dir = temp.path().join("text_index");
        std::fs::create_dir_all(&index_dir).unwrap();
        let mut schema_builder = tantivy::schema::Schema::builder();
        schema_builder.add_text_field("id", tantivy::schema::STRING | tantivy::schema::STORED);
        schema_builder.add_text_field("body", tantivy::schema::TEXT | tantivy::schema::STORED);
        let old_schema = schema_builder.build();
        tantivy::Index::create_in_dir(&index_dir, old_schema).unwrap();

        assert!(TextIndex::new(temp.path()).is_err());
        TextIndex::rebuild_atomic(
            temp.path(),
            &[(
                "alpha".to_string(),
                "workspace-a".to_string(),
                "schema migration marker".to_string(),
            )],
        )
        .unwrap();
        let index = TextIndex::new(temp.path()).unwrap();
        let results = index.search("migration", 3).unwrap();
        assert_eq!(results.first().map(|(id, _)| id.as_str()), Some("alpha"));
    }
}
