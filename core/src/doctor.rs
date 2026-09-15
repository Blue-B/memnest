use crate::config::Config;
use anyhow::Result;
use std::path::Path;

pub enum Status {
    Ok,
    Warn,
    Error,
}

pub struct Check {
    pub name: &'static str,
    pub status: Status,
    pub message: String,
}

pub async fn run(config: &Config) -> Result<Vec<Check>> {
    let mut checks = Vec::new();
    checks.push(check_data_dir(&config.data_dir).await);
    checks.push(check_database(&config.data_dir).await);
    checks.push(check_vector_index(&config.data_dir).await);
    checks.push(check_text_index(&config.data_dir).await);
    checks.push(check_embedding(config).await);
    checks.push(check_config(config));
    Ok(checks)
}

async fn check_data_dir(data_dir: &Path) -> Check {
    if !data_dir.exists() {
        return Check {
            name: "data directory",
            status: Status::Error,
            message: format!("does not exist: {}", data_dir.display()),
        };
    }
    if !data_dir.is_dir() {
        return Check {
            name: "data directory",
            status: Status::Error,
            message: format!("not a directory: {}", data_dir.display()),
        };
    }

    let test_file = data_dir.join(".doctor_write_test");
    match tokio::fs::write(&test_file, b"test").await {
        Ok(_) => {
            let _ = tokio::fs::remove_file(&test_file).await;
            Check {
                name: "data directory",
                status: Status::Ok,
                message: format!("{} is readable and writable", data_dir.display()),
            }
        }
        Err(e) => Check {
            name: "data directory",
            status: Status::Error,
            message: format!("not writable: {}", e),
        },
    }
}

async fn check_database(data_dir: &Path) -> Check {
    let db_path = data_dir.join("memory.db");
    if !db_path.exists() {
        return Check {
            name: "database",
            status: Status::Warn,
            message: format!(
                "database does not exist yet (will be created on first run): {}",
                db_path.display()
            ),
        };
    }

    let manager = r2d2_sqlite::SqliteConnectionManager::file(&db_path);
    let pool = match r2d2::Pool::new(manager) {
        Ok(p) => p,
        Err(e) => {
            return Check {
                name: "database",
                status: Status::Error,
                message: format!("cannot open pool: {}", e),
            };
        }
    };

    let conn = match pool.get() {
        Ok(c) => c,
        Err(e) => {
            return Check {
                name: "database",
                status: Status::Error,
                message: format!("cannot get connection: {}", e),
            };
        }
    };

    let journal: String = match conn.query_row("PRAGMA journal_mode", [], |row| row.get(0)) {
        Ok(j) => j,
        Err(e) => {
            return Check {
                name: "database",
                status: Status::Error,
                message: format!("cannot query journal mode: {}", e),
            };
        }
    };

    // Only the memory store is reported: facts/servers/notes are legacy tables
    // kept for data preservation and no longer part of any product surface.
    let chunk_count: i64 = conn
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM chunks WHERE {}",
                crate::models::VISIBLE_CHUNKS_SQL
            ),
            [],
            |row| row.get(0),
        )
        .unwrap_or(-1);
    let trashed_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM chunks WHERE project = '_trash'",
            [],
            |row| row.get(0),
        )
        .unwrap_or(-1);

    Check {
        name: "database",
        status: Status::Ok,
        message: format!("WAL={journal}, chunks={chunk_count}, trashed={trashed_count}"),
    }
}

async fn check_vector_index(data_dir: &Path) -> Check {
    let vector_dir = data_dir.join("vectors");
    if !vector_dir.exists() {
        return Check {
            name: "vector index",
            status: Status::Warn,
            message: "vector index directory does not exist yet (will be created on first run)"
                .to_string(),
        };
    }
    match crate::index::VectorIndex::new(data_dir) {
        Ok(idx) => Check {
            name: "vector index",
            status: Status::Ok,
            message: format!(
                "HNSW initialized, dim={}, active_entries={}",
                idx.dim(),
                idx.len()
            ),
        },
        Err(e) => Check {
            name: "vector index",
            status: Status::Error,
            message: format!("cannot initialize: {}", e),
        },
    }
}

async fn check_text_index(data_dir: &Path) -> Check {
    let text_dir = data_dir.join("text_index");
    if !text_dir.exists() {
        return Check {
            name: "text index",
            status: Status::Warn,
            message: "text index directory does not exist yet (will be created on first run)"
                .to_string(),
        };
    }
    let meta_exists = text_dir.join("meta.json").exists();
    if !meta_exists {
        return Check {
            name: "text index",
            status: Status::Warn,
            message: format!(
                "directory exists but meta.json missing: {}",
                text_dir.display()
            ),
        };
    }
    match crate::index::TextIndex::new(data_dir) {
        Ok(_) => Check {
            name: "text index",
            status: Status::Ok,
            message: format!("Tantivy index ready at {}", text_dir.display()),
        },
        Err(e) => Check {
            name: "text index",
            status: Status::Error,
            message: format!("cannot open: {}", e),
        },
    }
}

async fn check_embedding(config: &Config) -> Check {
    let model_dir = config.data_dir.join("models");
    let cache_present = dir_has_files(&model_dir);
    let cache_state = if cache_present {
        format!("model cache present at {}", model_dir.display())
    } else {
        format!(
            "model cache is not warmed at {}; run `memnest --data-dir {} --warmup-embedding` on an online machine before offline use",
            model_dir.display(),
            config.data_dir.display()
        )
    };
    Check {
        name: "embedding",
        status: if cache_present {
            Status::Ok
        } else {
            Status::Warn
        },
        message: format!(
            "native neural embeddings (model={}, dim={}); {}",
            config.embed_model, config.embed_dim, cache_state
        ),
    }
}

fn dir_has_files(path: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(path) else {
        return false;
    };
    for entry in entries.flatten() {
        let entry_path = entry.path();
        if entry_path.is_file() || dir_has_files(&entry_path) {
            return true;
        }
    }
    false
}

fn check_config(config: &Config) -> Check {
    let mut issues = Vec::new();
    if config.embed_dim == 0 {
        issues.push("embed_dim must be > 0".to_string());
    }
    if !(0.0..=1.0).contains(&config.distance_cutoff) {
        issues.push("distance_cutoff should be in [0,1]".to_string());
    }
    if config.recency_penalty_rate < 0.0 {
        issues.push("recency_penalty_rate must be >= 0".to_string());
    }

    if issues.is_empty() {
        Check {
            name: "configuration",
            status: Status::Ok,
            message: "all parameters within valid ranges".to_string(),
        }
    } else {
        Check {
            name: "configuration",
            status: Status::Warn,
            message: issues.join("; "),
        }
    }
}

pub fn print_report(checks: &[Check]) -> i32 {
    let mut errors = 0usize;
    let mut warns = 0usize;

    for check in checks {
        let (icon, color) = match check.status {
            Status::Ok => ("✓", "\x1b[32m"),    // green
            Status::Warn => ("⚠", "\x1b[33m"),  // yellow
            Status::Error => ("✗", "\x1b[31m"), // red
        };
        let reset = "\x1b[0m";
        println!(
            "  {} {}{:<20}{} {}",
            color, icon, check.name, reset, check.message
        );
        match check.status {
            Status::Error => errors += 1,
            Status::Warn => warns += 1,
            _ => {}
        }
    }

    println!();
    if errors > 0 {
        println!("  {} error(s), {} warning(s)", errors, warns);
        1
    } else if warns > 0 {
        println!("  0 errors, {} warning(s)", warns);
        0
    } else {
        println!("  All checks passed");
        0
    }
}

/// Probe only public diagnostic contracts; never open a DB, initialize an index,
/// or embed text. Bound both elapsed time and untrusted response size.
pub async fn diagnose_service(base: &str) -> i32 {
    println!("endpoint: {base}");
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), async {
        let client = reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let token = crate::server::auth_token();
        let health = diagnostic_json(&client, base, "/health", token.as_deref(), None).await?;
        if health.get("status").and_then(|v| v.as_str()) != Some("ok") {
            anyhow::bail!("health contract mismatch (expected status=ok)");
        }
        // Do not echo response strings: even version/error fields may contain
        // credentials, server-local paths, or terminal control sequences.
        println!("health: ok (response body omitted)");
        match health
            .pointer("/embedding/loaded")
            .and_then(|v| v.as_bool())
        {
            Some(true) => println!("embedding: loaded"),
            Some(false) => println!("embedding: lazy (loads on first search or write)"),
            None => println!("embedding: unknown (older health contract)"),
        }
        let pending = health
            .pointer("/index/pending_operations")
            .and_then(|v| v.as_u64());
        let rebuild = health
            .pointer("/index/rebuild_required")
            .and_then(|v| v.as_bool());
        match (pending, rebuild) {
            (Some(pending), Some(rebuild)) => {
                println!("index: {pending} pending operation(s), rebuild_required={rebuild}")
            }
            _ => println!("index: unknown (older health contract)"),
        }
        let tools = diagnostic_json(
            &client,
            base,
            "/mcp",
            token.as_deref(),
            Some(serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}})),
        )
        .await?;
        let list = tools
            .pointer("/result/tools")
            .and_then(|v| v.as_array())
            .ok_or_else(|| anyhow::anyhow!("MCP capability mismatch: tools/list unavailable"))?;
        for (name, fields) in [
            ("memory_search", &["query", "project"][..]),
            ("memory_get", &["id", "offset", "max_chars"][..]),
        ] {
            let properties = list
                .iter()
                .find(|v| v["name"] == name)
                .and_then(|v| v.pointer("/inputSchema/properties"));
            if !fields
                .iter()
                .all(|field| properties.and_then(|p| p.get(field)).is_some())
            {
                anyhow::bail!(
                    "MCP capability mismatch: {name} missing required source-client fields"
                );
            }
        }
        println!("capabilities: scoped search and paged get advertised (not an execution test)");
        Ok::<(), anyhow::Error>(())
    })
    .await;
    println!("remote capture: unknown (not exposed by the service health contract)");
    match result {
        Ok(Ok(())) => 0,
        Ok(Err(error)) => {
            println!("diagnostic: {error}");
            1
        }
        Err(_) => {
            println!("diagnostic: timed out (5 second total budget)");
            1
        }
    }
}

async fn diagnostic_json(
    client: &reqwest::Client,
    base: &str,
    path: &str,
    token: Option<&str>,
    body: Option<serde_json::Value>,
) -> Result<serde_json::Value> {
    let mut request = if let Some(body) = body {
        client.post(format!("{base}{path}")).json(&body)
    } else {
        client.get(format!("{base}{path}"))
    };
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    let mut response = request
        .send()
        .await
        .map_err(|_| anyhow::anyhow!("service unreachable or transport failure at {path}"))?;
    match response.status().as_u16() {
        401 | 403 => {
            anyhow::bail!("authentication failure at {path}; check MEMNEST_TOKEN (not printed)")
        }
        200 => (),
        code => anyhow::bail!("HTTP {code} at {path}; service/capability mismatch"),
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| anyhow::anyhow!("response transport failure at {path}"))?
    {
        if bytes.len() + chunk.len() > 256 * 1024 {
            anyhow::bail!("diagnostic response exceeds 256 KiB");
        }
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| anyhow::anyhow!("invalid JSON contract at {path}"))
}
