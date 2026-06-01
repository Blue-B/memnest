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
        "프로젝트 구분 없는 수동 저장소. 교훈·선호·결정을 어디서든 검색한다.",
    ),
    (
        "root",
        "project",
        "프로젝트 cwd를 판별할 수 없을 때 쓰이는 루트 버킷. 도구 호출 로그가 이곳으로 온다.",
    ),
    ("default", "project", "cwd 메타가 아예 없을 때의 폴백."),
    (
        "global",
        "project",
        "구버전 자리. 이제 교훈은 playbook으로.",
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
                COALESCE(m.description, '')                                      AS description
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
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(|e| e.into())
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

    pub fn get_chunk(&self, id: &str) -> Result<Option<MemoryChunk>> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT id, project, document, embedding, metadata, created_at, updated_at
             FROM chunks WHERE id = ?1",
        )?;
        let mut rows = stmt.query(params![id])?;
        if let Some(row) = rows.next()? {
            Ok(Some(self.row_to_chunk(row)?))
        } else {
            Ok(None)
        }
    }

    pub fn delete_chunk(&self, id: &str) -> Result<bool> {
        let conn = self.pool.get()?;
        let affected = conn.execute("DELETE FROM chunks WHERE id = ?1", params![id])?;
        Ok(affected > 0)
    }

    /// Returns the id of any existing chunk whose `document` exactly matches
    /// (after trimming) within the same project. Used by `memory_add` to skip
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
                password: crate::crypto::decrypt(&encrypted_password).unwrap_or(encrypted_password),
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
        let result = stmt
            .query_row(params![name], |row| {
                let encrypted_password: String = row.get(3)?;
                Ok(ServerInfo {
                    name: row.get(0)?,
                    host: row.get(1)?,
                    user: row.get(2)?,
                    password: crate::crypto::decrypt(&encrypted_password)
                        .unwrap_or(encrypted_password),
                    port: row.get(4)?,
                    ssh_cmd: row.get(5)?,
                    scp_cmd: row.get(6)?,
                    note: row.get(7)?,
                    project_path: row.get(8)?,
                    updated: row.get(9)?,
                })
            })
            .optional()?;
        Ok(result)
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
        let result = stmt
            .query_row(params![key], |row| {
                let encrypted: String = row.get(2)?;
                Ok(Secret {
                    key: row.get(0)?,
                    kind: row.get(1)?,
                    value: crate::crypto::decrypt(&encrypted).unwrap_or(encrypted),
                    note: row.get(3)?,
                    updated: row.get(4)?,
                })
            })
            .optional()?;
        Ok(result)
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
}
