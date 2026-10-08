//! Opt-in working-file fingerprints. Only the local CLI reads files; the server
//! stores caller-supplied metadata and never executes a memory or checks its truth.
use anyhow::{Result, anyhow, bail, ensure};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::models::{Approach, ChunkType, Importance, Metadata};

const MAX_FILES: usize = 16;
const MAX_PATH_BYTES: usize = 256;
const MAX_FILE_BYTES: u64 = 1024 * 1024;
const MAX_REPLY_BYTES: usize = 256 * 1024;
pub const ASSERTION: &str = "caller_recorded_fingerprints_not_solution_verification";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct CodeEvidence {
    pub workspace_id: String,
    pub captured_at: String,
    pub git_head: Option<String>,
    pub files: Vec<FileFingerprint>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FileFingerprint {
    pub path: String,
    pub sha256: String,
}

fn hex(value: &str, lengths: &[usize]) -> bool {
    lengths.contains(&value.len())
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

fn validate_path(path: &str) -> Result<()> {
    ensure!(
        !path.is_empty()
            && path.len() <= MAX_PATH_BYTES
            && !path
                .chars()
                .any(|c| c.is_control() || matches!(c, '\\' | ':'))
            && path.split('/').all(|p| !matches!(p, "" | "." | "..")
                && !p.eq_ignore_ascii_case(".git")
                && !p.ends_with(['.', ' ']))
            && crate::redaction::redact_text(path) == path,
        "code evidence requires safe relative file paths (at most 256 bytes)"
    );
    Ok(())
}

impl CodeEvidence {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.workspace_id.starts_with("ws_")
                && self.workspace_id.len() <= 80
                && self
                    .workspace_id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-')),
            "invalid code evidence workspace"
        );
        ensure!(
            self.captured_at.len() <= 64
                && chrono::DateTime::parse_from_rfc3339(&self.captured_at).is_ok(),
            "invalid code evidence timestamp"
        );
        ensure!(
            self.git_head.as_deref().is_none_or(|v| hex(v, &[40, 64])),
            "invalid code evidence Git revision"
        );
        ensure!(
            !self.files.is_empty() && self.files.len() <= MAX_FILES,
            "code evidence requires 1 to 16 files"
        );
        let mut seen = HashSet::new();
        for file in &self.files {
            validate_path(&file.path)?;
            ensure!(
                seen.insert(&file.path) && hex(&file.sha256, &[64]),
                "duplicate path or invalid code evidence digest"
            );
        }
        Ok(())
    }
}

pub fn view(evidence: &CodeEvidence) -> Value {
    if evidence.validate().is_err() {
        return json!({"assertion": ASSERTION, "status": "invalid_metadata"});
    }
    let mut value = json!(evidence);
    value["assertion"] = json!(ASSERTION);
    value
}

pub fn schema() -> Value {
    json!({"type":"object","additionalProperties":false,
        "properties":{
            "workspace_id":{"type":"string","maxLength":80},
            "captured_at":{"type":"string","maxLength":64},
            "git_head":{"type":["string","null"],"maxLength":64},
            "files":{"type":"array","minItems":1,"maxItems":MAX_FILES,"items":{
                "type":"object","additionalProperties":false,"properties":{
                    "path":{"type":"string","maxLength":MAX_PATH_BYTES},
                    "sha256":{"type":"string","pattern":"^[0-9a-f]{64}$"}
                },"required":["path","sha256"]}}
        },"required":["workspace_id","captured_at","git_head","files"]})
}

fn workspace(root: &Path) -> Result<String> {
    crate::workspace::identity(
        root.to_str()
            .ok_or_else(|| anyhow!("workspace must be UTF-8"))?,
    )
    .map(|v| v.id)
}

// No symlinks, directory scans, or file contents in the response. This is a
// trusted local workstation operation, not a sandbox against local file races.
fn digest(root: &Path, path: &str) -> Result<String> {
    validate_path(path)?;
    let mut candidate = root.to_path_buf();
    for part in path.split('/') {
        candidate.push(part);
        ensure!(
            !std::fs::symlink_metadata(&candidate)?
                .file_type()
                .is_symlink(),
            "symlink not allowed"
        );
    }
    ensure!(
        std::fs::symlink_metadata(&candidate)?.is_file(),
        "regular file required"
    );
    let file = std::fs::File::open(candidate)?;
    ensure!(file.metadata()?.is_file(), "regular file required");
    let mut bytes = Vec::new();
    file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes)?;
    ensure!(bytes.len() as u64 <= MAX_FILE_BYTES, "file exceeds 1 MiB");
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

async fn git_head(root: &Path) -> Option<String> {
    // rev-parse runs no remembered command, hook, diff driver, or shell. A HEAD
    // is context only: fingerprints describe the working files, including edits.
    let output = tokio::time::timeout(
        Duration::from_secs(2),
        tokio::process::Command::new("git")
            .args(["--no-optional-locks", "-c", "core.fsmonitor=false", "-C"])
            .arg(root)
            .args(["rev-parse", "--verify", "HEAD"])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .ok()?
    .ok()?;
    if !output.status.success() {
        return None;
    }
    let value = std::str::from_utf8(&output.stdout).ok()?.trim();
    hex(value, &[40, 64]).then(|| value.to_string())
}

async fn capture(root: &Path, paths: &[String]) -> Result<CodeEvidence> {
    ensure!(
        !paths.is_empty() && paths.len() <= MAX_FILES,
        "choose 1 to 16 files"
    );
    let mut files = Vec::new();
    for path in paths {
        files.push(FileFingerprint {
            path: path.clone(),
            sha256: digest(root, path).map_err(|_| {
                anyhow!(
                    "cannot fingerprint selected file; use regular non-symlink files up to 1 MiB"
                )
            })?,
        });
    }
    let evidence = CodeEvidence {
        workspace_id: workspace(root)?,
        captured_at: chrono::Utc::now().to_rfc3339(),
        git_head: git_head(root).await,
        files,
    };
    evidence.validate()?;
    Ok(evidence)
}

fn compare(root: &Path, evidence: &CodeEvidence) -> Result<Value> {
    evidence.validate()?;
    if workspace(root)? != evidence.workspace_id {
        return Ok(json!({"status":"different_workspace","files":[],"assertion":ASSERTION}));
    }
    let files: Vec<_> = evidence
        .files
        .iter()
        .map(|file| {
            let status = match digest(root, &file.path) {
                Ok(current) if current == file.sha256 => "unchanged",
                Ok(_) => "changed",
                // Check absence without treating a symlink or unreadable path as unchanged.
                Err(_) => {
                    if std::fs::symlink_metadata(root.join(&file.path))
                        .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
                    {
                        "missing"
                    } else {
                        "unavailable"
                    }
                }
            };
            json!({"path":file.path,"status":status})
        })
        .collect();
    let status = if files.iter().all(|v| v["status"] == "unchanged") {
        "unchanged"
    } else if files.iter().any(|v| v["status"] == "unavailable") {
        "unavailable"
    } else {
        "changed"
    };
    Ok(json!({"status":status,"files":files,"assertion":ASSERTION,
        "captured_at":evidence.captured_at,"saved_git_head":evidence.git_head,
        "scope":"Only selected file bytes were compared. Changes do not invalidate the solution; unchanged files do not verify it."}))
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Capture selected files locally without reading or writing the memory service.
    Capture {
        #[arg(long = "file", required = true)]
        files: Vec<String>,
    },
    /// Compare a bounded code_evidence JSON object from stdin, without network calls.
    Compare,
    /// Save a caller-reported approach with fingerprints of explicitly selected working files.
    Remember {
        #[arg(long)]
        text: String,
        #[arg(long = "file", required = true)]
        files: Vec<String>,
        #[arg(long, value_parser = ["proposed", "failed", "reported_success"])]
        status: String,
        #[arg(long)]
        applicability: String,
        /// Observed result text, not independent verification.
        #[arg(long, default_value = "")]
        evidence: String,
        #[arg(long)]
        project: Option<String>,
        #[arg(long)]
        supersedes: Option<String>,
    },
    /// Read one saved baseline and compare only its selected files. No memory is changed.
    Check { id: String },
}

fn service(url: Option<&str>) -> Result<(reqwest::Client, reqwest::Url)> {
    let raw = url
        .map(str::to_string)
        .or_else(|| std::env::var("MEMNEST_URL").ok())
        .unwrap_or_else(|| "http://127.0.0.1:3111".into());
    let base = reqwest::Url::parse(&raw).map_err(|_| anyhow!("invalid Memnest URL"))?;
    ensure!(
        base.scheme() == "http"
            && matches!(base.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"))
            && base.username().is_empty()
            && base.password().is_none()
            && base.path() == "/"
            && base.query().is_none()
            && base.fragment().is_none(),
        "code evidence CLI requires a loopback HTTP service URL without credentials, path, query, or fragment"
    );
    let mut headers = reqwest::header::HeaderMap::new();
    if let Some(token) = std::env::var("MEMNEST_TOKEN")
        .ok()
        .filter(|v| !v.trim().is_empty())
    {
        let mut value = reqwest::header::HeaderValue::from_str(&format!("Bearer {}", token.trim()))
            .map_err(|_| anyhow!("invalid Memnest token"))?;
        value.set_sensitive(true);
        headers.insert(reqwest::header::AUTHORIZATION, value);
    }
    Ok((
        reqwest::Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(30))
            .default_headers(headers)
            .build()?,
        base,
    ))
}

async fn request(
    client: &reqwest::Client,
    url: reqwest::Url,
    body: Option<Value>,
) -> Result<Value> {
    let builder = if let Some(body) = body {
        client.post(url).json(&body)
    } else {
        client.get(url)
    };
    let mut response = builder
        .send()
        .await
        .map_err(|_| anyhow!("Memnest request failed"))?;
    ensure!(
        response.status().is_success(),
        "Memnest request failed (HTTP {})",
        response.status().as_u16()
    );
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| anyhow!("Memnest response read failed"))?
    {
        ensure!(
            bytes.len() + chunk.len() <= MAX_REPLY_BYTES,
            "Memnest response too large"
        );
        bytes.extend_from_slice(&chunk);
    }
    serde_json::from_slice(&bytes).map_err(|_| anyhow!("invalid Memnest response"))
}

pub async fn run(command: &Command, url: Option<&str>, cwd: Option<&Path>) -> Result<Value> {
    let root = std::fs::canonicalize(cwd.map(PathBuf::from).unwrap_or(std::env::current_dir()?))?;
    ensure!(root.is_dir(), "workspace directory required");
    match command {
        Command::Capture { files } => Ok(json!(capture(&root, files).await?)),
        Command::Compare => {
            let mut bytes = Vec::new();
            std::io::stdin()
                .take(16 * 1024 + 1)
                .read_to_end(&mut bytes)?;
            ensure!(
                bytes.len() <= 16 * 1024,
                "code evidence input exceeds 16 KiB"
            );
            let mut value: Value = serde_json::from_slice(&bytes)
                .map_err(|_| anyhow!("invalid code evidence JSON"))?;
            if let Some(object) = value.as_object_mut() {
                object.remove("assertion");
            }
            let baseline: CodeEvidence = serde_json::from_value(value)
                .map_err(|_| anyhow!("invalid code evidence metadata"))?;
            let mut report = compare(&root, &baseline)?;
            report["current_git_head"] = json!(git_head(&root).await);
            Ok(report)
        }
        Command::Remember {
            text,
            files,
            status,
            applicability,
            evidence,
            project,
            supersedes,
        } => {
            let (client, base) = service(url)?;
            // Old servers ignore unknown metadata. Refuse before writing there.
            let health = request(&client, base.join("health")?, None).await?;
            ensure!(
                health["capabilities"]["code_evidence"] == true,
                "core does not support code evidence; upgrade it before saving"
            );
            let baseline = capture(&root, files).await?;
            let mut approach = Approach {
                status: serde_json::from_value(json!(status))?,
                applicability: applicability.clone(),
                evidence: evidence.clone(),
            };
            crate::server::operations::prepare_approach(&mut approach)?;
            let metadata = Metadata {
                chunk_type: ChunkType::Manual,
                importance: Importance::Knowledge,
                approach: Some(approach),
                code_evidence: Some(baseline),
                supersedes: supersedes.clone(),
                adapter: Some("code-evidence-cli".into()),
                ..Default::default()
            };
            request(&client, base.join("add")?, Some(json!({"text":text,"project":project.as_deref().unwrap_or(""),"cwd":root.to_str(),"metadata":metadata}))).await
        }
        Command::Check { id } => {
            let (client, base) = service(url)?;
            ensure!(
                !id.trim().is_empty() && id.len() <= 256,
                "memory id required (at most 256 bytes)"
            );
            let mut target = base.join("chunk/")?;
            target
                .path_segments_mut()
                .map_err(|_| anyhow!("invalid Memnest URL"))?
                .pop_if_empty()
                .push(id);
            target.set_query(Some("max_chars=1"));
            let row = request(&client, target, None).await?;
            ensure!(
                row["id"].as_str() == Some(id.as_str()),
                "Memnest returned a different memory"
            );
            if row.get("code_evidence").is_none() {
                bail!("core does not support code evidence; upgrade it before checking");
            }
            let mut report = if row["code_evidence"].is_null() {
                json!({"status":"not_recorded","files":[],"assertion":ASSERTION})
            } else {
                let mut saved = row["code_evidence"].clone();
                if let Some(object) = saved.as_object_mut() {
                    object.remove("assertion");
                }
                let baseline: CodeEvidence = serde_json::from_value(saved)
                    .map_err(|_| anyhow!("invalid saved code evidence"))?;
                compare(&root, &baseline)?
            };
            report["id"] = json!(id);
            report["current_git_head"] = json!(git_head(&root).await);
            // Only validated status values; never forward arbitrary stored commands.
            report["approach_status"] = match row["approach"]["status"].as_str() {
                Some(v @ ("proposed" | "failed" | "reported_success")) => json!(v),
                _ => Value::Null,
            };
            Ok(report)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn selected_files_only_and_workspace_boundaries() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        std::fs::write(root.join("code.rs"), "before").unwrap();
        let baseline = capture(&root, &["code.rs".into()]).await.unwrap();
        assert_eq!(compare(&root, &baseline).unwrap()["status"], "unchanged");
        std::fs::write(root.join("other.rs"), "unrelated change").unwrap();
        assert_eq!(compare(&root, &baseline).unwrap()["status"], "unchanged");
        std::fs::write(root.join("code.rs"), "after").unwrap();
        assert_eq!(
            compare(&root, &baseline).unwrap()["files"][0]["status"],
            "changed"
        );
        std::fs::remove_file(root.join("code.rs")).unwrap();
        assert_eq!(
            compare(&root, &baseline).unwrap()["files"][0]["status"],
            "missing"
        );
        let other = tempfile::tempdir().unwrap();
        assert_eq!(
            compare(other.path(), &baseline).unwrap()["status"],
            "different_workspace"
        );
    }

    #[tokio::test]
    async fn unsafe_or_unbounded_baselines_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("code.rs"), "ok").unwrap();
        let mut baseline = capture(temp.path(), &["code.rs".into()]).await.unwrap();
        for path in [
            "../outside",
            "/absolute",
            "a/../b",
            "a//b",
            "C:/file",
            "a\\b",
            ".git/config",
            ".GIT/config",
            "a. /file",
            "a\nfile",
        ] {
            baseline.files[0].path = path.into();
            assert!(baseline.validate().is_err());
        }
        baseline.files[0].path = "code.rs".into();
        baseline.files.push(baseline.files[0].clone());
        assert!(baseline.validate().is_err());
        std::fs::write(
            temp.path().join("big"),
            vec![0; MAX_FILE_BYTES as usize + 1],
        )
        .unwrap();
        assert!(digest(temp.path(), "big").is_err());
        assert!(service(Some("https://example.com")).is_err());
        assert!(service(Some("http://127.0.0.1:3111/?token=private")).is_err());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn symlinks_do_not_read_outside_the_workspace() {
        let temp = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::fs::write(
            outside.path().join("secret"),
            "not readable through evidence",
        )
        .unwrap();
        std::os::unix::fs::symlink(outside.path(), temp.path().join("linked")).unwrap();
        assert!(digest(temp.path(), "linked/secret").is_err());
    }
}
