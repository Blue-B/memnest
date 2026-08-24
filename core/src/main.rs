use anyhow::{Context, bail};
use clap::{Parser, Subcommand};
use fs2::FileExt;
use memnest::hook::HookFormat;
use memnest::models::{ChunkType, Importance, MemoryChunk, Metadata};
use memnest::{MemorySystem, config::Config};
use serde::Deserialize;
use std::io::{BufRead, BufReader};
use std::net::ToSocketAddrs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::info;

#[derive(Parser, Debug)]
#[command(name = "memnest")]
#[command(version)]
#[command(about = "Persistent memory for AI coding agents")]
struct Cli {
    #[arg(long, default_value = "3111")]
    port: u16,

    #[arg(long, default_value = "127.0.0.1")]
    host: String,

    /// Deprecated no-op, accepted so existing service files keep starting.
    #[arg(long, hide = true)]
    viewer_port: Option<u16>,

    #[command(subcommand)]
    command: Option<CliCommand>,

    #[arg(long)]
    mcp: bool,

    #[arg(long)]
    data_dir: Option<String>,

    #[arg(long)]
    import_jsonl: Option<String>,

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

#[derive(Subcommand, Debug)]
enum CliCommand {
    /// Show the canonical dashboard URL and whether the local service is reachable.
    Status,
    /// Print the canonical dashboard URL. Terminals usually render it as a clickable link.
    Dashboard,
    /// Answer a host's prompt hook with a context pack, for automatic injection
    /// without a per-host extension. Reads the hook payload on stdin and writes
    /// the reply on stdout; never fails, so it cannot block a prompt.
    Hook {
        /// Address of the running service. Defaults to MEMNEST_URL, then http://127.0.0.1:3111.
        #[arg(long)]
        url: Option<String>,
        /// Reply shape. `auto` reads it from the payload.
        #[arg(long, value_enum, default_value_t = HookFormat::Auto)]
        format: HookFormat,
        /// Give up on the service after this long and inject nothing.
        #[arg(long, default_value_t = 2000)]
        timeout_ms: u64,
    },
    /// Follow local session transcripts and store new turns automatically, so
    /// any host that writes one gets AutoLog without an extension.
    Watch {
        /// Address of the running service. Defaults to MEMNEST_URL, then http://127.0.0.1:3111.
        #[arg(long)]
        url: Option<String>,
        /// Transcript directory to follow. Repeatable; defaults to the known
        /// Claude Code, pi, and Codex locations.
        #[arg(long = "path")]
        paths: Vec<String>,
        /// Make one pass and exit instead of following.
        #[arg(long)]
        once: bool,
        /// Seconds between passes.
        #[arg(long, default_value_t = 5)]
        interval: u64,
        /// Import existing history instead of following from the end.
        #[arg(long)]
        backfill: bool,
    },
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

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let cli = Cli::parse();

    let mut config = Config::default();
    config.api_port = cli.port;
    config.api_host = cli.host.clone();
    if cli.viewer_port.is_some() {
        eprintln!(
            "warning: --viewer-port is a deprecated no-op; every endpoint is served on --port ({})",
            cli.port
        );
    }
    if let Some(dir) = cli.data_dir {
        config.data_dir = PathBuf::from(dir);
    }

    if let Some(command) = &cli.command {
        match command {
            // Runs on every prompt, so it stays off the paths that probe the
            // service, open the data directory, or load the embedder.
            CliCommand::Hook {
                url,
                format,
                timeout_ms,
            } => {
                memnest::hook::run(url.as_deref(), *format, *timeout_ms).await;
            }
            // Talks to the service over HTTP like any other adapter, so it also
            // stays off the paths that open the data directory.
            CliCommand::Watch {
                url,
                paths,
                once,
                interval,
                backfill,
            } => {
                memnest::watch::run(
                    url.as_deref(),
                    paths,
                    &config.data_dir,
                    *once,
                    *interval,
                    *backfill,
                )
                .await?;
            }
            CliCommand::Status => {
                let reachable = service_reachable(&config.api_host, config.api_port);
                println!("memnest v{}", env!("CARGO_PKG_VERSION"));
                println!(
                    "service: {}",
                    if reachable {
                        "reachable"
                    } else {
                        "not reachable"
                    }
                );
                println!("dashboard: {}", dashboard_url(&config));
                println!("data dir: {}", config.data_dir.display());
            }
            CliCommand::Dashboard => println!("{}", dashboard_url(&config)),
        }
        return Ok(());
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
            .encode_document("memnest embedding warmup")?;
        println!(
            "embedding warmup complete: model={}, data_dir={}",
            config.embed_model,
            config.data_dir.display()
        );
        return Ok(());
    }

    if cli.doctor {
        println!("memnest doctor v{}", env!("CARGO_PKG_VERSION"));
        println!("  data dir: {:?}", config.data_dir);
        println!();
        let checks = memnest::doctor::run(&config).await?;
        let exit_code = memnest::doctor::print_report(&checks);
        std::process::exit(exit_code);
    }

    if !cli.mcp && cli.import_jsonl.is_none() {
        enforce_bind_safety(&config.api_host)?;
    }

    info!("Starting memnest v{}", env!("CARGO_PKG_VERSION"));
    info!("Data dir: {:?}", config.data_dir);

    let system = Arc::new(tokio::sync::RwLock::new(
        MemorySystem::new(config.clone()).await?,
    ));

    if let Some(path) = cli.import_jsonl.as_deref() {
        let imported_memories = import_jsonl(system.clone(), path).await?;
        println!("imported {} memories", imported_memories);
        return Ok(());
    }

    if cli.mcp {
        info!("MCP stdio server enabled");
        memnest::server::mcp::run_stdio(system.clone()).await?;
        return Ok(());
    }

    // Kick off lifecycle pruning for expiring filtered data and trash GC.
    // Conversation AutoLog is retained until explicitly deleted.
    if config.enable_lifecycle {
        memnest::lifecycle::spawn_periodic_lifecycle(system.clone());
    } else {
        info!("lifecycle prune loop disabled (enable_lifecycle=false)");
    }

    // Start API server
    let app = memnest::server::create_router(system.clone());
    let addr = format!("{}:{}", config.api_host, config.api_port);
    info!("API server starting on http://{}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    let server = axum::serve(listener, app);

    info!("memnest ready");

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

fn dashboard_url(config: &Config) -> String {
    format!(
        "http://{}:{}/",
        canonical_display_host(&config.api_host),
        config.api_port
    )
}

fn canonical_display_host(host: &str) -> String {
    if matches!(host, "127.0.0.1" | "0.0.0.0" | "::" | "::1") {
        "localhost".to_string()
    } else if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]")
    } else {
        host.to_string()
    }
}

fn service_reachable(host: &str, port: u16) -> bool {
    let probe_host = if matches!(host, "0.0.0.0" | "::") {
        "localhost"
    } else {
        host
    };
    (probe_host, port)
        .to_socket_addrs()
        .ok()
        .into_iter()
        .flatten()
        .any(|address| {
            std::net::TcpStream::connect_timeout(&address, std::time::Duration::from_millis(400))
                .is_ok()
        })
}

fn bind_is_safe(host: &str, token: Option<String>) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1")
        || memnest::server::normalize_token(token).is_some()
}

fn enforce_bind_safety(host: &str) -> anyhow::Result<()> {
    if bind_is_safe(host, std::env::var("MEMNEST_TOKEN").ok()) {
        return Ok(());
    }
    bail!(
        "refusing to bind to {host} without authentication; set MEMNEST_TOKEN or bind to 127.0.0.1"
    )
}

fn backup_data_dir(source: &Path, target: &Path) -> anyhow::Result<()> {
    validate_source_target(source, target)?;
    if target.exists() {
        bail!("backup target already exists: {}", target.display());
    }
    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let staging = sibling_temp_path(target, "backup-staging");
    let result = (|| -> anyhow::Result<()> {
        create_consistent_snapshot(source, &staging)?;
        validate_backup_dir(&staging)?;
        std::fs::rename(&staging, target)?;
        Ok(())
    })();
    if staging.exists() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result.with_context(|| format!("failed to backup {}", source.display()))
}

fn restore_data_dir(source: &Path, target: &Path, force: bool) -> anyhow::Result<()> {
    validate_source_target(source, target)?;
    if target.exists() && !target.is_dir() {
        bail!("data directory is not a directory: {}", target.display());
    }
    if target.exists()
        && std::fs::read_dir(target)
            .with_context(|| format!("cannot read data directory {}", target.display()))?
            .next()
            .is_some()
        && !force
    {
        bail!(
            "data directory is not empty: {}; pass --force to replace it",
            target.display()
        );
    }

    let parent = target.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let _target_lock = memnest::acquire_writer_lock(target)
        .context("cannot restore while another memnest writer is active")?;
    let _legacy_target_lock = acquire_legacy_writer_lock(target)?;
    let staging = sibling_temp_path(target, "restore-staging");
    if let Err(error) =
        create_consistent_snapshot(source, &staging).and_then(|_| validate_backup_dir(&staging))
    {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(error)
            .with_context(|| format!("failed to stage restore from {}", source.display()));
    }

    let previous = sibling_temp_path(target, "restore-previous");
    let had_target = target.exists();
    if had_target {
        std::fs::rename(target, &previous)?;
    }
    if let Err(error) = std::fs::rename(&staging, target) {
        if had_target {
            let _ = std::fs::rename(&previous, target);
        }
        let _ = std::fs::remove_dir_all(&staging);
        return Err(error).context("failed to install staged restore");
    }
    if had_target {
        std::fs::remove_dir_all(previous)?;
    }
    Ok(())
}

fn acquire_legacy_writer_lock(target: &Path) -> anyhow::Result<Option<std::fs::File>> {
    let path = target.join(".writer.lock");
    if !path.exists() {
        return Ok(None);
    }
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?;
    lock.try_lock_exclusive()
        .context("cannot restore while a legacy memnest writer is active")?;
    Ok(Some(lock))
}

fn create_consistent_snapshot(source: &Path, target: &Path) -> anyhow::Result<()> {
    std::fs::create_dir_all(target)?;
    for entry in
        std::fs::read_dir(source).with_context(|| format!("failed to read {}", source.display()))?
    {
        let entry = entry?;
        let name = entry.file_name();
        let name_text = name.to_string_lossy();
        if matches!(
            name_text.as_ref(),
            "memory.db"
                | "memory.db-wal"
                | "memory.db-shm"
                | "vectors"
                | "text_index"
                | "models"
                | ".writer.lock"
        ) || name_text.starts_with(".vectors.")
            || name_text.starts_with(".text_index.")
        {
            continue;
        }
        copy_entry(&entry.path(), &target.join(name))?;
    }

    let database = source.join("memory.db");
    if !database.is_file() {
        bail!("backup source has no memory.db: {}", source.display());
    }
    let destination = target.join("memory.db");
    let connection = rusqlite::Connection::open(&database)?;
    connection.execute(
        "VACUUM INTO ?1",
        rusqlite::params![destination.to_string_lossy().as_ref()],
    )?;
    Ok(())
}

fn copy_entry(source: &Path, target: &Path) -> anyhow::Result<()> {
    let metadata = std::fs::symlink_metadata(source)?;
    if metadata.file_type().is_symlink() {
        bail!(
            "refusing to copy symlink in data directory: {}",
            source.display()
        );
    }
    if metadata.is_dir() {
        std::fs::create_dir_all(target)?;
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            copy_entry(&entry.path(), &target.join(entry.file_name()))?;
        }
    } else if metadata.is_file() {
        std::fs::copy(source, target)?;
    }
    Ok(())
}

fn validate_backup_dir(path: &Path) -> anyhow::Result<()> {
    let database = path.join("memory.db");
    let connection = rusqlite::Connection::open_with_flags(
        &database,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )?;
    let check: String = connection.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    if check != "ok" {
        bail!("SQLite quick_check failed: {check}");
    }

    let mut encrypted = Vec::new();
    if sqlite_table_exists(&connection, "secrets")? {
        let mut stmt = connection.prepare("SELECT key, value FROM secrets")?;
        let rows = stmt.query_map([], |row| {
            let key: String = row.get(0)?;
            let value: String = row.get(1)?;
            Ok((format!("secret:{key}"), value))
        })?;
        encrypted.extend(rows.collect::<rusqlite::Result<Vec<_>>>()?);
    }
    if sqlite_table_exists(&connection, "servers")? {
        let mut stmt = connection.prepare("SELECT name, password FROM servers")?;
        let rows = stmt.query_map([], |row| {
            let name: String = row.get(0)?;
            let value: String = row.get(1)?;
            Ok((format!("server:{name}"), value))
        })?;
        encrypted.extend(rows.collect::<rusqlite::Result<Vec<_>>>()?);
    }
    if !encrypted.is_empty() {
        let key = std::env::var("MEMNEST_MASTER_KEY")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.trim().to_string())
            .or_else(|| std::fs::read_to_string(path.join("master.key")).ok())
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("backup contains vault rows but no master key"))?;
        memnest::crypto::validate_ciphertexts(&key, &encrypted)?;
    }
    Ok(())
}

fn sqlite_table_exists(connection: &rusqlite::Connection, table: &str) -> anyhow::Result<bool> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
        rusqlite::params![table],
        |row| row.get(0),
    )?)
}

fn validate_source_target(source: &Path, target: &Path) -> anyhow::Result<()> {
    if !source.is_dir() {
        bail!("data directory does not exist: {}", source.display());
    }
    let source = canonical_candidate(source)?;
    let target = canonical_candidate(target)?;
    if source.starts_with(&target) || target.starts_with(&source) {
        bail!(
            "source and target directories must not overlap: {} and {}",
            source.display(),
            target.display()
        );
    }
    Ok(())
}

fn canonical_candidate(path: &Path) -> anyhow::Result<PathBuf> {
    let mut cursor = path;
    let mut tail = Vec::new();
    while !cursor.exists() {
        let name = cursor
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("cannot resolve path {}", path.display()))?;
        tail.push(name.to_os_string());
        cursor = cursor
            .parent()
            .ok_or_else(|| anyhow::anyhow!("cannot resolve path {}", path.display()))?;
    }
    let mut resolved = std::fs::canonicalize(cursor)?;
    for part in tail.iter().rev() {
        resolved.push(part);
    }
    Ok(resolved)
}

fn sibling_temp_path(path: &Path, label: &str) -> PathBuf {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("memnest");
    parent.join(format!(".{name}.{label}-{}", uuid::Uuid::new_v4().simple()))
}

async fn import_jsonl(
    system: Arc<tokio::sync::RwLock<MemorySystem>>,
    path: &str,
) -> anyhow::Result<usize> {
    let file = std::fs::File::open(path)?;
    let reader = BufReader::new(file);
    let sys = system.read().await;
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
                &uuid::Uuid::new_v4().to_string().replace('-', "")[..16]
            )
        });
        let created_at = parse_import_timestamp(rec.created_at.as_deref());
        let embedding = match rec.embedding {
            Some(v) if !v.is_empty() => Some(v),
            _ => Some(sys.embedder.encode_document(&document)?),
        };
        let mut metadata = rec.metadata.unwrap_or(Metadata {
            chunk_type: ChunkType::AutoLog,
            importance: Importance::Log,
            ..Default::default()
        });
        metadata.raw_chunk = None;
        let chunk = MemoryChunk {
            id: id.clone(),
            project: project.clone(),
            document,
            embedding,
            metadata,
            created_at,
            updated_at: created_at,
        };
        sys.db.write().await.insert_chunk(&chunk)?;
        count += 1;
    }
    sys.sync_pending_indexes().await?;
    Ok(count)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn write_test_database(path: &Path, value: &str) {
        std::fs::create_dir_all(path).unwrap();
        let connection = rusqlite::Connection::open(path.join("memory.db")).unwrap();
        connection
            .execute_batch("CREATE TABLE marker (value TEXT NOT NULL);")
            .unwrap();
        connection
            .execute(
                "INSERT INTO marker (value) VALUES (?1)",
                rusqlite::params![value],
            )
            .unwrap();
    }

    fn read_test_marker(path: &Path) -> String {
        rusqlite::Connection::open(path.join("memory.db"))
            .unwrap()
            .query_row("SELECT value FROM marker", [], |row| row.get(0))
            .unwrap()
    }

    #[test]
    fn backup_rejects_overlaps_and_omits_rebuildable_indexes() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("data");
        write_test_database(&source, "source");
        std::fs::create_dir_all(source.join("vectors")).unwrap();
        std::fs::write(source.join("vectors/index.hnsw"), "derived").unwrap();

        assert!(backup_data_dir(&source, &source.join("nested-backup")).is_err());
        let backup = temp.path().join("backup");
        backup_data_dir(&source, &backup).unwrap();
        assert_eq!(read_test_marker(&backup), "source");
        assert!(!backup.join("vectors").exists());
    }

    #[test]
    fn failed_restore_keeps_existing_target_and_valid_restore_swaps_it() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("target");
        write_test_database(&target, "old");
        let invalid = temp.path().join("invalid");
        std::fs::create_dir_all(&invalid).unwrap();
        std::fs::write(invalid.join("memory.db"), "not sqlite").unwrap();

        assert!(restore_data_dir(&invalid, &target, true).is_err());
        assert_eq!(read_test_marker(&target), "old");

        let source = temp.path().join("source");
        write_test_database(&source, "new");
        let active_writer = memnest::acquire_writer_lock(&target).unwrap();
        assert!(restore_data_dir(&source, &target, true).is_err());
        assert_eq!(read_test_marker(&target), "old");
        drop(active_writer);
        assert!(restore_data_dir(&source, &target, false).is_err());
        restore_data_dir(&source, &target, true).unwrap();
        assert_eq!(read_test_marker(&target), "new");
    }

    #[test]
    fn dashboard_hosts_are_safe_for_urls() {
        assert_eq!(canonical_display_host("127.0.0.1"), "localhost");
        assert_eq!(canonical_display_host("0.0.0.0"), "localhost");
        assert_eq!(canonical_display_host("192.168.1.20"), "192.168.1.20");
        assert_eq!(canonical_display_host("2001:db8::1"), "[2001:db8::1]");
    }

    #[test]
    fn external_bind_requires_non_empty_token() {
        assert!(!bind_is_safe("0.0.0.0", None));
        assert!(!bind_is_safe("0.0.0.0", Some("   ".into())));
        assert!(bind_is_safe("0.0.0.0", Some(" token ".into())));
        assert!(bind_is_safe("127.0.0.1", None));
    }
}
