use anyhow::{Result, bail};
use sha2::{Digest, Sha256};

use crate::models::is_internal_project;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkspaceIdentity {
    pub id: String,
    pub display_name: String,
    pub legacy_project: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchScope {
    All,
    Projects {
        primary: String,
        allowed: Vec<String>,
    },
}

impl SearchScope {
    pub fn explicit(project: impl Into<String>) -> Self {
        let project = project.into();
        if project == "all" {
            Self::All
        } else {
            Self::Projects {
                primary: project.clone(),
                allowed: vec![project],
            }
        }
    }

    pub fn primary(&self) -> &str {
        match self {
            Self::All => "all",
            Self::Projects { primary, .. } => primary,
        }
    }

    pub fn projects(&self) -> Option<&[String]> {
        match self {
            Self::All => None,
            Self::Projects { allowed, .. } => Some(allowed),
        }
    }
}

pub fn identity(cwd: &str) -> Result<WorkspaceIdentity> {
    let normalized = normalize_absolute(cwd)?;
    let display_name = normalized
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or("workspace")
        .to_string();
    let slug: String = display_name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .take(30)
        .collect();
    let slug = slug.trim_matches('-');
    let slug = if slug.is_empty() { "workspace" } else { slug };
    let digest = format!("{:x}", Sha256::digest(normalized.as_bytes()));
    let legacy_project = if is_reserved_legacy(&display_name) {
        None
    } else {
        Some(display_name.clone())
    };
    Ok(WorkspaceIdentity {
        id: format!("ws_{slug}_{}", &digest[..32]),
        display_name,
        legacy_project,
    })
}

fn normalize_absolute(cwd: &str) -> Result<String> {
    let replaced = cwd.trim().replace('\\', "/");
    let (prefix, rest) = if let Some(rest) = replaced.strip_prefix('/') {
        ("/".to_string(), rest)
    } else if replaced.len() >= 3
        && replaced.as_bytes()[0].is_ascii_alphabetic()
        && replaced.as_bytes()[1] == b':'
        && replaced.as_bytes()[2] == b'/'
    {
        (
            format!("{}:/", replaced[..1].to_ascii_lowercase()),
            &replaced[3..],
        )
    } else {
        bail!("cwd must be an absolute path");
    };

    let mut parts: Vec<&str> = Vec::new();
    for part in rest.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            value => parts.push(value),
        }
    }
    Ok(format!("{prefix}{}", parts.join("/")))
}

fn is_reserved_legacy(project: &str) -> bool {
    is_internal_project(project) || matches!(project, "all" | "root" | "default" | "global")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_basename_different_paths_get_different_ids() {
        let first = identity("/work/client-a/api").unwrap();
        let second = identity("/personal/api").unwrap();
        assert_ne!(first.id, second.id);
        assert_eq!(first.display_name, "api");
        assert!(first.id.starts_with("ws_api_"));
    }

    #[test]
    fn separators_and_dot_segments_are_normalized() {
        assert_eq!(
            identity("/work//api/./src/../").unwrap().id,
            identity("/work/api").unwrap().id
        );
        assert_eq!(
            identity(r"C:\\Users\\me\\api").unwrap().id,
            identity("c:/Users/me/api/").unwrap().id
        );
    }

    #[test]
    fn inferred_identity_never_uses_reserved_sentinels() {
        let all = identity("/work/all").unwrap();
        assert_ne!(all.id, "all");
        assert_eq!(all.legacy_project, None);
        assert!(identity("relative/path").is_err());
    }
}
