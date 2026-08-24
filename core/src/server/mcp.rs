use crate::MemorySystem;
use crate::models::{ChunkType, Importance, Metadata};
use anyhow::Result;
use axum::{
    body::Bytes,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::{Value, json};
use std::io::{self, BufRead, Write};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Revision reported when the client asks for one we do not know. Unchanged from
/// the stdio-only implementation so existing clients see the same handshake.
const DEFAULT_PROTOCOL_VERSION: &str = "2024-11-05";
const SUPPORTED_PROTOCOL_VERSIONS: &[&str] = &["2024-11-05", "2025-03-26", "2025-06-18"];

/// Handle one JSON-RPC message, independent of the transport that carried it.
///
/// Returns `None` for notifications, which take no reply: either an MCP
/// `notifications/*` method or any message sent without an `id`.
pub async fn dispatch(system: Arc<RwLock<MemorySystem>>, req: &Value) -> Option<Value> {
    let method = req.get("method").and_then(Value::as_str).unwrap_or("");
    if method.starts_with("notifications/") {
        return None;
    }
    let id = req.get("id")?.clone();

    let result = match method {
        "initialize" => {
            let version = req
                .get("params")
                .and_then(|p| p.get("protocolVersion"))
                .and_then(Value::as_str)
                .filter(|v| SUPPORTED_PROTOCOL_VERSIONS.contains(v))
                .unwrap_or(DEFAULT_PROTOCOL_VERSION);
            json!({
                "protocolVersion": version,
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "memnest", "version": env!("CARGO_PKG_VERSION")}
            })
        }
        "tools/list" => {
            let enabled = system.read().await.secret_tools_enabled;
            json!({"tools": tools(enabled)})
        }
        "tools/call" => {
            let params = req.get("params").cloned().unwrap_or_default();
            match call_tool(system, &params).await {
                Ok(text) => json!({"content":[{"type":"text","text":text}]}),
                Err(e) => {
                    json!({"content":[{"type":"text","text":format!("error: {}", e)}],"isError":true})
                }
            }
        }
        "shutdown" => Value::Null,
        _ => {
            return Some(json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {"code": -32601, "message": format!("method not found: {}", method)}
            }));
        }
    };
    Some(json!({"jsonrpc":"2.0","id":id,"result":result}))
}

pub async fn run_stdio(system: Arc<RwLock<MemorySystem>>) -> Result<()> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    for line in stdin.lock().lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let req: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(e) => {
                write_response(
                    &mut stdout,
                    json!({"jsonrpc":"2.0","id":null,"error":{"code":-32700,"message":e.to_string()}}),
                )?;
                continue;
            }
        };
        let is_shutdown = req.get("method").and_then(Value::as_str) == Some("shutdown");
        if let Some(response) = dispatch(system.clone(), &req).await {
            write_response(&mut stdout, response)?;
        }
        if is_shutdown {
            break;
        }
    }
    Ok(())
}

/// MCP Streamable HTTP endpoint. Takes one JSON-RPC message or a batch and answers
/// with a single JSON body; a notification-only post gets 202 with no content.
/// No SSE upgrade is offered, which the spec allows for a stateless server.
pub async fn http_endpoint(
    State(system): State<Arc<RwLock<MemorySystem>>>,
    body: Bytes,
) -> Response {
    let parsed: Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(e) => return json_rpc_error(StatusCode::BAD_REQUEST, -32700, e.to_string()),
    };

    let responses = match &parsed {
        Value::Object(_) => dispatch(system, &parsed).await.into_iter().collect(),
        Value::Array(batch) if !batch.is_empty() => {
            let mut out = Vec::new();
            for message in batch {
                if let Some(response) = dispatch(system.clone(), message).await {
                    out.push(response);
                }
            }
            out
        }
        _ => {
            return json_rpc_error(
                StatusCode::BAD_REQUEST,
                -32600,
                "expected a JSON-RPC object or a non-empty batch".to_string(),
            );
        }
    };

    if responses.is_empty() {
        return StatusCode::ACCEPTED.into_response();
    }
    let body = if parsed.is_array() {
        Value::Array(responses)
    } else {
        responses.into_iter().next().unwrap_or(Value::Null)
    };
    (StatusCode::OK, axum::Json(body)).into_response()
}

fn json_rpc_error(status: StatusCode, code: i32, message: String) -> Response {
    (
        status,
        axum::Json(json!({
            "jsonrpc": "2.0",
            "id": Value::Null,
            "error": {"code": code, "message": message}
        })),
    )
        .into_response()
}

fn write_response(stdout: &mut io::Stdout, value: Value) -> Result<()> {
    writeln!(stdout, "{}", serde_json::to_string(&value)?)?;
    stdout.flush()?;
    Ok(())
}

fn tools(crypto_enabled: bool) -> Vec<Value> {
    let mut tools = vec![
        json!({"name":"memory_remember","description":"Save durable memory. Pass project explicitly or cwd for an isolated workspace. Sensitive values are rejected; use secret_set.","inputSchema":{"type":"object","properties":{"text":{"type":"string"},"project":{"type":"string"},"cwd":{"type":"string","description":"Absolute workspace path; used only when project is omitted."},"importance":{"type":"string","enum":["log","knowledge","decision","preference"],"default":"knowledge"},"memory_kind":{"type":"string","enum":["record","fact","rule","procedure"],"default":"record"},"confidence":{"type":"number","minimum":0,"maximum":1},"source_ids":{"type":"array","items":{"type":"string"}},"supersedes":{"type":"string"},"sensitive":{"type":"boolean","description":"Must be false; use secret_set for sensitive values."}},"required":["text"]}}),
        json!({"name":"memory_search","description":"Hybrid memory search. Pass project explicitly or cwd for the isolated workspace plus playbook. project=all is explicit cross-project search.","inputSchema":{"type":"object","properties":{"query":{"type":"string"},"project":{"type":"string"},"cwd":{"type":"string","description":"Absolute workspace path; used only when project is omitted."},"n_results":{"type":"integer","default":3,"minimum":1,"maximum":50},"recent_first":{"type":"boolean","default":false},"category":{"type":"string"}},"required":["query"]}}),
        json!({"name":"memory_get","description":"Fetch one memory by id.","inputSchema":{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}}),
        json!({"name":"memory_update","description":"Update one memory and refresh indexes.","inputSchema":{"type":"object","properties":{"id":{"type":"string"},"text":{"type":"string"},"project":{"type":"string"},"importance":{"type":"string","enum":["log","knowledge","decision","preference"]},"chunk_type":{"type":"string","enum":["auto_log","manual","filtered","consolidated"]},"sensitive":{"type":"boolean","description":"Must be false; use secret_set for sensitive values."}},"required":["id"]}}),
        json!({"name":"memory_delete","description":"Soft-delete one memory to the internal trash bucket.","inputSchema":{"type":"object","properties":{"id":{"type":"string"}},"required":["id"]}}),
    ];
    if crypto_enabled {
        tools.extend([
            json!({"name":"secret_set","description":"Store an AES-256-GCM encrypted credential.","inputSchema":{"type":"object","properties":{"key":{"type":"string"},"value":{"type":"string"},"kind":{"type":"string"},"note":{"type":"string"}},"required":["key","value"]}}),
            json!({"name":"secret_get","description":"Retrieve and decrypt a credential.","inputSchema":{"type":"object","properties":{"key":{"type":"string"}},"required":["key"]}}),
            json!({"name":"secret_list","description":"List credential metadata without values.","inputSchema":{"type":"object","properties":{}}}),
            json!({"name":"secret_delete","description":"Permanently delete a credential.","inputSchema":{"type":"object","properties":{"key":{"type":"string"}},"required":["key"]}}),
        ]);
    }
    tools
}

async fn call_tool(system: Arc<RwLock<MemorySystem>>, params: &Value) -> Result<String> {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let args = params.get("arguments").cloned().unwrap_or_default();
    match name {
        "memory_remember" => memory_remember(system, &args).await,
        "memory_search" => memory_search(system, &args).await,
        "memory_get" => memory_get(system, &args).await,
        "memory_update" => memory_update(system, &args).await,
        "memory_delete" => memory_delete(system, &args).await,
        "secret_set" => secret_set(system, &args).await,
        "secret_get" => secret_get(system, &args).await,
        "secret_list" => secret_list(system).await,
        "secret_delete" => secret_delete(system, &args).await,
        _ => anyhow::bail!("unknown tool: {name}"),
    }
}

async fn require_vault(system: &Arc<RwLock<MemorySystem>>) -> Result<()> {
    anyhow::ensure!(
        system.read().await.secret_tools_enabled && crate::crypto::is_enabled(),
        "secret tools are disabled; set MEMNEST_EXPOSE_SECRET_TOOLS=1"
    );
    Ok(())
}

async fn secret_set(system: Arc<RwLock<MemorySystem>>, args: &Value) -> Result<String> {
    require_vault(&system).await?;
    let key = args.get("key").and_then(Value::as_str).unwrap_or("").trim();
    let value = args.get("value").and_then(Value::as_str).unwrap_or("");
    anyhow::ensure!(!key.is_empty(), "key is required");
    anyhow::ensure!(!value.is_empty(), "value is required");
    let kind = args
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let note = args
        .get("note")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let secret = crate::models::Secret {
        key: key.to_string(),
        kind,
        value: value.to_string(),
        note,
        updated: chrono::Utc::now(),
    };
    let sys = system.read().await;
    sys.db
        .write()
        .await
        .insert_secret(&secret)
        .map_err(|_| anyhow::anyhow!("secret operation failed"))?;
    Ok(format!(
        "secret stored encrypted: {} (encryption={})",
        key,
        if crate::crypto::is_enabled() {
            "AES-256-GCM"
        } else {
            "disabled (no master key)"
        }
    ))
}

async fn secret_get(system: Arc<RwLock<MemorySystem>>, args: &Value) -> Result<String> {
    require_vault(&system).await?;
    let key = args.get("key").and_then(Value::as_str).unwrap_or("").trim();
    anyhow::ensure!(!key.is_empty(), "key is required");
    let sys = system.read().await;
    let db = sys.db.read().await;
    match db
        .get_secret(key)
        .map_err(|_| anyhow::anyhow!("secret operation failed"))?
    {
        Some(secret) => Ok(format!(
            "{}\n--\nkind: {}\nnote: {}\nupdated: {}",
            secret.value,
            secret.kind,
            secret.note,
            secret.updated.to_rfc3339()
        )),
        None => anyhow::bail!("secret not found"),
    }
}

async fn secret_list(system: Arc<RwLock<MemorySystem>>) -> Result<String> {
    require_vault(&system).await?;
    let sys = system.read().await;
    let db = sys.db.read().await;
    let secrets = db
        .list_secret_meta()
        .map_err(|_| anyhow::anyhow!("secret operation failed"))?;
    if secrets.is_empty() {
        return Ok("no secrets stored".to_string());
    }
    let mut lines = vec!["=== secrets (values not shown) ===".to_string()];
    for s in secrets {
        lines.push(format!(
            "  {} [{}] {} (updated {})",
            s.key,
            if s.kind.is_empty() { "-" } else { &s.kind },
            if s.note.is_empty() { "" } else { &s.note },
            s.updated.to_rfc3339()
        ));
    }
    Ok(lines.join("\n"))
}

async fn secret_delete(system: Arc<RwLock<MemorySystem>>, args: &Value) -> Result<String> {
    require_vault(&system).await?;
    let key = args.get("key").and_then(Value::as_str).unwrap_or("").trim();
    anyhow::ensure!(!key.is_empty(), "key is required");
    let sys = system.read().await;
    let removed = sys
        .db
        .write()
        .await
        .delete_secret(key)
        .map_err(|_| anyhow::anyhow!("secret operation failed"))?;
    if removed {
        Ok(format!("secret deleted: {}", key))
    } else {
        anyhow::bail!("secret not found")
    }
}

async fn memory_remember(system: Arc<RwLock<MemorySystem>>, args: &Value) -> Result<String> {
    let mut metadata = Metadata {
        chunk_type: ChunkType::Manual,
        importance: Importance::Knowledge,
        adapter: Some(
            args.get("adapter")
                .and_then(Value::as_str)
                .unwrap_or("mcp")
                .to_string(),
        ),
        ..Default::default()
    };
    if args
        .get("sensitive")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        metadata.sensitive = true;
    }
    if let Some(value) = args.get("importance").and_then(Value::as_str) {
        metadata.importance = parse_importance(value)?;
    }
    if let Some(value) = args.get("memory_kind").cloned() {
        metadata.memory_kind = serde_json::from_value(value)?;
    }
    metadata.confidence = args
        .get("confidence")
        .and_then(Value::as_f64)
        .map(|v| v.clamp(0.0, 1.0) as f32);
    metadata.source_ids = args
        .get("source_ids")
        .and_then(Value::as_array)
        .map(|v| {
            v.iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    metadata.supersedes = args
        .get("supersedes")
        .and_then(Value::as_str)
        .map(str::to_string);
    metadata.cwd = args.get("cwd").and_then(Value::as_str).map(str::to_string);
    let out = super::operations::remember(
        system,
        super::operations::RememberInput {
            text: args
                .get("text")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            project: args
                .get("project")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            cwd: args.get("cwd").and_then(Value::as_str).map(str::to_string),
            metadata: Some(metadata),
            sensitive: args
                .get("sensitive")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        },
    )
    .await?;
    Ok(serde_json::to_string(&out)?)
}

async fn memory_update(system: Arc<RwLock<MemorySystem>>, args: &Value) -> Result<String> {
    let value = args.clone();
    if value
        .get("sensitive")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        anyhow::bail!("sensitive memory is not supported; use secret_set");
    }
    let req: crate::server::api::UpdateRequest = serde_json::from_value(value)?;
    Ok(serde_json::to_string(
        &super::operations::update(system, req).await?,
    )?)
}

async fn memory_delete(system: Arc<RwLock<MemorySystem>>, args: &Value) -> Result<String> {
    let id = args.get("id").and_then(Value::as_str).unwrap_or("");
    Ok(serde_json::to_string(
        &super::operations::delete(system, vec![id.to_string()]).await?,
    )?)
}

fn parse_importance(value: &str) -> Result<Importance> {
    match value {
        "log" => Ok(Importance::Log),
        "knowledge" => Ok(Importance::Knowledge),
        "decision" => Ok(Importance::Decision),
        "preference" => Ok(Importance::Preference),
        other => anyhow::bail!("invalid importance: {other}"),
    }
}

async fn memory_get(system: Arc<RwLock<MemorySystem>>, args: &Value) -> Result<String> {
    let id = args.get("id").and_then(Value::as_str).unwrap_or("");
    let c = super::operations::get(system, id).await?;
    Ok(format!(
        "id={} project={} type={} importance={} created={}\n{}",
        c["id"].as_str().unwrap_or(""),
        c["project"].as_str().unwrap_or(""),
        c["chunk_type"].as_str().unwrap_or(""),
        c["importance"].as_str().unwrap_or(""),
        c["timestamp"].as_str().unwrap_or(""),
        c["document"].as_str().unwrap_or("")
    ))
}

pub(crate) async fn memory_search(
    system: Arc<RwLock<MemorySystem>>,
    args: &Value,
) -> Result<String> {
    let query = args.get("query").and_then(Value::as_str).unwrap_or("");
    let project = args.get("project").and_then(Value::as_str).unwrap_or("");
    let n = args.get("n_results").and_then(Value::as_u64).unwrap_or(3) as usize;
    let out = super::operations::search(
        system,
        super::operations::SearchInput {
            query: query.to_string(),
            project: project.to_string(),
            cwd: args.get("cwd").and_then(Value::as_str).map(str::to_string),
            n_results: n,
            recent_first: args
                .get("recent_first")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            category: args
                .get("category")
                .and_then(Value::as_str)
                .map(str::to_string),
            exclude_reserved: false,
            adapter: "mcp".to_string(),
        },
    )
    .await?;
    let mut lines = vec![
        format!("=== memory search results ({query}) ==="),
        format!("recall_id={}", out.recall_id),
    ];
    if out.results.is_empty() {
        lines.push("no results".to_string());
    }
    for (i, item) in out.results.iter().take(n).enumerate() {
        lines.push(format!(
            "[{}] project={} score={:.4} id={}",
            i + 1,
            item.project,
            item.score,
            item.id
        ));
        lines.push(format!("    {}", item.document));
    }
    Ok(lines.join("\n"))
}


#[cfg(test)]
mod tests {
    //! Transport-independent checks on `dispatch`, the shared entry point behind
    //! both stdio and `POST /mcp`.
    use super::*;
    use crate::server::api::test_support::build_system;

    #[test]
    fn tool_list_is_deterministic_for_vault_capability() {
        let memory_names: Vec<_> = tools(false)
            .into_iter()
            .map(|tool| tool["name"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(memory_names.len(), 5);
        assert!(memory_names.iter().all(|name| name.starts_with("memory_")));
        let enabled = tools(true);
        assert_eq!(enabled.len(), 9);
        let search = enabled
            .iter()
            .find(|tool| tool["name"] == "memory_search")
            .unwrap();
        let required = search["inputSchema"]["required"].as_array().unwrap();
        assert!(required.contains(&json!("query")));
        assert!(!required.contains(&json!("project")));
        assert!(search["inputSchema"]["properties"]["cwd"].is_object());
        assert!(
            search["inputSchema"]["properties"]["project"]
                .get("default")
                .is_none()
        );
    }

    #[tokio::test]
    async fn dispatch_handles_protocol_methods() {
        let (_tmp, system) = build_system().await;

        let init = dispatch(
            system.clone(),
            &json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        )
        .await
        .expect("initialize is a request, not a notification");
        assert_eq!(init["jsonrpc"], "2.0");
        assert_eq!(init["id"], 1);
        assert_eq!(init["result"]["protocolVersion"], DEFAULT_PROTOCOL_VERSION);
        assert_eq!(init["result"]["serverInfo"]["name"], "memnest");
        assert!(init["result"]["capabilities"]["tools"].is_object());

        // A revision we know is echoed back so Streamable HTTP clients negotiate.
        let negotiated = dispatch(
            system.clone(),
            &json!({"jsonrpc":"2.0","id":2,"method":"initialize",
                    "params":{"protocolVersion":"2025-06-18"}}),
        )
        .await
        .expect("initialize returns a response");
        assert_eq!(negotiated["result"]["protocolVersion"], "2025-06-18");

        // An unknown revision falls back instead of parroting it.
        let unknown_version = dispatch(
            system.clone(),
            &json!({"jsonrpc":"2.0","id":3,"method":"initialize",
                    "params":{"protocolVersion":"1999-01-01"}}),
        )
        .await
        .expect("initialize returns a response");
        assert_eq!(
            unknown_version["result"]["protocolVersion"],
            DEFAULT_PROTOCOL_VERSION
        );

        let listed = dispatch(
            system.clone(),
            &json!({"jsonrpc":"2.0","id":4,"method":"tools/list"}),
        )
        .await
        .expect("tools/list returns a response");
        let listed_tools = listed["result"]["tools"]
            .as_array()
            .expect("tools is an array");
        assert_eq!(listed_tools.len(), tools(true).len());
        assert!(
            listed_tools
                .iter()
                .any(|t| t["name"] == "memory_search" && t["inputSchema"].is_object())
        );

        let unknown = dispatch(
            system.clone(),
            &json!({"jsonrpc":"2.0","id":5,"method":"does/not/exist"}),
        )
        .await
        .expect("an unknown method still owes the caller a reply");
        assert_eq!(unknown["id"], 5);
        assert_eq!(unknown["error"]["code"], -32601);
        assert!(unknown.get("result").is_none());

        // Notifications take no reply, which is what lets POST /mcp answer 202.
        assert!(
            dispatch(
                system.clone(),
                &json!({"jsonrpc":"2.0","method":"notifications/initialized"})
            )
            .await
            .is_none()
        );
        assert!(
            dispatch(system, &json!({"jsonrpc":"2.0","method":"tools/list"}))
                .await
                .is_none()
        );
    }

    #[tokio::test]
    async fn secret_capability_and_not_found_match_across_transports() {
        let (_tmp, system) = build_system().await;
        system.write().await.vault_enabled = false;
        let unavailable = crate::server::api::get_secret(
            State(system.clone()),
            axum::extract::Path("missing".to_string()),
        )
        .await;
        assert_eq!(
            unavailable.status(),
            axum::http::StatusCode::SERVICE_UNAVAILABLE
        );
        let unavailable_mcp = dispatch(system.clone(), &json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"secret_get","arguments":{"key":"missing"}}})).await.unwrap();
        assert_eq!(unavailable_mcp["result"]["isError"], true);

        system.write().await.vault_enabled = true;
        let missing = crate::server::api::get_secret(
            State(system.clone()),
            axum::extract::Path("missing".to_string()),
        )
        .await;
        assert_eq!(missing.status(), axum::http::StatusCode::NOT_FOUND);
        let missing_mcp = dispatch(system, &json!({"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"secret_get","arguments":{"key":"missing"}}})).await.unwrap();
        assert_eq!(missing_mcp["result"]["isError"], true);
    }

    #[tokio::test]
    async fn dispatch_executes_tool_calls() {
        let (_tmp, system) = build_system().await;
        assert_eq!(tools(true).len(), 9);
        let names: Vec<String> = tools(true)
            .iter()
            .filter_map(|tool| tool["name"].as_str().map(str::to_string))
            .collect();
        assert_eq!(
            names,
            vec![
                "memory_remember",
                "memory_search",
                "memory_get",
                "memory_update",
                "memory_delete",
                "secret_set",
                "secret_get",
                "secret_list",
                "secret_delete"
            ]
        );

        let failed = dispatch(
            system,
            &json!({"jsonrpc":"2.0","id":8,"method":"tools/call",
                    "params":{"name":"memory_get","arguments":{"id":"nope_missing"}}}),
        )
        .await
        .expect("tools/call returns a response");
        assert_eq!(failed["id"], 8);
        assert_eq!(failed["result"]["isError"], true);
        assert!(failed.get("error").is_none());
    }

    async fn body_json(response: axum::response::Response) -> Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn http_and_mcp_share_the_canonical_memory_contract() {
        use axum::Json;
        use axum::extract::State;

        let (_tmp, system) = build_system().await;
        let http_add = crate::server::api::add(
            State(system.clone()),
            Json(crate::server::api::AddRequest {
                text: "canonical parity probe".into(),
                project: "contract".into(),
                cwd: None,
                metadata: None,
                sensitive: None,
            }),
        )
        .await;
        assert_eq!(http_add.status(), axum::http::StatusCode::CREATED);
        let added = body_json(http_add).await;
        let id = added["id"].as_str().unwrap();

        let mcp_add = dispatch(system.clone(), &json!({"jsonrpc":"2.0","id":20,"method":"tools/call","params":{"name":"memory_remember","arguments":{"text":"canonical parity probe","project":"contract"}}})).await.unwrap();
        assert!(mcp_add["result"]["isError"].is_null());
        let mcp_added: Value =
            serde_json::from_str(mcp_add["result"]["content"][0]["text"].as_str().unwrap())
                .unwrap();
        assert_eq!(mcp_added["id"], id);
        assert_eq!(mcp_added["project"], added["project"]);

        let unscoped_http = crate::server::api::search(
            State(system.clone()),
            Json(crate::server::api::SearchRequest {
                query: "must fail closed".into(),
                project: String::new(),
                cwd: None,
                n_results: 3,
                recent_first: false,
                category: String::new(),
                exclude_reserved: false,
                adapter: "test-http".into(),
            }),
        )
        .await;
        assert_eq!(unscoped_http.status(), axum::http::StatusCode::BAD_REQUEST);

        let http_search = crate::server::api::search(
            State(system.clone()),
            Json(crate::server::api::SearchRequest {
                query: "canonical parity probe".into(),
                project: "contract".into(),
                cwd: None,
                n_results: 3,
                recent_first: false,
                category: String::new(),
                exclude_reserved: false,
                adapter: "test-http".into(),
            }),
        )
        .await;
        let searched = body_json(http_search).await;
        assert_eq!(searched["results"][0]["id"], id);
        let mcp_search = dispatch(system.clone(), &json!({"jsonrpc":"2.0","id":21,"method":"tools/call","params":{"name":"memory_search","arguments":{"query":"canonical parity probe","project":"contract","n_results":3}}})).await.unwrap();
        let search_text = mcp_search["result"]["content"][0]["text"].as_str().unwrap();
        assert!(search_text.contains(&format!("id={id}")));

        let invalid_http = crate::server::api::add(
            State(system.clone()),
            Json(crate::server::api::AddRequest {
                text: "rejected".into(),
                project: "_trash".into(),
                cwd: None,
                metadata: None,
                sensitive: None,
            }),
        )
        .await;
        assert_eq!(invalid_http.status(), axum::http::StatusCode::BAD_REQUEST);
        let invalid_mcp = dispatch(system.clone(), &json!({"jsonrpc":"2.0","id":22,"method":"tools/call","params":{"name":"memory_remember","arguments":{"text":"rejected","project":"_trash"}}})).await.unwrap();
        assert_eq!(invalid_mcp["result"]["isError"], true);

        let http_get = body_json(
            crate::server::api::get_chunk_full(
                State(system.clone()),
                axum::extract::Path(id.to_string()),
            )
            .await,
        )
        .await;
        let mcp_get = dispatch(system.clone(), &json!({"jsonrpc":"2.0","id":23,"method":"tools/call","params":{"name":"memory_get","arguments":{"id":id}}})).await.unwrap();
        assert_eq!(http_get["document"], "canonical parity probe");
        assert!(
            mcp_get["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("canonical parity probe")
        );

        let update: crate::server::api::UpdateRequest = serde_json::from_value(
            json!({"id":id,"project":"contract-updated","text":"canonical parity updated"}),
        )
        .unwrap();
        let http_update =
            body_json(crate::server::api::update(State(system.clone()), Json(update)).await).await;
        let mcp_update = dispatch(system.clone(), &json!({"jsonrpc":"2.0","id":24,"method":"tools/call","params":{"name":"memory_update","arguments":{"id":id,"project":"contract-updated","text":"canonical parity updated"}}})).await.unwrap();
        let mcp_updated: Value =
            serde_json::from_str(mcp_update["result"]["content"][0]["text"].as_str().unwrap())
                .unwrap();
        assert_eq!(http_update["project"], mcp_updated["project"]);

        let empty_update: crate::server::api::UpdateRequest =
            serde_json::from_value(json!({"id":id,"text":"   "})).unwrap();
        let empty_update_response =
            crate::server::api::update(State(system.clone()), Json(empty_update)).await;
        assert_eq!(
            empty_update_response.status(),
            axum::http::StatusCode::BAD_REQUEST
        );

        let delete_request: crate::server::api::DeleteRequest =
            serde_json::from_value(json!({"ids":[id]})).unwrap();
        let deleted = body_json(
            crate::server::api::delete(State(system.clone()), Json(delete_request)).await,
        )
        .await;
        assert_eq!(deleted["deleted"][0], id);
        let stored = system
            .read()
            .await
            .db
            .read()
            .await
            .get_chunk(id)
            .unwrap()
            .unwrap();
        assert_eq!(stored.project, "_trash");
        assert!(
            crate::server::operations::get(system.clone(), id)
                .await
                .is_ok(),
            "soft delete keeps the record addressable by id"
        );

        let second = crate::server::operations::remember(
            system.clone(),
            crate::server::operations::RememberInput {
                text: "mcp delete parity probe".into(),
                project: "contract".into(),
                cwd: None,
                metadata: None,
                sensitive: false,
            },
        )
        .await
        .unwrap();
        let mcp_delete = dispatch(system.clone(), &json!({"jsonrpc":"2.0","id":26,"method":"tools/call","params":{"name":"memory_delete","arguments":{"id":second.id}}})).await.unwrap();
        assert!(mcp_delete["result"]["isError"].is_null());
        assert_eq!(
            system
                .read()
                .await
                .db
                .read()
                .await
                .get_chunk(&second.id)
                .unwrap()
                .unwrap()
                .project,
            "_trash"
        );
    }
}
