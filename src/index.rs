//! Rebuildable per-working-view SQLite. Authored files remain the only source.
mod current;
mod git;
pub(crate) mod residue;
pub(crate) use current::{cached, refresh_current, with_current};
use crate::{Policy, Result, require, sha256};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

/// Fixed failure identity with optional safe native path/field context.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Refusal {
    pub stage: &'static str,
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<crate::source_error::SourceError>,
}
impl Refusal {
    pub(crate) fn changed_source() -> Self {
        Self { stage: "source_freshness", kind: "source_changed", problem: None }
    }
    pub(crate) fn is_changed_source(error: &(dyn std::error::Error + 'static)) -> bool {
        error.downcast_ref::<Self>().is_some_and(|e|
            e.stage == "source_freshness" && e.kind == "source_changed")
    }
    pub fn from_error(stage: &'static str, error: &(dyn std::error::Error + 'static)) -> Self {
        let kind = error_kind(error);
        Self {
            stage,
            kind: if kind == "rejected" { "unknown" } else { kind },
            problem: error
                .downcast_ref::<crate::source_error::SourceError>()
                .cloned(),
        }
    }
}
impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "retrieval refused ({}: {})", self.stage, self.kind)?;
        if let Some(problem) = &self.problem {
            write!(f, ": {problem}")?;
        }
        if self.kind == "source_changed" {
            write!(f, ". Retry after concurrent publication or editing finishes.")?;
        }
        Ok(())
    }
}
impl std::error::Error for Refusal {}

#[cfg(test)]
mod refusal_kind_tests {
    use super::Refusal;

    /// A refusal kind must come from the fixed identity the checked native
    /// helper path produces, never from a guess. These are those exact
    /// identities, so the mapping needs no subprocess and no real deadline.
    #[test]
    fn native_timeout_and_failure_identities_map_to_their_refusal_kinds() {
        for (message, kind) in [
            ("policy process timed out", "timeout"),
            ("barrier expired", "timeout"),
            ("inspection barrier expired", "timeout"),
            ("refresh lock expired", "timeout"),
            ("policy process exited nonzero", "helper_failed"),
        ] {
            let error: Box<dyn std::error::Error> = message.into();
            assert_eq!(
                Refusal::from_error("source_validation", error.as_ref()).kind,
                kind,
                "{message}"
            );
        }
        let timed_out = std::io::Error::from(std::io::ErrorKind::TimedOut);
        assert_eq!(
            Refusal::from_error("source_validation", &timed_out).kind,
            "timeout"
        );
    }
}

// Retrieval classification is restricted to typed errors and fixed identities.
fn error_kind(error: &(dyn std::error::Error + 'static)) -> &'static str {
    if let Some(refusal) = error.downcast_ref::<Refusal>() {
        return refusal.kind;
    }
    if let Some(problem) = error.downcast_ref::<crate::source_error::SourceError>() {
        return problem.kind;
    }
    if error.is::<serde_json::Error>() {
        return "invalid_request";
    }
    if let Some(error) = error.downcast_ref::<std::io::Error>() {
        return match error.kind() {
            std::io::ErrorKind::NotFound => "not_found",
            std::io::ErrorKind::PermissionDenied => "permission_denied",
            std::io::ErrorKind::StorageFull
            | std::io::ErrorKind::FileTooLarge
            | std::io::ErrorKind::OutOfMemory => "resource_exhausted",
            std::io::ErrorKind::TimedOut => "timeout",
            std::io::ErrorKind::WouldBlock => "busy",
            _ => "io",
        };
    }
    if let Some(rusqlite::Error::SqliteFailure(error, _)) = error.downcast_ref() {
        return match error.code {
            rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked => "busy",
            rusqlite::ErrorCode::DiskFull
            | rusqlite::ErrorCode::OutOfMemory
            | rusqlite::ErrorCode::TooBig => "resource_exhausted",
            rusqlite::ErrorCode::PermissionDenied | rusqlite::ErrorCode::ReadOnly => {
                "permission_denied"
            }
            _ => "sqlite",
        };
    }
    let text = error.to_string();
    if text.starts_with(crate::CANDIDATE_CHANGED_PREFIX) {
        return "runtime_changed";
    }
    match text.as_str() {
        "policy process timed out"
        | "barrier expired"
        | "inspection barrier expired"
        | "refresh lock expired" => "timeout",
        "oversized request"
        | "oversized search"
        | "oversized node file"
        | "oversized authored file"
        | "policy process output exceeds limit" => "resource_exhausted",
        "unsafe or oversized authored file" => "unsafe_or_oversized_source",
        "policy process exited nonzero" => "helper_failed",
        "unexpected policy stderr" | "malformed checked graph verdict" => "helper_invalid_output",
        "checked graph rejected or malformed verdict" => "invalid_source",
        "changed source identity"
        | "changed base identity"
        | "changed base bytes"
        | "changed authored bytes" => "source_changed",
        "candidate file set changed; rebuild and recheck" => "runtime_changed",
        _ => "rejected",
    }
}

/// Retired observer barrier shape. Native search explicitly refuses this adapter.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PauseBeforeAccept {
    pub ready: PathBuf,
    pub release: PathBuf,
}

/// Legacy receipt shape retained for checked-publication serialization only.
#[derive(Default, Debug, Serialize)]
pub struct Reuse {
    pub accepted_main: usize,
    pub prepared_worktree: usize,
    pub computed: usize,
    pub changed_dependencies: usize,
    pub search_bodies_reused: usize,
    pub changed_rows: usize,
    pub sqlite_changes: u64,
}

fn processing_identity() -> &'static str {
    static ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ID.get_or_init(|| {
        sha256(
            concat!(
                "sifttext-index/1;o200k_base;tiktoken-rs/0.6.0;",
                include_str!("lib.rs"),
                include_str!("coherent_capture.rs"),
                include_str!("index.rs"),
                include_str!("index/current.rs"),
                include_str!("index/git.rs"),
                include_str!("git_snapshot.rs"),
                include_str!("index/residue.rs"),
                include_str!("source_version.rs"),
                include_str!("status.rs"),
                include_str!("search.rs"),
                include_str!("search_document.rs"),
                include_str!("token_metadata.rs"),
                include_str!("token_count.rs"),
                include_str!("vendor/leviathan/search.rs"),
                include_str!("vendor/leviathan/query_text.rs"),
                include_str!("vendor/leviathan/relevance.rs"),
                include_str!("vendor/leviathan/card.rs"),
                include_str!("vendor/leviathan/snippet.rs"),
                include_str!("../Cargo.lock")
            )
            .as_bytes(),
        )
    })
}

fn project_body(row: &Value) -> Result<String> {
    Ok(
        crate::search_document::project(crate::search_document::Fields {
            name: row["name"].as_str().ok_or("missing name")?,
            micro_summary: row["micro_summary"].as_str().ok_or("missing summary")?,
            scope: row["scope"].as_str().unwrap_or(""),
            content: row["content"].as_str().ok_or("missing content")?,
        })?
        .body,
    )
}

type Identity = (u64, u64, i64, i64);
fn identity(path: &Path) -> Result<Identity> {
    let m = fs::symlink_metadata(path)?;
    Ok((m.dev(), m.ino(), m.ctime(), m.ctime_nsec()))
}

/// Explicitly repair generated outputs after loss or damage; authored knowledge is unchanged.
pub fn repair(policy: &Policy, path: &Path) -> Result<Value> {
    let (version, nodes) = current::repair(policy, path)?;
    Ok(serde_json::json!({"status":"current", "version":version, "nodes":nodes}))
}

/// Project the requested native outline from the current working-view transaction.
pub fn outline(policy: &Policy, path: &Path, id: &str, full: bool) -> Result<String> {
    let root = working_path(path, None)?;
    Ok(with_current(policy, &root, |tx, version| {
        crate::outlines::indexed_outline(tx, &root, id, full, version)
    })?.1)
}

// Check the root boundary before any derived write, without traversing live artifacts.
fn working_path(path: &Path, base: Option<&Path>) -> Result<PathBuf> {
    require(
        base.is_none(),
        "unsupported old sparse adapter; use native Git sparse checkout",
    )?;
    crate::real_dir(path)?;
    Ok(fs::canonicalize(path)?)
}
