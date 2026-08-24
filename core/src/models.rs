use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Buckets that hold soft-deleted (`_trash`) or replaced (`_superseded`) rows.
/// They stay addressable by id for restore, but must never appear in a count,
/// listing, collection view, or search scope shown to a user.
pub const INTERNAL_PROJECTS: &[&str] = &["_trash", "_superseded"];

/// SQL predicate selecting only user-visible chunks. Kept next to
/// [`INTERNAL_PROJECTS`] so the storage layer and the API filter agree; the
/// `internal_bucket_filters_agree` test proves they stay in sync.
pub const VISIBLE_CHUNKS_SQL: &str = "project NOT IN ('_trash','_superseded')";

pub fn is_internal_project(project: &str) -> bool {
    INTERNAL_PROJECTS.contains(&project.trim())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryChunk {
    pub id: String,
    pub project: String,
    pub document: String,
    pub embedding: Option<Vec<f32>>,
    pub metadata: Metadata,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Metadata {
    #[serde(default)]
    pub chunk_type: ChunkType,
    #[serde(default)]
    pub importance: Importance,
    /// Optional semantic category (failure/correction/insight/...).
    /// Stored in the metadata JSON blob; `#[serde(default)]` keeps legacy rows
    /// (written before this field existed) deserializing as `General`.
    #[serde(default)]
    pub category: MemoryCategory,
    #[serde(default)]
    pub session_id: String,
    /// Absolute working directory of the client that produced this chunk.
    /// Lets us distinguish e.g. `/mnt/c/Users/root/projA` from `/home/x/projA`
    /// (both would collapse to `projA` as project basename).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// Legacy session-lineage metadata retained for row and transcript compatibility.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Host integration that produced this memory (`pi`, `claude-code`,
    /// `codex`, `generic-http`, ...). Kept separate from `source` so adapters
    /// can identify themselves without changing the semantic source label.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adapter_version: Option<String>,
    /// Product-facing semantic kind. Legacy rows default to `record`.
    #[serde(default)]
    pub memory_kind: MemoryKind,
    /// Optional confidence assigned by an importer.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    /// Provenance and replacement links used by structured memory workflows.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total: Option<i64>,
    #[serde(default)]
    pub truncated: bool,
    #[serde(default)]
    pub raw_chunk: Option<String>,
    #[serde(default)]
    pub access_count: i64,
    #[serde(default)]
    pub keywords: Vec<String>,
    /// Legacy schema field. Public memory operations reject true; sensitive
    /// values belong in the encrypted secret vault.
    #[serde(default)]
    pub sensitive: bool,
    /// When true, automatic TTL expiry is suppressed for this chunk regardless
    /// of its age or chunk_type. Pinned chunks must be deleted explicitly.
    #[serde(default)]
    pub pinned: bool,
    /// Set when a chunk is soft-deleted to `_trash`. Holds the project it
    /// originally belonged to so `/restore` can move it back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub original_project: Option<String>,
    /// RFC 3339 timestamp of when the chunk was moved to `_trash`.
    /// The trash GC hard-deletes rows whose `trashed_at` is older than 30 days.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trashed_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChunkType {
    #[default]
    AutoLog,
    Manual,
    Filtered,
    Consolidated,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Importance {
    #[default]
    Log,
    Knowledge,
    Decision,
    Preference,
}

/// Semantic category supplied by clients. The engine stores and returns it
/// without classifying memory content.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryCategory {
    #[default]
    General,
    Failure,
    Correction,
    Insight,
    Preference,
    Convention,
    ToolQuirk,
}

/// Stable cross-platform memory kinds. Adapters may classify memories, but the
/// core never requires a specific agent runtime.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    #[default]
    Record,
    Fact,
    Rule,
    Procedure,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub project: String,
    pub session_id: String,
    pub summary: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fact {
    pub id: String,
    pub subject: String,
    pub predicate: String,
    pub object: String,
    pub timestamp: DateTime<Utc>,
    pub source_session: Option<String>,
    pub history: Vec<FactHistory>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactHistory {
    pub object: String,
    pub timestamp: DateTime<Utc>,
    pub source_session: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    pub host: String,
    pub user: String,
    #[serde(skip_serializing)]
    pub password: String,
    pub port: u16,
    pub ssh_cmd: String,
    pub scp_cmd: String,
    pub note: String,
    pub project_path: Option<String>,
    pub updated: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub key: String,
    pub value: String,
    pub updated: DateTime<Utc>,
    pub prev: Option<NotePrev>,
}

/// Encrypted secret entry (PAT, API key, password, etc.).
/// `value` is always stored AES-GCM encrypted on disk; `get_secret` returns
/// plaintext after automatic decryption. `kind` is a free-form classifier
/// ("github_pat", "openai_key", "ssh_pass", ...) used for filtering only.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Secret {
    pub key: String,
    #[serde(default)]
    pub kind: String,
    pub value: String,
    #[serde(default)]
    pub note: String,
    pub updated: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotePrev {
    pub value: String,
    pub date: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub chunk: MemoryChunk,
    pub vector_score: f32,
    pub text_score: f32,
    pub combined_score: f32,
    pub recency_penalty: f32,
    pub type_bonus: f32,
    pub importance_bonus: f32,
    pub keyword_bonus: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallEvent {
    pub id: String,
    pub query: String,
    pub project: String,
    pub result_ids: Vec<String>,
    pub duration_ms: i64,
    pub adapter: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessingJob {
    pub id: String,
    pub operation: String,
    pub target_id: String,
    pub state: String,
    pub canonical_id: Option<String>,
    pub adapter: String,
    pub error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OperationsSummary {
    pub recalls_24h: usize,
    pub queued_jobs: usize,
    pub running_jobs: usize,
    pub failed_jobs: usize,
    pub average_recall_ms_24h: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectionStat {
    pub name: String,
    pub chunk_count: usize,
    /// Manual chunks (`chunk_type == "manual"`). High-signal, human-curated.
    #[serde(default)]
    pub manual_count: usize,
    /// Autolog chunks (`chunk_type == "auto_log"`). Tool-call dumps, noisy.
    #[serde(default)]
    pub autolog_count: usize,
    /// Collection kind — drives viewer color/icon and write policy.
    /// Possible values: `playbook` | `project` | `autolog` | `archive`.
    #[serde(default = "default_kind")]
    pub kind: String,
    /// Free-form description shown in the viewer card.
    #[serde(default)]
    pub description: String,
    /// Sum of document byte lengths for all chunks in this collection.
    #[serde(default)]
    pub text_bytes: u64,
}

fn default_kind() -> String {
    "project".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_bucket_filters_agree() {
        for project in INTERNAL_PROJECTS {
            assert!(is_internal_project(project));
            assert!(
                VISIBLE_CHUNKS_SQL.contains(&format!("'{project}'")),
                "{project} missing from VISIBLE_CHUNKS_SQL"
            );
        }
        assert!(!is_internal_project("root"));
        assert!(!is_internal_project("playbook"));
    }

    #[test]
    fn metadata_missing_pinned_deserializes_as_false() {
        // Old DB rows stored before the `pinned` field existed must still load.
        let json = r#"{"chunk_type":"auto_log","importance":"log","category":"general","session_id":"","truncated":false,"access_count":0,"keywords":[],"sensitive":false}"#;
        let meta: Metadata = serde_json::from_str(json).expect("deserialize");
        assert!(!meta.pinned, "pinned must default to false for legacy rows");
        assert_eq!(meta.memory_kind, MemoryKind::Record);
    }

    #[test]
    fn metadata_pinned_true_round_trips() {
        let mut meta = Metadata::default();
        meta.pinned = true;
        let json = serde_json::to_string(&meta).unwrap();
        let meta2: Metadata = serde_json::from_str(&json).unwrap();
        assert!(meta2.pinned);
    }
}
