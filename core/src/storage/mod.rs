use crate::models::*;
use anyhow::Result;
use chrono::Utc;
use half::f16;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{Connection, OptionalExtension, params};
use serde_json;
use std::path::Path;

/// Seeded collection metadata. INSERT OR IGNORE preserves later user edits.
/// (name, kind, description)
/// Two kinds only:
///   - playbook : cross-project manual notes (lessons, prefs, decisions)
///   - project  : per-cwd bucket; tool calls auto-dump here, manual notes welcome
const DEFAULT_COLLECTION_META: &[(&str, &str, &str)] = &[
    (
        "playbook",
        "playbook",
        "Cross-project manual store. Lessons, preferences, and decisions, searchable from anywhere.",
    ),
    (
        "root",
        "project",
        "Root bucket used when the project cwd cannot be determined. Tool-call logs land here.",
    ),
    (
        "default",
        "project",
        "Fallback for writes that carry no cwd metadata at all.",
    ),
    (
        "global",
        "project",
        "Legacy bucket. Lessons now go to playbook.",
    ),
];

/// When a collection has no `collection_meta` row yet, classify it by name +
/// chunk_type distribution. Lets the viewer show sensible defaults for legacy
/// collections without forcing the user to label everything by hand.
fn infer_collection_kind(name: &str, _manual: usize, _autolog: usize) -> String {
    // Only two kinds: playbook (cross-project manual) vs project (per-cwd bucket).
    if name == "playbook" {
        "playbook".to_string()
    } else {
        "project".to_string()
    }
}

#[derive(Debug, Clone)]
pub struct RecentChunk {
    pub id: String,
    pub project: String,
    pub document: String,
    pub created_at: chrono::DateTime<Utc>,
    pub chunk_type: ChunkType,
    pub importance: Importance,
}

pub struct Database {
    pool: Pool<SqliteConnectionManager>,
}

impl Database {
    pub async fn new(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir)?;
        let db_path = data_dir.join("memory.db");
        let manager = SqliteConnectionManager::file(&db_path);
        let pool = Pool::new(manager)?;
        let conn = pool.get()?;

        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA temp_store = MEMORY;
            PRAGMA mmap_size = 30000000000;

            CREATE TABLE IF NOT EXISTS chunks (
                id TEXT PRIMARY KEY,
                project TEXT NOT NULL,
                document TEXT NOT NULL,
                embedding BLOB,
                metadata TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_chunks_project ON chunks(project);
            CREATE INDEX IF NOT EXISTS idx_chunks_created ON chunks(created_at);
            CREATE INDEX IF NOT EXISTS idx_chunks_project_created ON chunks(project, created_at DESC);

            CREATE TABLE IF NOT EXISTS session_summaries (
                id TEXT PRIMARY KEY,
                project TEXT NOT NULL,
                session_id TEXT NOT NULL,
                summary TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_summaries_project ON session_summaries(project);
            CREATE INDEX IF NOT EXISTS idx_summaries_created ON session_summaries(created_at);

            CREATE TABLE IF NOT EXISTS facts (
                id TEXT PRIMARY KEY,
                subject TEXT NOT NULL,
                predicate TEXT NOT NULL,
                object TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                source_session TEXT,
                history TEXT NOT NULL DEFAULT '[]'
            );
            CREATE INDEX IF NOT EXISTS idx_facts_subject ON facts(subject);
            CREATE INDEX IF NOT EXISTS idx_facts_predicate ON facts(predicate);

            CREATE TABLE IF NOT EXISTS servers (
                name TEXT PRIMARY KEY,
                host TEXT NOT NULL,
                user TEXT NOT NULL,
                password TEXT NOT NULL,
                port INTEGER NOT NULL DEFAULT 22,
                ssh_cmd TEXT,
                scp_cmd TEXT,
                note TEXT,
                project_path TEXT,
                updated TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS notes (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated TEXT NOT NULL,
                prev TEXT
            );

            CREATE TABLE IF NOT EXISTS secrets (
                key TEXT PRIMARY KEY,
                kind TEXT NOT NULL DEFAULT '',
                value TEXT NOT NULL,
                note TEXT NOT NULL DEFAULT '',
                updated TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS graph_edges (
                source TEXT NOT NULL,
                target TEXT NOT NULL,
                predicate TEXT NOT NULL,
                meta TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                PRIMARY KEY (source, target, predicate)
            ) WITHOUT ROWID;
            CREATE INDEX IF NOT EXISTS idx_graph_source ON graph_edges(source);
            CREATE INDEX IF NOT EXISTS idx_graph_target ON graph_edges(target);

            -- Per-collection metadata: kind + free-form description shown in the viewer.
            CREATE TABLE IF NOT EXISTS collection_meta (
                name        TEXT PRIMARY KEY,
                kind        TEXT NOT NULL DEFAULT 'project',  -- playbook|project|autolog|archive
                description TEXT NOT NULL DEFAULT '',
                updated_at  TEXT NOT NULL
            );

            -- A queued write may be semantically deduplicated after the caller
            -- receives its id. Aliases keep that public id resolvable and point
            -- it at the canonical memory instead of creating a phantom id.
            CREATE TABLE IF NOT EXISTS memory_aliases (
                alias_id TEXT PRIMARY KEY,
                canonical_id TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_memory_aliases_canonical
                ON memory_aliases(canonical_id);

            CREATE TABLE IF NOT EXISTS processing_jobs (
                id TEXT PRIMARY KEY,
                operation TEXT NOT NULL,
                target_id TEXT NOT NULL,
                state TEXT NOT NULL CHECK (state IN ('queued','running','succeeded','deduplicated','failed')),
                canonical_id TEXT,
                adapter TEXT NOT NULL DEFAULT 'core',
                error TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_processing_jobs_updated
                ON processing_jobs(updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_processing_jobs_state
                ON processing_jobs(state, updated_at DESC);

            CREATE TABLE IF NOT EXISTS recall_events (
                id TEXT PRIMARY KEY,
                query TEXT NOT NULL,
                project TEXT NOT NULL,
                result_ids TEXT NOT NULL DEFAULT '[]',
                duration_ms INTEGER NOT NULL DEFAULT 0,
                adapter TEXT NOT NULL DEFAULT 'http',
                outcome TEXT NOT NULL DEFAULT 'pending'
                    CHECK (outcome IN ('pending','helpful','harmful','ignored')),
                feedback_note TEXT,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_recall_events_created
                ON recall_events(created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_recall_events_outcome
                ON recall_events(outcome, created_at DESC);
            CREATE TABLE IF NOT EXISTS recall_result_feedback (
                recall_id TEXT NOT NULL,
                memory_id TEXT NOT NULL,
                outcome TEXT NOT NULL CHECK (outcome IN ('helpful','harmful','ignored')),
                PRIMARY KEY (recall_id, memory_id)
            );
            "#,
        )?;
        migrate_legacy_schema(&conn)?;
        conn.execute_batch(
            r#"
            CREATE INDEX IF NOT EXISTS idx_chunks_type ON chunks(json_extract(metadata, '$.chunk_type'));
            CREATE INDEX IF NOT EXISTS idx_chunks_session ON chunks(json_extract(metadata, '$.session_id'));
            CREATE INDEX IF NOT EXISTS idx_chunks_cwd ON chunks(json_extract(metadata, '$.cwd'));
            "#,
        )?;

        // Seed well-known collection metadata. INSERT OR IGNORE preserves user edits.
        let now = chrono::Utc::now().to_rfc3339();
        for (name, kind, desc) in DEFAULT_COLLECTION_META {
            conn.execute(
                "INSERT OR IGNORE INTO collection_meta (name, kind, description, updated_at)
                 VALUES (?1, ?2, ?3, ?4)",
                params![name, kind, desc, now],
            )?;
        }

        // Queued/running rows cannot be replayed because memory text is never
        // persisted in telemetry. Mark them failed on restart instead of
        // presenting stale work as active or pretending it completed.
        conn.execute(
            "UPDATE processing_jobs
             SET state = 'failed',
                 error = COALESCE(error, 'interrupted by service restart'),
                 updated_at = ?1
             WHERE state IN ('queued', 'running')",
            params![Utc::now().to_rfc3339()],
        )?;

        // Operational history is intentionally bounded. It contains redacted
        // queries and safe status metadata, never memory bodies or secrets.
        conn.execute(
            "DELETE FROM recall_events WHERE datetime(created_at) < datetime('now', '-90 days')",
            [],
        )?;
        conn.execute(
            "DELETE FROM processing_jobs WHERE datetime(updated_at) < datetime('now', '-90 days')",
            [],
        )?;

        Ok(Self { pool })
    }

    // ── Chunks ───────────────────────────────────────────────

    pub fn insert_chunk(&self, chunk: &MemoryChunk) -> Result<()> {
        let conn = self.pool.get()?;
        let embedding_bytes = chunk
            .embedding
            .as_ref()
            .map(|embedding| encode_embedding(embedding))
            .unwrap_or_default();
        let meta = serde_json::to_string(&chunk.metadata)?;
        conn.execute(
            "INSERT OR REPLACE INTO chunks (id, project, document, embedding, metadata, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                chunk.id,
                chunk.project,
                chunk.document,
                embedding_bytes,
                meta,
                chunk.created_at.to_rfc3339(),
                chunk.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn get_chunks_by_project(&self, project: &str, limit: usize) -> Result<Vec<MemoryChunk>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, project, document, embedding, metadata, created_at, updated_at
             FROM chunks WHERE project = ?1 ORDER BY created_at DESC LIMIT ?2",
        )?;
        let mut rows = stmt.query(params![project, limit as i64])?;
        let mut chunks = Vec::new();
        while let Some(row) = rows.next()? {
            chunks.push(self.row_to_chunk(row)?);
        }
        Ok(chunks)
    }

    pub fn get_all_chunks(&self, limit: usize) -> Result<Vec<MemoryChunk>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, project, document, embedding, metadata, created_at, updated_at
             FROM chunks ORDER BY created_at DESC LIMIT ?1",
        )?;
        let mut rows = stmt.query(params![limit as i64])?;
        let mut chunks = Vec::new();
        while let Some(row) = rows.next()? {
            chunks.push(self.row_to_chunk(row)?);
        }
        Ok(chunks)
    }

    pub fn collection_stats(&self, limit: usize) -> Result<Vec<CollectionStat>> {
        let conn = self.pool.get()?;
        // Aggregate per-project counts + chunk_type breakdown in a single pass.
        // LEFT JOIN to collection_meta so collections without an explicit meta row
        // still show up (kind defaults to inferred value below).
        let mut stmt = conn.prepare(
            "SELECT
                c.project                                                        AS name,
                COUNT(*)                                                         AS chunk_count,
                SUM(CASE WHEN json_extract(c.metadata, '$.chunk_type') = 'manual'   THEN 1 ELSE 0 END) AS manual_count,
                SUM(CASE WHEN json_extract(c.metadata, '$.chunk_type') = 'auto_log' THEN 1 ELSE 0 END) AS autolog_count,
                COALESCE(m.kind,        '')                                      AS kind,
                COALESCE(m.description, '')                                      AS description,
                COALESCE(SUM(LENGTH(c.document)), 0)                             AS text_bytes
             FROM chunks c
             LEFT JOIN collection_meta m ON m.name = c.project
             GROUP BY c.project
             ORDER BY chunk_count DESC, c.project ASC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            let name: String = row.get(0)?;
            let chunk_count = row.get::<_, i64>(1)? as usize;
            let manual_count = row.get::<_, Option<i64>>(2)?.unwrap_or(0) as usize;
            let autolog_count = row.get::<_, Option<i64>>(3)?.unwrap_or(0) as usize;
            let stored_kind: String = row.get(4)?;
            let description: String = row.get(5)?;
            let text_bytes = row.get::<_, i64>(6)? as u64;
            let kind = if !stored_kind.is_empty() {
                stored_kind
            } else {
                infer_collection_kind(&name, manual_count, autolog_count)
            };
            Ok(CollectionStat {
                name,
                chunk_count,
                manual_count,
                autolog_count,
                kind,
                description,
                text_bytes,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.into())
    }

    /// Age bucket counts for the 'root' project (chunks older than 30/90/180 days).
    /// Cutoff strings are RFC3339-formatted UTC timestamps passed from the caller.
    pub fn age_buckets_root(
        &self,
        cut30: &str,
        cut90: &str,
        cut180: &str,
    ) -> Result<(u64, u64, u64)> {
        let conn = self.pool.get()?;
        let over_30d: u64 = conn.query_row(
            "SELECT COUNT(*) FROM chunks WHERE project = 'root' AND created_at < ?1",
            params![cut30],
            |row| row.get::<_, i64>(0),
        )? as u64;
        let over_90d: u64 = conn.query_row(
            "SELECT COUNT(*) FROM chunks WHERE project = 'root' AND created_at < ?1",
            params![cut90],
            |row| row.get::<_, i64>(0),
        )? as u64;
        let over_180d: u64 = conn.query_row(
            "SELECT COUNT(*) FROM chunks WHERE project = 'root' AND created_at < ?1",
            params![cut180],
            |row| row.get::<_, i64>(0),
        )? as u64;
        Ok((over_30d, over_90d, over_180d))
    }

    /// Upsert collection metadata. Used by `PUT /collection/:name/meta`.
    pub fn upsert_collection_meta(
        &self,
        name: &str,
        kind: Option<&str>,
        description: Option<&str>,
    ) -> Result<()> {
        let conn = self.pool.get()?;
        let now = chrono::Utc::now().to_rfc3339();
        // Read existing row so we can do a partial update.
        let existing: Option<(String, String)> = conn
            .query_row(
                "SELECT kind, description FROM collection_meta WHERE name = ?1",
                params![name],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .ok();
        let (cur_kind, cur_desc) =
            existing.unwrap_or_else(|| ("project".to_string(), String::new()));
        let new_kind = kind.map(|k| k.to_string()).unwrap_or(cur_kind);
        let new_desc = description.map(|d| d.to_string()).unwrap_or(cur_desc);
        conn.execute(
            "INSERT INTO collection_meta (name, kind, description, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(name) DO UPDATE SET
                kind        = excluded.kind,
                description = excluded.description,
                updated_at  = excluded.updated_at",
            params![name, new_kind, new_desc, now],
        )?;
        Ok(())
    }

    /// Fetch a single collection's metadata, or None if not yet recorded.
    pub fn get_collection_meta(&self, name: &str) -> Result<Option<(String, String)>> {
        let conn = self.pool.get()?;
        Ok(conn
            .query_row(
                "SELECT kind, description FROM collection_meta WHERE name = ?1",
                params![name],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .ok())
    }

    pub fn recent_chunks(&self, limit: usize) -> Result<Vec<RecentChunk>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, project, document, metadata, created_at
             FROM chunks
             ORDER BY created_at DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            let meta: String = row.get(3)?;
            let metadata: Metadata = serde_json::from_str(&meta).unwrap_or_default();
            Ok(RecentChunk {
                id: row.get(0)?,
                project: row.get(1)?,
                document: row.get(2)?,
                chunk_type: metadata.chunk_type,
                importance: metadata.importance,
                created_at: row.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.into())
    }

    pub fn canonical_chunk_id(&self, id: &str) -> Result<String> {
        let conn = self.pool.get()?;
        Ok(conn
            .query_row(
                "SELECT canonical_id FROM memory_aliases WHERE alias_id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or_else(|| id.to_string()))
    }

    pub fn get_chunk(&self, id: &str) -> Result<Option<MemoryChunk>> {
        let canonical_id = self.canonical_chunk_id(id)?;
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, project, document, embedding, metadata, created_at, updated_at
             FROM chunks WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![canonical_id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(self.row_to_chunk(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn insert_memory_alias(&self, alias_id: &str, canonical_id: &str) -> Result<()> {
        let conn = self.pool.get()?;
        conn.execute(
            "INSERT OR REPLACE INTO memory_aliases (alias_id, canonical_id, created_at)
             VALUES (?1, ?2, ?3)",
            params![alias_id, canonical_id, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn delete_chunk(&self, id: &str) -> Result<bool> {
        let mut conn = self.pool.get()?;
        let tx = conn.transaction()?;
        let canonical_id: String = tx
            .query_row(
                "SELECT canonical_id FROM memory_aliases WHERE alias_id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?
            .unwrap_or_else(|| id.to_string());
        tx.execute(
            "DELETE FROM memory_aliases WHERE alias_id = ?1 OR canonical_id = ?1",
            params![canonical_id],
        )?;
        let affected = tx.execute("DELETE FROM chunks WHERE id = ?1", params![canonical_id])?;
        tx.commit()?;
        Ok(affected > 0)
    }

    /// Move a chunk to `_trash`: sets `project = "_trash"`, records
    /// `original_project` and `trashed_at` in metadata, and upserts via
    /// INSERT OR REPLACE. Returns false if the chunk is not found or is
    /// already in `_trash`.
    pub fn trash_chunk(&self, id: &str, trashed_at: &str) -> Result<bool> {
        let Some(mut chunk) = self.get_chunk(id)? else {
            return Ok(false);
        };
        if chunk.project == "_trash" {
            return Ok(false);
        }
        let original = std::mem::take(&mut chunk.project);
        chunk.project = "_trash".to_string();
        chunk.metadata.original_project = Some(original);
        chunk.metadata.trashed_at = Some(trashed_at.to_string());
        chunk.updated_at = Utc::now();
        self.insert_chunk(&chunk)?;
        Ok(true)
    }

    /// Restore a chunk from `_trash` back to its `original_project`.
    /// Clears `original_project` and `trashed_at` from metadata.
    /// Returns `None` if the chunk is not found or is not in `_trash`.
    pub fn restore_chunk(&self, id: &str) -> Result<Option<MemoryChunk>> {
        let Some(mut chunk) = self.get_chunk(id)? else {
            return Ok(None);
        };
        if chunk.project != "_trash" {
            return Ok(None);
        }
        let original = chunk
            .metadata
            .original_project
            .take()
            .unwrap_or_else(|| "default".to_string());
        chunk.metadata.trashed_at = None;
        chunk.project = original;
        chunk.updated_at = Utc::now();
        self.insert_chunk(&chunk)?;
        Ok(Some(chunk))
    }

    /// Returns the id of any existing chunk whose `document` exactly matches
    /// (after trimming) within the same project. Used by `memory_remember` to skip
    /// trivial duplicates without re-running the embedding model.
    pub fn find_exact_duplicate(&self, project: &str, document: &str) -> Result<Option<String>> {
        let conn = self.pool.get()?;
        let result = conn
            .query_row(
                "SELECT id FROM chunks WHERE project = ?1 AND document = ?2 LIMIT 1",
                params![project, document.trim()],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        Ok(result)
    }

    /// Increment access_count on a chunk and refresh updated_at. Used when a
    /// duplicate insert is suppressed but we still want to mark the existing
    /// chunk as recently relevant (boosts recency, signals it survived dedup).
    pub fn touch_chunk(&self, id: &str) -> Result<()> {
        let conn = self.pool.get()?;
        let now = Utc::now().to_rfc3339();
        // metadata is JSON — increment access_count inline so we don't need to
        // round-trip a full Metadata decode/encode for the hot dedup path.
        conn.execute(
            "UPDATE chunks
             SET metadata = json_set(metadata, '$.access_count',
                 COALESCE(json_extract(metadata, '$.access_count'), 0) + 1),
                 updated_at = ?2
             WHERE id = ?1",
            params![id, now],
        )?;
        Ok(())
    }

    /// Read every chunk tagged with `session_id` in metadata. Used by the
    /// fork preflight (`dry_run`) and any caller that wants to inspect a
    /// session's footprint without rewriting it.
    pub fn get_chunks_by_session(&self, session_id: &str) -> Result<Vec<MemoryChunk>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, project, document, embedding, metadata, created_at, updated_at
             FROM chunks
             WHERE json_extract(metadata, '$.session_id') = ?1
             ORDER BY created_at ASC",
        )?;
        let mut rows = stmt.query(params![session_id])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(self.row_to_chunk(row)?);
        }
        Ok(out)
    }

    /// Reparent every chunk tagged with `from_session_id` onto a new session.
    ///
    /// This is the storage half of `pi --fork`: when the CLI produces a new
    /// session id (and usually a new cwd), every memory chunk that originated
    /// in the source session is rewritten in place to belong to the new
    /// session id, new project bucket (derived from `to_cwd`) and new cwd.
    /// `parent_session_id` is set on each moved chunk so the lineage is still
    /// queryable. Returns `(matched, moved_chunk_ids)`.
    ///
    /// The original chunks are NOT duplicated — fork is treated as a true
    /// migration. Callers that need a copy-on-fork policy should snapshot
    /// before calling.
    pub fn reparent_session(
        &self,
        from_session_id: &str,
        to_session_id: &str,
        to_project: &str,
        to_cwd: &str,
    ) -> Result<Vec<MemoryChunk>> {
        anyhow::ensure!(
            !from_session_id.is_empty(),
            "from_session_id must not be empty"
        );
        anyhow::ensure!(!to_session_id.is_empty(), "to_session_id must not be empty");
        anyhow::ensure!(
            from_session_id != to_session_id,
            "from_session_id and to_session_id must differ"
        );
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, project, document, embedding, metadata, created_at, updated_at
             FROM chunks
             WHERE json_extract(metadata, '$.session_id') = ?1",
        )?;
        let mut rows = stmt.query(params![from_session_id])?;
        let mut moved = Vec::new();
        while let Some(row) = rows.next()? {
            let mut chunk = self.row_to_chunk(row)?;
            // Preserve original session lineage. If the chunk had been forked
            // before, keep the *oldest* known parent rather than overwriting
            // with the immediate predecessor — that's more useful for tracing.
            if chunk.metadata.parent_session_id.is_none() {
                chunk.metadata.parent_session_id = Some(from_session_id.to_string());
            }
            chunk.metadata.session_id = to_session_id.to_string();
            chunk.metadata.cwd = Some(to_cwd.to_string());
            chunk.project = to_project.to_string();
            chunk.updated_at = Utc::now();
            moved.push(chunk);
        }
        drop(rows);
        drop(stmt);
        // Re-insert each migrated chunk under the same primary key — keeps
        // FTS5 / vector index ids stable so callers only need to refresh the
        // text index's project field afterwards.
        for chunk in &moved {
            self.insert_chunk(chunk)?;
        }
        Ok(moved)
    }

    pub fn chunk_count(&self) -> Result<usize> {
        let conn = self.pool.get()?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM chunks", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    pub fn chunk_count_by_project(&self, project: &str) -> Result<usize> {
        let conn = self.pool.get()?;
        let count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM chunks WHERE project = ?1",
            params![project],
            |row| row.get(0),
        )?;
        Ok(count as usize)
    }

    pub fn vacuum(&self) -> Result<()> {
        let conn = self.pool.get()?;
        conn.execute_batch(
            r#"
            PRAGMA wal_checkpoint(TRUNCATE);
            VACUUM;
            PRAGMA optimize;
            "#,
        )?;
        Ok(())
    }

    fn row_to_chunk(&self, row: &rusqlite::Row) -> Result<MemoryChunk> {
        let embedding_bytes: Vec<u8> = row.get(3)?;
        let embedding = decode_embedding(&embedding_bytes)?;
        let meta: String = row.get(4)?;
        let metadata = serde_json::from_str(&meta)?;
        Ok(MemoryChunk {
            id: row.get(0)?,
            project: row.get(1)?,
            document: row.get(2)?,
            embedding,
            metadata,
            created_at: row.get(5)?,
            updated_at: row.get(6)?,
        })
    }

    // ── Session Summaries ────────────────────────────────────

    pub fn insert_summary(&self, summary: &SessionSummary) -> Result<()> {
        let conn = self.pool.get()?;
        conn.execute(
            "INSERT OR REPLACE INTO session_summaries (id, project, session_id, summary, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![summary.id, summary.project, summary.session_id, summary.summary, summary.created_at.to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn get_summaries_by_project(
        &self,
        project: &str,
        limit: usize,
    ) -> Result<Vec<SessionSummary>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, project, session_id, summary, created_at
             FROM session_summaries WHERE project = ?1 ORDER BY created_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![project, limit as i64], |row| {
            Ok(SessionSummary {
                id: row.get(0)?,
                project: row.get(1)?,
                session_id: row.get(2)?,
                summary: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.into())
    }

    pub fn get_summaries(&self, limit: usize) -> Result<Vec<SessionSummary>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, project, session_id, summary, created_at
             FROM session_summaries ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(SessionSummary {
                id: row.get(0)?,
                project: row.get(1)?,
                session_id: row.get(2)?,
                summary: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.into())
    }

    pub fn summary_count(&self) -> Result<usize> {
        let conn = self.pool.get()?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM session_summaries", [], |row| {
            row.get(0)
        })?;
        Ok(count as usize)
    }

    // ── Facts ────────────────────────────────────────────────

    pub fn insert_fact(&self, fact: &Fact) -> Result<()> {
        let conn = self.pool.get()?;
        let history = serde_json::to_string(&fact.history)?;
        conn.execute(
            "INSERT OR REPLACE INTO facts (id, subject, predicate, object, timestamp, source_session, history)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                fact.id,
                fact.subject,
                fact.predicate,
                fact.object,
                fact.timestamp.to_rfc3339(),
                fact.source_session,
                history,
            ],
        )?;
        Ok(())
    }

    pub fn get_facts(&self, limit: usize) -> Result<Vec<Fact>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, subject, predicate, object, timestamp, source_session, history
             FROM facts ORDER BY timestamp DESC LIMIT ?1",
        )?;
        let mut rows = stmt.query(params![limit as i64])?;
        let mut facts = Vec::new();
        while let Some(row) = rows.next()? {
            let history: String = row.get(6)?;
            facts.push(Fact {
                id: row.get(0)?,
                subject: row.get(1)?,
                predicate: row.get(2)?,
                object: row.get(3)?,
                timestamp: row.get(4)?,
                source_session: row.get(5)?,
                history: serde_json::from_str(&history).unwrap_or_default(),
            });
        }
        Ok(facts)
    }

    pub fn get_fact(&self, id: &str) -> Result<Option<Fact>> {
        let conn = self.pool.get()?;
        conn.query_row(
            "SELECT id, subject, predicate, object, timestamp, source_session, history
             FROM facts WHERE id = ?1",
            params![id],
            |row| {
                let history: String = row.get(6)?;
                Ok(Fact {
                    id: row.get(0)?,
                    subject: row.get(1)?,
                    predicate: row.get(2)?,
                    object: row.get(3)?,
                    timestamp: row.get(4)?,
                    source_session: row.get(5)?,
                    history: serde_json::from_str(&history).unwrap_or_default(),
                })
            },
        )
        .optional()
        .map_err(|e| e.into())
    }

    pub fn fact_count(&self) -> Result<usize> {
        let conn = self.pool.get()?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM facts", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    // ── Servers ──────────────────────────────────────────────

    pub fn insert_server(&self, server: &ServerInfo) -> Result<()> {
        let encrypted_password = crate::crypto::encrypt(&server.password)?;
        let conn = self.pool.get()?;
        conn.execute(
            "INSERT OR REPLACE INTO servers (name, host, user, password, port, ssh_cmd, scp_cmd, note, project_path, updated)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                server.name,
                server.host,
                server.user,
                encrypted_password,
                server.port,
                server.ssh_cmd,
                server.scp_cmd,
                server.note,
                server.project_path,
                server.updated.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn get_servers(&self) -> Result<Vec<ServerInfo>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT name, host, user, password, port, ssh_cmd, scp_cmd, note, project_path, updated FROM servers"
        )?;
        let mut rows = stmt.query([])?;
        let mut servers = Vec::new();
        while let Some(row) = rows.next()? {
            let encrypted_password: String = row.get(3)?;
            servers.push(ServerInfo {
                name: row.get(0)?,
                host: row.get(1)?,
                user: row.get(2)?,
                password: crate::crypto::decrypt(&encrypted_password)?,
                port: row.get(4)?,
                ssh_cmd: row.get(5)?,
                scp_cmd: row.get(6)?,
                note: row.get(7)?,
                project_path: row.get(8)?,
                updated: row.get(9)?,
            });
        }
        Ok(servers)
    }

    pub fn get_server(&self, name: &str) -> Result<Option<ServerInfo>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT name, host, user, password, port, ssh_cmd, scp_cmd, note, project_path, updated
             FROM servers WHERE name = ?1",
        )?;
        let result: Option<(
            String,
            String,
            String,
            String,
            u16,
            String,
            String,
            String,
            Option<String>,
            chrono::DateTime<chrono::Utc>,
        )> = stmt
            .query_row(params![name], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                    row.get(9)?,
                ))
            })
            .optional()?;
        result
            .map(
                |(
                    name,
                    host,
                    user,
                    password,
                    port,
                    ssh_cmd,
                    scp_cmd,
                    note,
                    project_path,
                    updated,
                )| {
                    Ok(ServerInfo {
                        name,
                        host,
                        user,
                        password: crate::crypto::decrypt(&password)?,
                        port,
                        ssh_cmd,
                        scp_cmd,
                        note,
                        project_path,
                        updated,
                    })
                },
            )
            .transpose()
    }

    pub fn server_count(&self) -> Result<usize> {
        let conn = self.pool.get()?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM servers", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    // ── Notes ────────────────────────────────────────────────

    pub fn insert_note(&self, note: &Note) -> Result<()> {
        let conn = self.pool.get()?;
        let prev = note
            .prev
            .as_ref()
            .map(|p| serde_json::to_string(p).unwrap());
        conn.execute(
            "INSERT OR REPLACE INTO notes (key, value, updated, prev)
             VALUES (?1, ?2, ?3, ?4)",
            params![note.key, note.value, note.updated.to_rfc3339(), prev],
        )?;
        Ok(())
    }

    pub fn get_notes(&self) -> Result<Vec<Note>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare("SELECT key, value, updated, prev FROM notes")?;
        let mut rows = stmt.query([])?;
        let mut notes = Vec::new();
        while let Some(row) = rows.next()? {
            let prev: Option<String> = row.get(3)?;
            notes.push(Note {
                key: row.get(0)?,
                value: row.get(1)?,
                updated: row.get(2)?,
                prev: prev.and_then(|s| serde_json::from_str(&s).ok()),
            });
        }
        Ok(notes)
    }

    pub fn get_note(&self, key: &str) -> Result<Option<Note>> {
        let conn = self.pool.get()?;
        let mut stmt =
            conn.prepare("SELECT key, value, updated, prev FROM notes WHERE key = ?1")?;
        let result = stmt
            .query_row(params![key], |row| {
                let prev: Option<String> = row.get(3)?;
                Ok(Note {
                    key: row.get(0)?,
                    value: row.get(1)?,
                    updated: row.get(2)?,
                    prev: prev.and_then(|s| serde_json::from_str(&s).ok()),
                })
            })
            .optional()?;
        Ok(result)
    }

    pub fn delete_note(&self, key: &str) -> Result<bool> {
        let conn = self.pool.get()?;
        let affected = conn.execute("DELETE FROM notes WHERE key = ?1", params![key])?;
        Ok(affected > 0)
    }

    pub fn note_count(&self) -> Result<usize> {
        let conn = self.pool.get()?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM notes", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    // ── Secrets (encrypted) ──────────────────────────────────

    pub(crate) fn encrypted_vault_values(&self) -> Result<Vec<String>> {
        let conn = self.pool.get()?;
        let mut values = Vec::new();
        for sql in [
            "SELECT value FROM secrets ORDER BY key",
            "SELECT password FROM servers ORDER BY name",
        ] {
            let mut stmt = conn.prepare(sql)?;
            let rows = stmt.query_map([], |row| row.get(0))?;
            values.extend(rows.collect::<Result<Vec<String>, _>>()?);
        }
        Ok(values)
    }

    pub fn insert_secret(&self, secret: &Secret) -> Result<()> {
        let encrypted = crate::crypto::encrypt(&secret.value)?;
        let conn = self.pool.get()?;
        conn.execute(
            "INSERT OR REPLACE INTO secrets (key, kind, value, note, updated)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                secret.key,
                secret.kind,
                encrypted,
                secret.note,
                secret.updated.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn get_secret(&self, key: &str) -> Result<Option<Secret>> {
        let conn = self.pool.get()?;
        let mut stmt =
            conn.prepare("SELECT key, kind, value, note, updated FROM secrets WHERE key = ?1")?;
        let result: Option<(
            String,
            String,
            String,
            String,
            chrono::DateTime<chrono::Utc>,
        )> = stmt
            .query_row(params![key], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })
            .optional()?;
        result
            .map(|(key, kind, encrypted, note, updated)| {
                Ok(Secret {
                    key,
                    kind,
                    value: crate::crypto::decrypt(&encrypted)?,
                    note,
                    updated,
                })
            })
            .transpose()
    }

    /// Returns secret metadata (key/kind/note/updated) WITHOUT decrypting the value.
    /// Use this for listing — never include `value` in list views.
    pub fn list_secret_meta(&self) -> Result<Vec<Secret>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare("SELECT key, kind, note, updated FROM secrets ORDER BY key")?;
        let mut rows = stmt.query([])?;
        let mut secrets = Vec::new();
        while let Some(row) = rows.next()? {
            secrets.push(Secret {
                key: row.get(0)?,
                kind: row.get(1)?,
                value: String::new(), // never expose ciphertext or plaintext in list
                note: row.get(2)?,
                updated: row.get(3)?,
            });
        }
        Ok(secrets)
    }

    pub fn delete_secret(&self, key: &str) -> Result<bool> {
        let conn = self.pool.get()?;
        let affected = conn.execute("DELETE FROM secrets WHERE key = ?1", params![key])?;
        Ok(affected > 0)
    }

    pub fn secret_count(&self) -> Result<usize> {
        let conn = self.pool.get()?;
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM secrets", [], |row| row.get(0))?;
        Ok(count as usize)
    }

    // ── Graph Edges ──────────────────────────────────────────

    pub fn insert_graph_edge(
        &self,
        source: &str,
        target: &str,
        predicate: &str,
        meta: &str,
    ) -> Result<()> {
        let conn = self.pool.get()?;
        conn.execute(
            "INSERT OR REPLACE INTO graph_edges (source, target, predicate, meta, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![source, target, predicate, meta, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn get_graph_edges(&self) -> Result<Vec<(String, String, String, String)>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare("SELECT source, target, predicate, meta FROM graph_edges")?;
        let mut rows = stmt.query([])?;
        let mut edges = Vec::new();
        while let Some(row) = rows.next()? {
            edges.push((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?));
        }
        Ok(edges)
    }

    pub fn graph_stats(&self) -> Result<(usize, usize)> {
        let conn = self.pool.get()?;
        let node_count: i64 = conn.query_row(
            "SELECT COUNT(DISTINCT source) + COUNT(DISTINCT target) FROM graph_edges",
            [],
            |row| row.get(0),
        )?;
        let edge_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM graph_edges", [], |row| row.get(0))?;
        Ok((node_count as usize, edge_count as usize))
    }

    // ── Operational observability ────────────────────────────

    pub fn upsert_processing_job(&self, job: &ProcessingJob) -> Result<()> {
        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO processing_jobs
             (id, operation, target_id, state, canonical_id, adapter, error, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET
               state = excluded.state,
               canonical_id = excluded.canonical_id,
               adapter = excluded.adapter,
               error = excluded.error,
               updated_at = excluded.updated_at",
            params![
                job.id,
                job.operation,
                job.target_id,
                job.state,
                job.canonical_id,
                job.adapter,
                job.error,
                job.created_at.to_rfc3339(),
                job.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn recent_processing_jobs(&self, limit: usize) -> Result<Vec<ProcessingJob>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, operation, target_id, state, canonical_id, adapter, error, created_at, updated_at
             FROM processing_jobs ORDER BY updated_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(ProcessingJob {
                id: row.get(0)?,
                operation: row.get(1)?,
                target_id: row.get(2)?,
                state: row.get(3)?,
                canonical_id: row.get(4)?,
                adapter: row.get(5)?,
                error: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn insert_recall_event(&self, event: &RecallEvent) -> Result<()> {
        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO recall_events
             (id, query, project, result_ids, duration_ms, adapter, outcome, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                event.id,
                event.query,
                event.project,
                serde_json::to_string(&event.result_ids)?,
                event.duration_ms,
                event.adapter,
                event.outcome,
                event.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn recent_recall_events(&self, limit: usize) -> Result<Vec<RecallEvent>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, query, project, result_ids, duration_ms, adapter, outcome, created_at
             FROM recall_events ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            let ids: String = row.get(3)?;
            Ok(RecallEvent {
                id: row.get(0)?,
                query: row.get(1)?,
                project: row.get(2)?,
                result_ids: serde_json::from_str(&ids).unwrap_or_default(),
                duration_ms: row.get(4)?,
                adapter: row.get(5)?,
                outcome: row.get(6)?,
                created_at: row.get(7)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn set_recall_feedback(
        &self,
        id: &str,
        memory_id: Option<&str>,
        outcome: &str,
        note: Option<&str>,
    ) -> Result<Option<Vec<String>>> {
        let mut conn = self.pool.get()?;
        let tx = conn.transaction()?;
        let event: Option<String> = tx
            .query_row(
                "SELECT result_ids FROM recall_events WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        let Some(ids_json) = event else {
            return Ok(None);
        };
        let result_ids: Vec<String> = serde_json::from_str(&ids_json).unwrap_or_default();
        let ids: Vec<String> = match memory_id {
            Some(memory_id) if result_ids.iter().any(|id| id == memory_id) => {
                vec![memory_id.to_string()]
            }
            Some(memory_id) => {
                return Err(anyhow::anyhow!(
                    "memory {memory_id} was not returned by recall {id}"
                ));
            }
            None => Vec::new(),
        };
        for memory_id in &ids {
            let previous_outcome: Option<String> = tx
                .query_row(
                    "SELECT outcome FROM recall_result_feedback WHERE recall_id = ?1 AND memory_id = ?2",
                    params![id, memory_id],
                    |row| row.get(0),
                )
                .optional()?;
            if previous_outcome.as_deref() == Some(outcome) {
                continue;
            }
            let canonical_id: String = tx
                .query_row(
                    "SELECT canonical_id FROM memory_aliases WHERE alias_id = ?1",
                    params![memory_id],
                    |row| row.get(0),
                )
                .optional()?
                .unwrap_or_else(|| memory_id.clone());
            let metadata_json: Option<String> = tx
                .query_row(
                    "SELECT metadata FROM chunks WHERE id = ?1",
                    params![canonical_id],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(metadata_json) = metadata_json else {
                continue;
            };
            let mut metadata: Metadata = serde_json::from_str(&metadata_json)?;
            match previous_outcome.as_deref() {
                Some("helpful") => metadata.helpful_count = (metadata.helpful_count - 1).max(0),
                Some("harmful") => metadata.harmful_count = (metadata.harmful_count - 1).max(0),
                _ => {}
            }
            match outcome {
                "helpful" => metadata.helpful_count += 1,
                "harmful" => metadata.harmful_count += 1,
                _ => {}
            }
            tx.execute(
                "UPDATE chunks SET metadata = ?2, updated_at = ?3 WHERE id = ?1",
                params![
                    canonical_id,
                    serde_json::to_string(&metadata)?,
                    Utc::now().to_rfc3339()
                ],
            )?;
            tx.execute(
                    "INSERT INTO recall_result_feedback (recall_id, memory_id, outcome) VALUES (?1, ?2, ?3)
                     ON CONFLICT(recall_id, memory_id) DO UPDATE SET outcome = excluded.outcome",
                    params![id, memory_id, outcome],
                )?;
        }
        tx.execute(
            "UPDATE recall_events SET outcome = ?2, feedback_note = ?3 WHERE id = ?1",
            params![id, outcome, note],
        )?;
        tx.commit()?;
        Ok(Some(ids))
    }

    pub fn operations_summary(&self) -> Result<OperationsSummary> {
        let conn = self.pool.get()?;
        let (recalls, helpful, harmful, average): (i64, i64, i64, f64) = conn.query_row(
            "SELECT
               COUNT(*),
               COALESCE(SUM(CASE WHEN outcome = 'helpful' THEN 1 ELSE 0 END), 0),
               COALESCE(SUM(CASE WHEN outcome = 'harmful' THEN 1 ELSE 0 END), 0),
               COALESCE(AVG(duration_ms), 0)
             FROM recall_events
             WHERE datetime(created_at) >= datetime('now', '-24 hours')",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        let count_state = |state: &str| -> Result<usize> {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM processing_jobs WHERE state = ?1",
                params![state],
                |row| row.get::<_, i64>(0),
            )? as usize)
        };
        Ok(OperationsSummary {
            recalls_24h: recalls as usize,
            helpful_24h: helpful as usize,
            harmful_24h: harmful as usize,
            queued_jobs: count_state("queued")?,
            running_jobs: count_state("running")?,
            failed_jobs: count_state("failed")?,
            average_recall_ms_24h: average,
        })
    }
}

fn encode_embedding(vector: &[f32]) -> Vec<u8> {
    if std::env::var("MEMNEST_EMBED_STORAGE").as_deref() == Ok("f32") {
        let mut bytes = Vec::with_capacity(vector.len() * 4);
        for value in vector {
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        return bytes;
    }

    let mut bytes = Vec::with_capacity(4 + vector.len() * 2);
    bytes.extend_from_slice(b"F16\0");
    for value in vector {
        bytes.extend_from_slice(&f16::from_f32(*value).to_bits().to_le_bytes());
    }
    bytes
}

fn decode_embedding(bytes: &[u8]) -> Result<Option<Vec<f32>>> {
    if bytes.is_empty() {
        return Ok(None);
    }

    if bytes.starts_with(b"F16\0") {
        let payload = &bytes[4..];
        let mut vector = Vec::with_capacity(payload.len() / 2);
        for chunk in payload.chunks_exact(2) {
            let bits = u16::from_le_bytes(chunk.try_into()?);
            vector.push(f16::from_bits(bits).to_f32());
        }
        return Ok(Some(vector));
    }

    let mut vector = Vec::with_capacity(bytes.len() / 4);
    for chunk in bytes.chunks_exact(4) {
        vector.push(f32::from_le_bytes(chunk.try_into()?));
    }
    Ok(Some(vector))
}

fn migrate_legacy_schema(conn: &Connection) -> Result<()> {
    add_column_if_missing(
        conn,
        "chunks",
        "embedding",
        "ALTER TABLE chunks ADD COLUMN embedding BLOB",
    )?;
    add_column_if_missing(
        conn,
        "chunks",
        "metadata",
        "ALTER TABLE chunks ADD COLUMN metadata TEXT NOT NULL DEFAULT '{}'",
    )?;
    add_column_if_missing(
        conn,
        "servers",
        "ssh_cmd",
        "ALTER TABLE servers ADD COLUMN ssh_cmd TEXT",
    )?;
    add_column_if_missing(
        conn,
        "servers",
        "scp_cmd",
        "ALTER TABLE servers ADD COLUMN scp_cmd TEXT",
    )?;
    add_column_if_missing(
        conn,
        "servers",
        "note",
        "ALTER TABLE servers ADD COLUMN note TEXT",
    )?;
    add_column_if_missing(
        conn,
        "servers",
        "project_path",
        "ALTER TABLE servers ADD COLUMN project_path TEXT",
    )?;
    add_column_if_missing(
        conn,
        "facts",
        "history",
        "ALTER TABLE facts ADD COLUMN history TEXT NOT NULL DEFAULT '[]'",
    )?;
    add_column_if_missing(
        conn,
        "notes",
        "prev",
        "ALTER TABLE notes ADD COLUMN prev TEXT",
    )?;
    Ok(())
}

fn add_column_if_missing(
    conn: &Connection,
    table: &str,
    column: &str,
    alter_sql: &str,
) -> Result<()> {
    let pragma = format!("PRAGMA table_info({table})");
    let mut stmt = conn.prepare(&pragma)?;
    let mut rows = stmt.query([])?;
    while let Some(row) = rows.next()? {
        let existing: String = row.get(1)?;
        if existing == column {
            return Ok(());
        }
    }
    conn.execute_batch(alter_sql)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn sample_chunk(project: &str, document: &str, importance: Importance) -> MemoryChunk {
        let now = Utc::now();
        MemoryChunk {
            id: Uuid::new_v4().to_string(),
            project: project.to_string(),
            document: document.to_string(),
            embedding: None,
            metadata: Metadata {
                importance,
                ..Default::default()
            },
            created_at: now,
            updated_at: now,
        }
    }

    fn session_chunk(project: &str, session_id: &str, document: &str) -> MemoryChunk {
        let mut c = sample_chunk(project, document, Importance::Knowledge);
        c.metadata.session_id = session_id.to_string();
        c.metadata.cwd = Some(format!("/old/{project}"));
        c
    }

    #[tokio::test]
    async fn reparent_session_moves_chunks_to_new_session_and_project() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path()).await.unwrap();
        let c1 = session_chunk("old-proj", "sess_A", "chunk one");
        let c2 = session_chunk("old-proj", "sess_A", "chunk two");
        // c3 belongs to a different session and must NOT be touched.
        let c3 = session_chunk("old-proj", "sess_B", "unrelated");
        db.insert_chunk(&c1).unwrap();
        db.insert_chunk(&c2).unwrap();
        db.insert_chunk(&c3).unwrap();

        let moved = db
            .reparent_session(
                "sess_A",
                "sess_NEW",
                "new-proj",
                "/mnt/c/Users/root/new-proj",
            )
            .expect("reparent");
        assert_eq!(moved.len(), 2);
        for chunk in &moved {
            assert_eq!(chunk.metadata.session_id, "sess_NEW");
            assert_eq!(chunk.metadata.parent_session_id.as_deref(), Some("sess_A"));
            assert_eq!(
                chunk.metadata.cwd.as_deref(),
                Some("/mnt/c/Users/root/new-proj")
            );
            assert_eq!(chunk.project, "new-proj");
        }

        // Source session must be empty post-move; unrelated session intact.
        assert!(db.get_chunks_by_session("sess_A").unwrap().is_empty());
        let untouched = db.get_chunks_by_session("sess_B").unwrap();
        assert_eq!(untouched.len(), 1);
        assert_eq!(untouched[0].project, "old-proj");
    }

    #[tokio::test]
    async fn reparent_session_preserves_oldest_parent_on_double_fork() {
        // When a chunk is forked a second time, the original ancestor should
        // remain the recorded parent — otherwise lineage gets lost on each hop.
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path()).await.unwrap();
        let c = session_chunk("p1", "sess_root", "original");
        db.insert_chunk(&c).unwrap();

        db.reparent_session("sess_root", "sess_mid", "p2", "/p2")
            .unwrap();
        let moved = db
            .reparent_session("sess_mid", "sess_leaf", "p3", "/p3")
            .unwrap();

        assert_eq!(moved.len(), 1);
        assert_eq!(
            moved[0].metadata.parent_session_id.as_deref(),
            Some("sess_root"),
            "parent should pin to the original root, not the intermediate session"
        );
    }

    #[tokio::test]
    async fn lightweight_dashboard_queries_do_not_require_embedding_decode() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path()).await.unwrap();
        db.insert_chunk(&sample_chunk(
            "alpha",
            "first decision",
            Importance::Decision,
        ))
        .unwrap();
        db.insert_chunk(&sample_chunk("alpha", "second note", Importance::Log))
            .unwrap();
        db.insert_chunk(&sample_chunk("beta", "project fact", Importance::Knowledge))
            .unwrap();

        let stats = db.collection_stats(10).unwrap();
        assert_eq!(stats[0].name, "alpha");
        assert_eq!(stats[0].chunk_count, 2);
        assert_eq!(stats[1].name, "beta");
        assert_eq!(stats[1].chunk_count, 1);

        let recent = db.recent_chunks(2).unwrap();
        assert_eq!(recent.len(), 2);
        assert!(recent.iter().all(|chunk| !chunk.document.is_empty()));
    }

    #[tokio::test]
    async fn get_summaries_returns_all_projects_by_recency() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path()).await.unwrap();
        let now = Utc::now();
        db.insert_summary(&SessionSummary {
            id: "s1".to_string(),
            project: "alpha".to_string(),
            session_id: "one".to_string(),
            summary: "alpha summary".to_string(),
            created_at: now,
        })
        .unwrap();
        db.insert_summary(&SessionSummary {
            id: "s2".to_string(),
            project: "beta".to_string(),
            session_id: "two".to_string(),
            summary: "beta summary".to_string(),
            created_at: now + chrono::TimeDelta::seconds(1),
        })
        .unwrap();

        let summaries = db.get_summaries(10).unwrap();
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].project, "beta");
    }

    #[tokio::test]
    async fn legacy_schema_is_migrated_on_open() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("memory.db");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE chunks (
                id TEXT PRIMARY KEY,
                project TEXT NOT NULL,
                document TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE facts (
                id TEXT PRIMARY KEY,
                subject TEXT NOT NULL,
                predicate TEXT NOT NULL,
                object TEXT NOT NULL,
                timestamp TEXT NOT NULL,
                source_session TEXT
            );
            CREATE TABLE servers (
                name TEXT PRIMARY KEY,
                host TEXT NOT NULL,
                user TEXT NOT NULL,
                password TEXT NOT NULL,
                port INTEGER NOT NULL DEFAULT 22,
                updated TEXT NOT NULL
            );
            CREATE TABLE notes (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated TEXT NOT NULL
            );
            "#,
        )
        .unwrap();
        drop(conn);

        let db = Database::new(dir.path()).await.unwrap();
        db.insert_chunk(&sample_chunk(
            "legacy",
            "migrated chunk",
            Importance::Knowledge,
        ))
        .unwrap();
        db.insert_note(&Note {
            key: "legacy-note".to_string(),
            value: "ok".to_string(),
            updated: Utc::now(),
            prev: None,
        })
        .unwrap();

        assert_eq!(db.chunk_count_by_project("legacy").unwrap(), 1);
        assert_eq!(
            db.get_note("legacy-note").unwrap().unwrap().value,
            "ok".to_string()
        );
    }

    #[tokio::test]
    async fn trash_chunk_preserves_original_project() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path()).await.unwrap();
        let chunk = sample_chunk("my-project", "hello world", Importance::Log);
        let id = chunk.id.clone();
        db.insert_chunk(&chunk).unwrap();

        let now = Utc::now().to_rfc3339();
        let moved = db.trash_chunk(&id, &now).unwrap();
        assert!(moved, "trash_chunk should return true");

        let trashed = db.get_chunk(&id).unwrap().unwrap();
        assert_eq!(trashed.project, "_trash");
        assert_eq!(
            trashed.metadata.original_project.as_deref(),
            Some("my-project")
        );
        assert!(trashed.metadata.trashed_at.is_some());
    }

    #[tokio::test]
    async fn restore_chunk_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path()).await.unwrap();
        let chunk = sample_chunk("my-project", "round trip test", Importance::Log);
        let id = chunk.id.clone();
        db.insert_chunk(&chunk).unwrap();

        let now = Utc::now().to_rfc3339();
        db.trash_chunk(&id, &now).unwrap();

        let restored = db.restore_chunk(&id).unwrap().expect("should restore");
        assert_eq!(restored.project, "my-project");
        assert!(restored.metadata.original_project.is_none());
        assert!(restored.metadata.trashed_at.is_none());

        let in_db = db.get_chunk(&id).unwrap().unwrap();
        assert_eq!(in_db.project, "my-project");
    }

    #[tokio::test]
    async fn trash_chunk_noop_when_already_trashed() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path()).await.unwrap();
        let chunk = sample_chunk("proj", "doc", Importance::Log);
        let id = chunk.id.clone();
        db.insert_chunk(&chunk).unwrap();

        let now = Utc::now().to_rfc3339();
        assert!(db.trash_chunk(&id, &now).unwrap());
        assert!(!db.trash_chunk(&id, &now).unwrap(), "already in trash");
    }

    #[tokio::test]
    async fn semantic_dedup_alias_keeps_queued_id_resolvable() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path()).await.unwrap();
        let chunk = sample_chunk("proj", "canonical memory", Importance::Knowledge);
        let canonical_id = chunk.id.clone();
        db.insert_chunk(&chunk).unwrap();
        db.insert_memory_alias("queued-id", &canonical_id).unwrap();

        let resolved = db.get_chunk("queued-id").unwrap().expect("alias resolves");
        assert_eq!(resolved.id, canonical_id);
        assert_eq!(resolved.document, "canonical memory");
        assert_eq!(db.canonical_chunk_id("queued-id").unwrap(), resolved.id);
        assert!(
            db.trash_chunk("queued-id", &Utc::now().to_rfc3339())
                .unwrap()
        );
        assert_eq!(
            db.get_chunk("queued-id").unwrap().unwrap().project,
            "_trash"
        );
    }

    #[tokio::test]
    async fn operational_events_and_feedback_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path()).await.unwrap();
        let chunk = sample_chunk("proj", "useful memory", Importance::Knowledge);
        let chunk_id = chunk.id.clone();
        db.insert_chunk(&chunk).unwrap();
        let now = Utc::now();
        db.insert_recall_event(&RecallEvent {
            id: "recall-test".to_string(),
            query: "safe query".to_string(),
            project: "proj".to_string(),
            result_ids: vec![chunk_id.clone()],
            duration_ms: 12,
            adapter: "test".to_string(),
            outcome: "pending".to_string(),
            created_at: now,
        })
        .unwrap();
        db.upsert_processing_job(&ProcessingJob {
            id: "job-test".to_string(),
            operation: "embed_and_store".to_string(),
            target_id: chunk_id.clone(),
            state: "succeeded".to_string(),
            canonical_id: None,
            adapter: "test".to_string(),
            error: None,
            created_at: now,
            updated_at: now,
        })
        .unwrap();

        let aggregate = db
            .set_recall_feedback("recall-test", None, "helpful", Some("aggregate only"))
            .unwrap()
            .unwrap();
        assert!(aggregate.is_empty());
        assert_eq!(
            db.get_chunk(&chunk_id)
                .unwrap()
                .unwrap()
                .metadata
                .helpful_count,
            0,
            "aggregate feedback must not change result ranking"
        );

        let ids = db
            .set_recall_feedback("recall-test", Some(&chunk_id), "helpful", Some("worked"))
            .unwrap()
            .unwrap();
        assert_eq!(ids, vec![chunk_id.clone()]);
        db.set_recall_feedback("recall-test", Some(&chunk_id), "helpful", Some("retry"))
            .unwrap();
        assert_eq!(
            db.get_chunk(&chunk_id)
                .unwrap()
                .unwrap()
                .metadata
                .helpful_count,
            1,
            "same feedback must be idempotent"
        );
        db.set_recall_feedback("recall-test", Some(&chunk_id), "harmful", None)
            .unwrap();

        let recalls = db.recent_recall_events(10).unwrap();
        let jobs = db.recent_processing_jobs(10).unwrap();
        let summary = db.operations_summary().unwrap();
        assert_eq!(recalls[0].outcome, "harmful");
        assert_eq!(jobs[0].state, "succeeded");
        assert_eq!(summary.recalls_24h, 1);
        assert_eq!(summary.helpful_24h, 0);
        assert_eq!(summary.harmful_24h, 1);
        assert_eq!(
            db.get_chunk(&chunk_id)
                .unwrap()
                .unwrap()
                .metadata
                .helpful_count,
            0
        );
        assert_eq!(
            db.get_chunk(&chunk_id)
                .unwrap()
                .unwrap()
                .metadata
                .harmful_count,
            1
        );

        db.insert_recall_event(&RecallEvent {
            id: "empty-recall".to_string(),
            query: "no result".to_string(),
            project: "proj".to_string(),
            result_ids: Vec::new(),
            duration_ms: 2,
            adapter: "test".to_string(),
            outcome: "pending".to_string(),
            created_at: now,
        })
        .unwrap();
        assert_eq!(
            db.set_recall_feedback("empty-recall", None, "ignored", None)
                .unwrap(),
            Some(Vec::new())
        );
        assert!(
            db.set_recall_feedback("missing", None, "ignored", None)
                .unwrap()
                .is_none()
        );
    }

    #[tokio::test]
    async fn vault_validation_accepts_empty_database() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path()).await.unwrap();
        crate::crypto::validate_ciphertexts(
            "empty-vault-key",
            &db.encrypted_vault_values().unwrap(),
        )
        .unwrap();
    }

    #[tokio::test]
    async fn vault_validation_accepts_correct_key() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path()).await.unwrap();
        let encrypted = crate::crypto::encrypt_with_master_key("correct-key", "secret").unwrap();
        let conn = db.pool.get().unwrap();
        conn.execute(
            "INSERT INTO secrets (key, kind, value, note, updated) VALUES ('sample', '', ?1, '', ?2)",
            params![encrypted, Utc::now().to_rfc3339()],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO servers (name, host, user, password, updated) VALUES ('sample', 'localhost', 'user', ?1, ?2)",
            params![encrypted, Utc::now().to_rfc3339()],
        )
        .unwrap();
        crate::crypto::validate_ciphertexts("correct-key", &db.encrypted_vault_values().unwrap())
            .unwrap();
    }

    #[tokio::test]
    async fn vault_validation_rejects_wrong_key() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path()).await.unwrap();
        let encrypted = crate::crypto::encrypt_with_master_key("correct-key", "secret").unwrap();
        db.pool
            .get()
            .unwrap()
            .execute(
                "INSERT INTO secrets (key, kind, value, note, updated) VALUES ('sample', '', ?1, '', ?2)",
                params![encrypted, Utc::now().to_rfc3339()],
            )
            .unwrap();
        assert!(
            crate::crypto::validate_ciphertexts(
                "wrong-key",
                &db.encrypted_vault_values().unwrap(),
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn vault_validation_rejects_corrupt_ciphertext() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path()).await.unwrap();
        db.pool
            .get()
            .unwrap()
            .execute(
                "INSERT INTO secrets (key, kind, value, note, updated) VALUES ('sample', '', '$enc$corrupt', '', ?1)",
                params![Utc::now().to_rfc3339()],
            )
            .unwrap();
        assert!(
            crate::crypto::validate_ciphertexts(
                "correct-key",
                &db.encrypted_vault_values().unwrap(),
            )
            .is_err()
        );
    }

    #[tokio::test]
    async fn secret_rows_never_fall_back_to_stored_text() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path()).await.unwrap();
        let conn = db.pool.get().unwrap();
        for (key, value) in [("plain", "plaintext-value"), ("corrupt", "$enc$not-base64")] {
            conn.execute(
                "INSERT INTO secrets (key, kind, value, note, updated) VALUES (?1, '', ?2, '', ?3)",
                params![key, value, Utc::now().to_rfc3339()],
            )
            .unwrap();
            assert!(db.get_secret(key).is_err());
        }
        conn.execute(
            "INSERT INTO servers (name, host, user, password, updated) VALUES ('plain-server', 'localhost', 'user', 'plaintext-password', ?1)",
            params![Utc::now().to_rfc3339()],
        )
        .unwrap();
        assert!(db.get_server("plain-server").is_err());
        assert!(db.get_servers().is_err());
    }

    #[tokio::test]
    async fn startup_marks_interrupted_jobs_failed() {
        let dir = tempfile::tempdir().unwrap();
        {
            let db = Database::new(dir.path()).await.unwrap();
            let now = Utc::now();
            db.upsert_processing_job(&ProcessingJob {
                id: "interrupted".to_string(),
                operation: "embed_and_store".to_string(),
                target_id: "manual-x".to_string(),
                state: "running".to_string(),
                canonical_id: None,
                adapter: "test".to_string(),
                error: None,
                created_at: now,
                updated_at: now,
            })
            .unwrap();
        }
        let reopened = Database::new(dir.path()).await.unwrap();
        let jobs = reopened.recent_processing_jobs(5).unwrap();
        assert_eq!(jobs[0].state, "failed");
        assert_eq!(
            jobs[0].error.as_deref(),
            Some("interrupted by service restart")
        );
    }
}
