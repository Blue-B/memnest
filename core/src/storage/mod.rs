use crate::models::*;
use crate::workspace::WorkspaceIdentity;
use anyhow::Result;
use chrono::Utc;
use half::f16;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{Connection, OptionalExtension, params, params_from_iter};
use serde_json;
use std::path::Path;

/// Seeded collection metadata. INSERT OR IGNORE preserves later user edits.
/// (name, kind, description)
/// Two kinds only:
///   - playbook : cross-project manual notes (lessons, prefs, decisions)
///   - project  : per-cwd bucket for conversation turns captured by `memnest watch`
const DEFAULT_COLLECTION_META: &[(&str, &str, &str)] = &[
    (
        "playbook",
        "playbook",
        "Cross-project manual store. Lessons, preferences, and decisions, searchable from anywhere.",
    ),
    (
        "root",
        "project",
        "Root bucket used when the project cwd cannot be determined. Conversation turns captured by `memnest watch` land here.",
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

/// Classifies a collection that has no `collection_meta` row yet, so legacy
/// collections report a kind without the user labelling each one by hand.
/// The name decides it: there are only two kinds, `playbook` for the shared
/// cross-project bucket and `project` for a per-cwd workspace. An earlier
/// version also took the manual and autolog counts and never read them.
fn infer_collection_kind(name: &str) -> String {
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

            CREATE TABLE IF NOT EXISTS workspaces (
                id TEXT PRIMARY KEY,
                display_name TEXT NOT NULL,
                legacy_project TEXT,
                first_seen_at TEXT NOT NULL,
                last_seen_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_workspaces_legacy
                ON workspaces(legacy_project);

            CREATE TABLE IF NOT EXISTS session_summaries (
                id TEXT PRIMARY KEY,
                project TEXT NOT NULL,
                session_id TEXT NOT NULL,
                summary TEXT NOT NULL,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_summaries_project ON session_summaries(project);
            CREATE INDEX IF NOT EXISTS idx_summaries_created ON session_summaries(created_at);

            CREATE TABLE IF NOT EXISTS secrets (
                key TEXT PRIMARY KEY,
                kind TEXT NOT NULL DEFAULT '',
                value TEXT NOT NULL,
                note TEXT NOT NULL DEFAULT '',
                updated TEXT NOT NULL
            );

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

            -- SQLite is authoritative. Every chunk mutation queues the derived
            -- Tantivy/HNSW change in the same transaction.
            CREATE TABLE IF NOT EXISTS index_queue (
                chunk_id TEXT PRIMARY KEY,
                operation TEXT NOT NULL CHECK (operation IN ('upsert','delete')),
                generation INTEGER NOT NULL DEFAULT 1,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS index_meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            -- Recall history and its feedback table are gone. recall_events
            -- kept the redacted query text of every search for 90 days, which
            -- duplicated the transcript AutoLog that is already searchable.
            -- Dropping the table takes its indexes with it and removes the
            -- last copy of user query text memnest held on disk, so a rebuilt
            -- store and an upgraded one converge on the same schema.
            DROP TABLE IF EXISTS recall_events;
            DROP TABLE IF EXISTS recall_result_feedback;

            -- The facts, servers, and notes tables never had a production
            -- write path: no HTTP route, MCP tool, or CLI command inserted a
            -- row, so every store that exists carries them empty. Dropping
            -- them keeps a rebuilt store and an upgraded one on one schema.
            DROP TABLE IF EXISTS facts;
            DROP TABLE IF EXISTS servers;
            DROP TABLE IF EXISTS notes;
            "#,
        )?;
        migrate_legacy_schema(&conn)?;
        add_column_if_missing(
            &conn,
            "index_queue",
            "generation",
            "ALTER TABLE index_queue ADD COLUMN generation INTEGER NOT NULL DEFAULT 1;",
        )?;
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

        // Operational history is intentionally bounded. It holds safe status
        // metadata only, never queries, memory bodies, or secrets.
        conn.execute(
            "DELETE FROM processing_jobs WHERE datetime(updated_at) < datetime('now', '-90 days')",
            [],
        )?;

        Ok(Self { pool })
    }

    pub fn register_workspace_scope(&self, workspace: &WorkspaceIdentity) -> Result<Vec<String>> {
        let conn = self.pool.get()?;
        let now = Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO workspaces (id, display_name, legacy_project, first_seen_at, last_seen_at)
             VALUES (?1, ?2, ?3, ?4, ?4)
             ON CONFLICT(id) DO UPDATE SET
                 display_name = excluded.display_name,
                 legacy_project = excluded.legacy_project,
                 last_seen_at = excluded.last_seen_at",
            params![
                workspace.id,
                workspace.display_name,
                workspace.legacy_project,
                now
            ],
        )?;

        let mut projects = vec![workspace.id.clone()];
        if let Some(legacy) = &workspace.legacy_project {
            let owners: i64 = conn.query_row(
                "SELECT COUNT(*) FROM workspaces WHERE legacy_project = ?1",
                params![legacy],
                |row| row.get(0),
            )?;
            if owners == 1 {
                projects.push(legacy.clone());
            }
        }
        projects.push("playbook".to_string());
        Ok(projects)
    }

    // ── Chunks ───────────────────────────────────────────────

    pub fn insert_chunk(&self, chunk: &MemoryChunk) -> Result<()> {
        let mut conn = self.pool.get()?;
        let embedding_bytes = chunk
            .embedding
            .as_ref()
            .map(|embedding| encode_embedding(embedding))
            .unwrap_or_default();
        let meta = serde_json::to_string(&chunk.metadata)?;
        let tx = conn.transaction()?;
        tx.execute(
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
        tx.execute(
            "INSERT INTO index_queue (chunk_id, operation, generation, updated_at)
             VALUES (?1, 'upsert', 1, ?2)
             ON CONFLICT(chunk_id) DO UPDATE SET
                 operation = excluded.operation,
                 generation = index_queue.generation + 1,
                 updated_at = excluded.updated_at",
            params![chunk.id, Utc::now().to_rfc3339()],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn insert_superseding_chunk(&self, chunk: &MemoryChunk, superseded_id: &str) -> Result<()> {
        anyhow::ensure!(
            chunk.id != superseded_id,
            "a memory cannot supersede itself"
        );
        let mut conn = self.pool.get()?;
        let embedding_bytes = chunk
            .embedding
            .as_ref()
            .map(|embedding| encode_embedding(embedding))
            .unwrap_or_default();
        let meta = serde_json::to_string(&chunk.metadata)?;
        let tx = conn.transaction()?;
        let previous_project: String = tx.query_row(
            "SELECT project FROM chunks WHERE id = ?1",
            params![superseded_id],
            |row| row.get(0),
        )?;
        anyhow::ensure!(
            previous_project == chunk.project,
            "superseded memory must be active in the same project"
        );
        tx.execute(
            "UPDATE chunks SET project = '_superseded', updated_at = ?2 WHERE id = ?1",
            params![superseded_id, Utc::now().to_rfc3339()],
        )?;
        tx.execute(
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
        for id in [superseded_id, chunk.id.as_str()] {
            tx.execute(
                "INSERT INTO index_queue (chunk_id, operation, generation, updated_at)
                 VALUES (?1, 'upsert', 1, ?2)
                 ON CONFLICT(chunk_id) DO UPDATE SET
                     operation = excluded.operation,
                     generation = index_queue.generation + 1,
                     updated_at = excluded.updated_at",
                params![id, Utc::now().to_rfc3339()],
            )?;
        }
        tx.commit()?;
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

    pub fn get_chunks_by_projects(&self, projects: &[String]) -> Result<Vec<MemoryChunk>> {
        if projects.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.pool.get()?;
        let placeholders = std::iter::repeat_n("?", projects.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT id, project, document, embedding, metadata, created_at, updated_at
             FROM chunks WHERE project IN ({placeholders}) ORDER BY created_at DESC"
        );
        let mut stmt = conn.prepare(&sql)?;
        let mut rows = stmt.query(params_from_iter(projects.iter()))?;
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

    pub fn get_all_chunks_unbounded(&self) -> Result<Vec<MemoryChunk>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, project, document, embedding, metadata, created_at, updated_at
             FROM chunks ORDER BY created_at DESC",
        )?;
        let mut rows = stmt.query([])?;
        let mut chunks = Vec::new();
        while let Some(row) = rows.next()? {
            chunks.push(self.row_to_chunk(row)?);
        }
        Ok(chunks)
    }

    pub fn queue_index_upsert(&self, id: &str) -> Result<()> {
        let conn = self.pool.get()?;
        conn.execute(
            "INSERT INTO index_queue (chunk_id, operation, generation, updated_at)
             VALUES (?1, 'upsert', 1, ?2)
             ON CONFLICT(chunk_id) DO UPDATE SET
                 operation = excluded.operation,
                 generation = index_queue.generation + 1,
                 updated_at = excluded.updated_at",
            params![id, Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }

    pub fn pending_index_ops(&self) -> Result<Vec<(String, String, i64)>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT chunk_id, operation, generation
             FROM index_queue ORDER BY updated_at, chunk_id",
        )?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn index_rebuild_required(&self) -> Result<bool> {
        let conn = self.pool.get()?;
        let version: Option<String> = conn
            .query_row(
                "SELECT value FROM index_meta WHERE key = 'schema_version'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        let pending: bool = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM index_queue LIMIT 1)",
            [],
            |row| row.get(0),
        )?;
        Ok(version.as_deref() != Some("1") || pending)
    }

    pub fn complete_index_ops(&self, completed: &[(String, i64)]) -> Result<()> {
        if completed.is_empty() {
            return Ok(());
        }
        let mut conn = self.pool.get()?;
        let tx = conn.transaction()?;
        for (id, generation) in completed {
            tx.execute(
                "DELETE FROM index_queue WHERE chunk_id = ?1 AND generation = ?2",
                params![id, generation],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn complete_index_rebuild(&self) -> Result<()> {
        let mut conn = self.pool.get()?;
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM index_queue", [])?;
        tx.execute(
            "INSERT OR REPLACE INTO index_meta (key, value) VALUES ('schema_version', '1')",
            [],
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Per-collection aggregates. Internal buckets (`_trash`, `_superseded`)
    /// are excluded, so every dashboard/stats surface built on this is
    /// automatically free of soft-deleted rows.
    pub fn collection_stats(&self, limit: usize) -> Result<Vec<CollectionStat>> {
        let conn = self.pool.get()?;
        // Aggregate per-project counts + chunk_type breakdown in a single pass.
        // LEFT JOIN to collection_meta so collections without an explicit meta row
        // still show up (kind defaults to inferred value below).
        let mut stmt = conn.prepare(&format!(
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
             WHERE c.{VISIBLE_CHUNKS_SQL}
             GROUP BY c.project
             ORDER BY chunk_count DESC, c.project ASC
             LIMIT ?1"
        ))?;
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
                infer_collection_kind(&name)
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

    /// Most recently stored user-visible chunks. Soft-deleted rows are excluded.
    pub fn recent_chunks(&self, limit: usize) -> Result<Vec<RecentChunk>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT id, project, document, metadata, created_at
             FROM chunks
             WHERE {VISIBLE_CHUNKS_SQL}
             ORDER BY created_at DESC
             LIMIT ?1"
        ))?;
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
        if affected > 0 {
            tx.execute(
                "INSERT INTO index_queue (chunk_id, operation, generation, updated_at)
                 VALUES (?1, 'delete', 1, ?2)
                 ON CONFLICT(chunk_id) DO UPDATE SET
                     operation = excluded.operation,
                     generation = index_queue.generation + 1,
                     updated_at = excluded.updated_at",
                params![canonical_id, Utc::now().to_rfc3339()],
            )?;
        }
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

    /// Count of user-visible chunks. Soft-deleted (`_trash`) and superseded
    /// rows are never part of the total a user is shown.
    pub fn chunk_count(&self) -> Result<usize> {
        let conn = self.pool.get()?;
        let count: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM chunks WHERE {VISIBLE_CHUNKS_SQL}"),
            [],
            |row| row.get(0),
        )?;
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

    // ── Secrets (encrypted) ──────────────────────────────────

    /// Every ciphertext the vault key must be able to open. The `servers`
    /// table used to contribute `server:{name}` rows here, but nothing ever
    /// wrote one, so the vault is exactly the `secrets` table now.
    pub(crate) fn encrypted_vault_values(&self) -> Result<Vec<(String, String)>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare("SELECT key, value FROM secrets ORDER BY key")?;
        let rows = stmt.query_map([], |row| {
            let key: String = row.get(0)?;
            let value: String = row.get(1)?;
            Ok((format!("secret:{key}"), value))
        })?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(Into::into)
    }

    pub fn insert_secret(&self, secret: &Secret) -> Result<()> {
        let encrypted =
            crate::crypto::encrypt_bound(&format!("secret:{}", secret.key), &secret.value)?;
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
                let value = crate::crypto::decrypt_bound(&format!("secret:{key}"), &encrypted)?;
                Ok(Secret {
                    key,
                    kind,
                    value,
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

    /// Job counts come from storage; search latency comes from process-memory
    /// counters, because no per-search row is written to disk any more.
    pub fn operations_summary(&self) -> Result<OperationsSummary> {
        let conn = self.pool.get()?;
        let latency = crate::search_metrics::snapshot();
        let count_state = |state: &str| -> Result<usize> {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM processing_jobs WHERE state = ?1",
                params![state],
                |row| row.get::<_, i64>(0),
            )? as usize)
        };
        Ok(OperationsSummary {
            searches_since_start: latency.searches as usize,
            average_search_ms: latency.average_ms,
            max_search_ms: latency.max_ms,
            queued_jobs: count_state("queued")?,
            running_jobs: count_state("running")?,
            failed_jobs: count_state("failed")?,
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

    #[tokio::test]
    async fn superseding_insert_hides_old_truth_and_queues_both_indexes() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path()).await.unwrap();
        let old = sample_chunk("service", "port is 8320", Importance::Knowledge);
        let mut new = sample_chunk("service", "port is 9440", Importance::Knowledge);
        new.metadata.supersedes = Some(old.id.clone());
        db.insert_chunk(&old).unwrap();
        db.complete_index_ops(&[(old.id.clone(), 1)]).unwrap();

        db.insert_superseding_chunk(&new, &old.id).unwrap();
        assert_eq!(
            db.get_chunk(&old.id).unwrap().unwrap().project,
            "_superseded"
        );
        assert_eq!(db.get_chunk(&new.id).unwrap().unwrap().project, "service");
        assert_eq!(
            db.pending_index_ops().unwrap(),
            vec![
                (old.id.clone(), "upsert".to_string(), 1),
                (new.id.clone(), "upsert".to_string(), 1),
            ]
        );

        let cross_project = sample_chunk("other", "bad replacement", Importance::Knowledge);
        assert!(
            db.insert_superseding_chunk(&cross_project, &new.id)
                .is_err()
        );
        assert!(db.get_chunk(&cross_project.id).unwrap().is_none());
        assert_eq!(db.get_chunk(&new.id).unwrap().unwrap().project, "service");
    }

    #[tokio::test]
    async fn chunk_mutations_queue_derived_index_work_transactionally() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path()).await.unwrap();
        let mut chunk = sample_chunk("project", "queued index work", Importance::Knowledge);
        let id = chunk.id.clone();

        db.insert_chunk(&chunk).unwrap();
        let first = db.pending_index_ops().unwrap();
        assert_eq!(first, vec![(id.clone(), "upsert".to_string(), 1)]);

        chunk.document = "newer queued index work".to_string();
        db.insert_chunk(&chunk).unwrap();
        db.complete_index_ops(&[(id.clone(), 1)]).unwrap();
        assert_eq!(
            db.pending_index_ops().unwrap(),
            vec![(id.clone(), "upsert".to_string(), 2)]
        );
        db.complete_index_ops(&[(id.clone(), 2)]).unwrap();
        assert!(db.pending_index_ops().unwrap().is_empty());
        db.complete_index_rebuild().unwrap();
        assert!(!db.index_rebuild_required().unwrap());

        assert!(db.delete_chunk(&id).unwrap());
        assert_eq!(
            db.pending_index_ops().unwrap(),
            vec![(id, "delete".to_string(), 1)]
        );
    }

    #[tokio::test]
    async fn workspace_legacy_alias_is_used_only_while_unambiguous() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path()).await.unwrap();
        let first = crate::workspace::identity("/work/client-a/api").unwrap();
        let second = crate::workspace::identity("/personal/api").unwrap();

        assert_eq!(
            db.register_workspace_scope(&first).unwrap(),
            vec![first.id.clone(), "api".to_string(), "playbook".to_string()]
        );
        assert_eq!(
            db.register_workspace_scope(&second).unwrap(),
            vec![second.id.clone(), "playbook".to_string()]
        );
        assert_eq!(
            db.register_workspace_scope(&first).unwrap(),
            vec![first.id, "playbook".to_string()]
        );
    }

    #[tokio::test]
    async fn fresh_database_does_not_create_graph_edges() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path()).await.unwrap();
        drop(db);

        let conn = Connection::open(dir.path().join("memory.db")).unwrap();
        let exists: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'graph_edges')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!exists);
    }

    #[tokio::test]
    async fn legacy_graph_edges_survive_reopen_unchanged() {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("memory.db");
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            r#"
            CREATE TABLE graph_edges (
                source TEXT NOT NULL,
                target TEXT NOT NULL,
                predicate TEXT NOT NULL,
                meta TEXT NOT NULL DEFAULT '{}',
                created_at TEXT NOT NULL,
                PRIMARY KEY (source, target, predicate)
            ) WITHOUT ROWID;
            INSERT INTO graph_edges (source, target, predicate, meta, created_at)
            VALUES ('alpha', 'beta', 'depends_on', '{"weight":1}', '2026-01-02T03:04:05Z');
            "#,
        )
        .unwrap();
        let original_schema: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'graph_edges'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        drop(conn);

        let db = Database::new(dir.path()).await.unwrap();
        drop(db);

        let conn = Connection::open(&db_path).unwrap();
        let reopened_schema: String = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'graph_edges'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let row: (String, String, String, String, String) = conn
            .query_row(
                "SELECT source, target, predicate, meta, created_at FROM graph_edges",
                [],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();
        assert_eq!(reopened_schema, original_schema);
        assert_eq!(
            row,
            (
                "alpha".to_string(),
                "beta".to_string(),
                "depends_on".to_string(),
                r#"{"weight":1}"#.to_string(),
                "2026-01-02T03:04:05Z".to_string(),
            )
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

        assert_eq!(db.chunk_count_by_project("legacy").unwrap(), 1);

        // facts, servers, and notes never had a production write path, so an
        // upgraded store drops them instead of migrating their columns. A
        // rebuilt store and an upgraded one must expose the same table set.
        let conn = db.pool.get().unwrap();
        for table in ["facts", "servers", "notes"] {
            let present: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
                    params![table],
                    |row| row.get(0),
                )
                .unwrap();
            assert_eq!(present, 0, "legacy table {table} should be dropped on open");
        }
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
    async fn operational_jobs_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path()).await.unwrap();
        let chunk = sample_chunk("proj", "useful memory", Importance::Knowledge);
        let chunk_id = chunk.id.clone();
        db.insert_chunk(&chunk).unwrap();
        let now = Utc::now();
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

        // `/stats` is the only surface that reads these rows now, and it reads
        // them as per-state counts, so assert through that summary.
        let summary = db.operations_summary().unwrap();
        assert_eq!(summary.queued_jobs, 0);
        assert_eq!(summary.running_jobs, 0);
        assert_eq!(summary.failed_jobs, 0);

        let state: String = db
            .pool
            .get()
            .unwrap()
            .query_row(
                "SELECT state FROM processing_jobs WHERE id = 'job-test'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(state, "succeeded", "job row must survive the write");
    }

    /// A store written before this change still carries `recall_events` with
    /// user query text in it. Opening it must succeed and must leave no copy
    /// of that text behind.
    #[tokio::test]
    async fn legacy_recall_events_table_is_dropped_on_open() {
        let dir = tempfile::tempdir().unwrap();
        {
            let db = Database::new(dir.path()).await.unwrap();
            let conn = db.pool.get().unwrap();
            conn.execute_batch(
                r#"
                CREATE TABLE recall_events (
                    id TEXT PRIMARY KEY,
                    query TEXT NOT NULL,
                    project TEXT NOT NULL,
                    result_ids TEXT NOT NULL DEFAULT '[]',
                    duration_ms INTEGER NOT NULL DEFAULT 0,
                    adapter TEXT NOT NULL DEFAULT 'http',
                    created_at TEXT NOT NULL
                );
                CREATE INDEX idx_recall_events_created
                    ON recall_events(created_at DESC);
                INSERT INTO recall_events
                    (id, query, project, result_ids, duration_ms, adapter, created_at)
                VALUES
                    ('legacy-1', 'a prompt the user typed', 'proj', '[]', 5, 'http', '2026-01-01T00:00:00Z');
                "#,
            )
            .unwrap();
        }

        let reopened = Database::new(dir.path()).await.unwrap();
        let conn = reopened.pool.get().unwrap();
        let remaining: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master
                 WHERE type = 'table' AND name = 'recall_events'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(remaining, 0, "legacy recall_events must be dropped");

        // Reopening must still work after the drop.
        assert!(reopened.operations_summary().is_ok());
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
    async fn secret_ciphertext_is_bound_to_its_database_key() {
        let dir = tempfile::tempdir().unwrap();
        let db = Database::new(dir.path()).await.unwrap();
        crate::crypto::init_crypto(Some("row-bound-key")).unwrap();
        let now = Utc::now();
        for (key, value) in [("alpha", "value-a"), ("beta", "value-b")] {
            db.insert_secret(&Secret {
                key: key.to_string(),
                kind: String::new(),
                value: value.to_string(),
                note: String::new(),
                updated: now,
            })
            .unwrap();
        }
        let conn = db.pool.get().unwrap();
        let alpha: String = conn
            .query_row("SELECT value FROM secrets WHERE key = 'alpha'", [], |row| {
                row.get(0)
            })
            .unwrap();
        let beta: String = conn
            .query_row("SELECT value FROM secrets WHERE key = 'beta'", [], |row| {
                row.get(0)
            })
            .unwrap();
        conn.execute(
            "UPDATE secrets SET value = ?1 WHERE key = 'alpha'",
            params![beta],
        )
        .unwrap();
        conn.execute(
            "UPDATE secrets SET value = ?1 WHERE key = 'beta'",
            params![alpha],
        )
        .unwrap();
        assert!(db.get_secret("alpha").is_err());
        assert!(db.get_secret("beta").is_err());
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
        assert_eq!(reopened.operations_summary().unwrap().failed_jobs, 1);

        let (state, error): (String, Option<String>) = reopened
            .pool
            .get()
            .unwrap()
            .query_row(
                "SELECT state, error FROM processing_jobs WHERE id = 'interrupted'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(state, "failed");
        assert_eq!(error.as_deref(), Some("interrupted by service restart"));
    }
}
