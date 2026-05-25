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
    checks.push(check_graph(&config.data_dir).await);
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

    let chunk_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM chunks", [], |row| row.get(0))
        .unwrap_or(-1);
    let fact_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM facts", [], |row| row.get(0))
        .unwrap_or(-1);
    let server_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM servers", [], |row| row.get(0))
        .unwrap_or(-1);
    let note_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM notes", [], |row| row.get(0))
        .unwrap_or(-1);

    Check {
        name: "database",
        status: Status::Ok,
        message: format!(
            "WAL={}, chunks={}, facts={}, servers={}, notes={}",
            journal, chunk_count, fact_count, server_count, note_count
        ),
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

async fn check_graph(data_dir: &Path) -> Check {
    match crate::graph::KnowledgeGraph::new(data_dir) {
        Ok(g) => Check {
            name: "knowledge graph",
            status: Status::Ok,
            message: format!(
                "graph loaded, nodes={}, edges={}",
                g.node_count(),
                g.edge_count()
            ),
        },
        Err(e) => Check {
            name: "knowledge graph",
            status: Status::Error,
            message: format!("cannot initialize: {}", e),
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
            "model cache is not warmed at {}; run `palimpsest --data-dir {} --warmup-embedding` on an online machine before offline use",
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
