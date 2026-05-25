use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
    #[serde(default)]
    pub session_id: String,
    /// Absolute working directory of the client that produced this chunk.
    /// Lets us distinguish e.g. `/mnt/c/Users/root/projA` from `/home/x/projA`
    /// (both would collapse to `projA` as project basename) and is required
    /// for fork-aware reparenting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    /// When a session is forked from another, this records the source session id.
    /// Reparenting (`memory_session_fork`) sets this on every moved chunk so the
    /// origin remains queryable even after the chunks have migrated.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
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
    /// When true, the chunk content is treated as user-confidential.
    /// Redaction is skipped (so secrets remain retrievable) and the document
    /// is stored AES-GCM encrypted at rest.
    #[serde(default)]
    pub sensitive: bool,
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
pub struct GraphNode {
    pub id: String,
    pub depth: usize,
    pub path: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStats {
    pub collections: Vec<CollectionStat>,
    pub total_chunks: usize,
    pub session_summaries: usize,
    pub facts_count: usize,
    pub notes_count: usize,
    pub servers_count: usize,
    pub graph_nodes: usize,
    pub graph_edges: usize,
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
}

fn default_kind() -> String {
    "project".to_string()
}
