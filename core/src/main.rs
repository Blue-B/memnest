use anyhow::{Context, bail};
use clap::Parser;
use palimpsest::models::{ChunkType, Fact, FactHistory, Importance, MemoryChunk, Metadata};
use palimpsest::redaction::redact_text;
use palimpsest::{MemorySystem, config::Config};
use serde::Deserialize;
use serde_json::Value;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::info;

#[derive(Parser, Debug)]
#[command(name = "palimpsest")]
#[command(version)]
#[command(about = "Persistent memory for AI coding agents")]
struct Cli {
    #[arg(long, default_value = "3111")]
    port: u16,

    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    #[arg(long, default_value = "3113")]
    viewer_port: u16,

    #[arg(long)]
    mcp: bool,

    #[arg(long)]
    data_dir: Option<String>,

    #[arg(long)]
    import_jsonl: Option<String>,

    #[arg(long)]
    import_facts_json: Option<String>,

    #[arg(long)]
    doctor: bool,

    #[arg(long)]
    warmup_embedding: bool,

    #[arg(long)]
    backup_dir: Option<String>,

    #[arg(long)]
    restore_dir: Option<String>,

    #[arg(long)]
    force: bool,
}

#[derive(Debug, Deserialize)]
struct ImportRecord {
    id: Option<String>,
    project: Option<String>,
    document: Option<String>,
    text: Option<String>,
    embedding: Option<Vec<f32>>,
    metadata: Option<Metadata>,
    created_at: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ImportFactRecord {
    id: Option<String>,
    subject: Option<String>,
    predicate: Option<String>,
    object: Option<Value>,
    timestamp: Option<String>,
    source_session: Option<String>,
    #[serde(default)]
    history: Vec<ImportFactHistory>,
}

#[derive(Debug, Default, Deserialize)]
struct ImportFactHistory {
    object: Option<Value>,
    timestamp: Option<String>,
    source_session: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    let mut config = Config::default();
    config.api_port = cli.port;
    config.api_host = cli.host.clone();
    config.viewer_port = cli.viewer_port;
    if let Some(dir) = cli.data_dir {
        config.data_dir = PathBuf::from(dir);
    }

    if let Some(path) = cli.backup_dir.as_deref() {
        backup_data_dir(&config.data_dir, Path::new(path))?;
        println!(
            "backup complete: {} -> {}",
            config.data_dir.display(),
            Path::new(path).display()
        );
        return Ok(());
    }

    if let Some(path) = cli.restore_dir.as_deref() {
        restore_data_dir(Path::new(path), &config.data_dir, cli.force)?;
        println!(
            "restore complete: {} -> {}",
            Path::new(path).display(),
            config.data_dir.display()
        );
        return Ok(());
    }

    if cli.warmup_embedding {
        let system = MemorySystem::new(config.clone()).await?;
        let _ = system
            .embedder
            .encode_document("palimpsest embedding warmup")?;
        println!(
            "embedding warmup complete: model={}, data_dir={}",
            config.embed_model,
            config.data_dir.display()
        );
        return Ok(());
    }

    if cli.doctor {
        println!("palimpsest doctor v{}", env!("CARGO_PKG_VERSION"));
        println!("  data dir: {:?}", config.data_dir);
        println!();
        let checks = palimpsest::doctor::run(&config).await?;
        let exit_code = palimpsest::doctor::print_report(&checks);
        std::process::exit(exit_code);
    }

    if !cli.mcp && cli.import_jsonl.is_none() && cli.import_facts_json.is_none() {
        enforce_bind_safety(&config.api_host)?;
    }

    info!("Starting palimpsest v{}", env!("CARGO_PKG_VERSION"));
    info!("Data dir: {:?}", config.data_dir);

    let system = Arc::new(tokio::sync::RwLock::new(
        MemorySystem::new(config.clone()).await?,
    ));

    let mut imported = false;
    if let Some(path) = cli.import_jsonl.as_deref() {
        let imported_memories = import_jsonl(system.clone(), path).await?;
        println!("imported {} memories", imported_memories);
        imported = true;
    }
    if let Some(path) = cli.import_facts_json.as_deref() {
        let imported_facts = import_facts_json(system.clone(), path).await?;
        println!("imported {} facts", imported_facts);
        imported = true;
    }
    if imported {
        return Ok(());
    }

    if cli.mcp {
        info!("MCP stdio server enabled");
        palimpsest::server::mcp::run_stdio(system.clone()).await?;
        return Ok(());
    }

    // Kick off the daily TTL prune loop. Without this, AutoLog chunks
    // accumulate indefinitely (~537 in production at time of writing).
    palimpsest::lifecycle::spawn_periodic_lifecycle(system.clone());

    // Start API server
    let app = palimpsest::server::create_router(system.clone());
    let addr = format!("{}:{}", config.api_host, config.api_port);
    info!("API server starting on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    let server = axum::serve(listener, app);

    info!("palimpsest ready");

    tokio::select! {
        result = server => {
            result?;
        }
        _ = shutdown_signal() => {
            info!("received shutdown signal, saving indexes...");
            let sys = system.read().await;
            if let Err(e) = sys.save().await {
                tracing::error!("failed to save indexes: {}", e);
            } else {
                info!("indexes saved successfully");
            }
        }
    }

    Ok(())
}

#[cfg(unix)]
async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    let terminate = async {
        let mut signal = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler");
        signal.recv().await;
    };

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}

#[cfg(not(unix))]
async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

fn enforce_bind_safety(host: &str) -> anyhow::Result<()> {
    let local = matches!(host, "127.0.0.1" | "localhost" | "::1");
    if local || std::env::var("PALIMPSEST_TOKEN").is_ok() {
        return Ok(());
    }

    bail!(
        "refusing to bind to {host} without authentication; set PALIMPSEST_TOKEN or bind to 127.0.0.1"
    )
}

fn backup_data_dir(source: &Path, target: &Path) -> anyhow::Result<()> {
    if !source.exists() {
        bail!("data directory does not exist: {}", source.display());
    }
    if target.exists() {
        bail!("backup target already exists: {}", target.display());
    }
    copy_dir_recursive(source, target)
        .with_context(|| format!("failed to backup {}", source.display()))
}

fn restore_data_dir(source: &Path, target: &Path, force: bool) -> anyhow::Result<()> {
    if !source.exists() {
        bail!("restore source does not exist: {}", source.display());
    }
    if target.exists() {
        let mut entries = std::fs::read_dir(target)
            .with_context(|| format!("cannot read data directory {}", target.display()))?;
        if entries.next().is_some() {
            if !force {
                bail!(
                    "data directory is not empty: {}; pass --force to replace it",
                    target.display()
                );
            }
            std::fs::remove_dir_all(target)
                .with_context(|| format!("failed to remove {}", target.display()))?;
        }
    }
    copy_dir_recursive(source, target)
        .with_context(|| format!("failed to restore {}", source.display()))
}

fn copy_dir_recursive(source: &Path, target: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(target)
        .with_context(|| format!("failed to create {}", target.display()))?;
    for entry in
        std::fs::read_dir(source).with_context(|| format!("failed to read {}", source.display()))?
    {
        let entry = entry?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            copy_dir_recursive(&source_path, &target_path)?;
        } else if metadata.is_file() {
            std::fs::copy(&source_path, &target_path).with_context(|| {
                format!(
                    "failed to copy {} to {}",
                    source_path.display(),
                    target_path.display()
                )
            })?;
        }
    }
    Ok(())
}

async fn import_jsonl(
    system: Arc<tokio::sync::RwLock<MemorySystem>>,
    path: &str,
) -> anyhow::Result<usize> {
    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    let sys = system.read().await;
    let db = sys.db.write().await;
    let mut vector_index = sys.vector_index.write().await;
    let mut text_docs = Vec::new();
    let mut count = 0usize;

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let rec: ImportRecord = serde_json::from_str(&line)?;
        let document = rec.document.or(rec.text).unwrap_or_default();
        if document.trim().is_empty() {
            continue;
        }
        let project = rec.project.unwrap_or_else(|| "default".to_string());
        let id = rec.id.unwrap_or_else(|| {
            format!(
                "import_{}",
                uuid::Uuid::new_v4().to_string().replace('-', "")[..16].to_string()
            )
        });
        let created_at = parse_import_timestamp(rec.created_at.as_deref());
        let embedding = match rec.embedding {
            Some(v) if !v.is_empty() => Some(v),
            _ => Some(sys.embedder.encode_document(&document)?),
        };
        let chunk = MemoryChunk {
            id: id.clone(),
            project: project.clone(),
            document,
            embedding,
            metadata: rec.metadata.unwrap_or(Metadata {
                chunk_type: ChunkType::AutoLog,
                importance: Importance::Log,
                ..Default::default()
            }),
            created_at,
            updated_at: created_at,
        };
        db.insert_chunk(&chunk)?;
        if let Some(embedding) = &chunk.embedding {
            vector_index.add(&chunk.id, embedding)?;
        }
        text_docs.push((
            chunk.id.clone(),
            chunk.project.clone(),
            chunk.document.clone(),
        ));
        count += 1;
    }
    sys.add_text_docs(&text_docs).await?;
    Ok(count)
}

async fn import_facts_json(
    system: Arc<tokio::sync::RwLock<MemorySystem>>,
    path: &str,
) -> anyhow::Result<usize> {
    let file = std::fs::File::open(path)?;
    let value: Value = serde_json::from_reader(file)?;
    let records = import_fact_records(value)?;
    let sys = system.read().await;
    let db = sys.db.write().await;
    let mut count = 0usize;

    for (key, record) in records {
        let subject = record.subject.or_else(|| {
            key.as_deref()
                .and_then(|key| key.split_once("::").map(|(subject, _)| subject.to_string()))
        });
        let predicate = record.predicate.or_else(|| {
            key.as_deref().and_then(|key| {
                key.split_once("::")
                    .map(|(_, predicate)| predicate.to_string())
            })
        });
        let object = record.object.and_then(json_scalar_to_string);
        let (Some(subject), Some(predicate), Some(object)) = (subject, predicate, object) else {
            continue;
        };

        let id = record
            .id
            .or(key)
            .unwrap_or_else(|| palimpsest::facts::fact_id(&subject, &predicate));
        let history = record
            .history
            .into_iter()
            .filter_map(|item| {
                Some(FactHistory {
                    object: redact_text(&json_scalar_to_string(item.object?)?),
                    timestamp: parse_import_timestamp(item.timestamp.as_deref()),
                    source_session: item.source_session,
                })
            })
            .collect::<Vec<_>>();
        let fact = Fact {
            id: redact_text(&id),
            subject: redact_text(&subject),
            predicate: redact_text(&predicate),
            object: redact_text(&object),
            timestamp: parse_import_timestamp(record.timestamp.as_deref()),
            source_session: record.source_session,
            history,
        };
        db.insert_fact(&fact)?;
        count += 1;
    }
    Ok(count)
}

fn import_fact_records(value: Value) -> anyhow::Result<Vec<(Option<String>, ImportFactRecord)>> {
    match value {
        Value::Object(map) => map
            .into_iter()
            .map(|(key, value)| Ok((Some(key), serde_json::from_value(value)?)))
            .collect(),
        Value::Array(items) => items
            .into_iter()
            .map(|value| Ok((None, serde_json::from_value(value)?)))
            .collect(),
        _ => anyhow::bail!("facts JSON must be an object or array"),
    }
}

fn json_scalar_to_string(value: Value) -> Option<String> {
    match value {
        Value::Null => None,
        Value::String(value) => Some(value),
        Value::Bool(value) => Some(value.to_string()),
        Value::Number(value) => Some(value.to_string()),
        other => Some(other.to_string()),
    }
}

fn parse_import_timestamp(value: Option<&str>) -> chrono::DateTime<chrono::Utc> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return chrono::Utc::now();
    };

    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(value) {
        return dt.with_timezone(&chrono::Utc);
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f") {
        return dt.and_utc();
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S") {
        return dt.and_utc();
    }
    if let Ok(dt) = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M") {
        return dt.and_utc();
    }
    chrono::Utc::now()
}
