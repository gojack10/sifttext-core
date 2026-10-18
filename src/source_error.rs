//! Safe native capture context: never render parser input, link targets or authored values.
use crate::Result;
use serde::Serialize;
use std::{error::Error, fmt, path::Path};

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct SourceError {
    pub kind: &'static str,
    pub path: String,
    pub field: Option<String>,
    pub reason: &'static str,
    pub next_action: &'static str,
}
impl fmt::Display for SourceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.path, self.reason)?;
        if let Some(field) = &self.field {
            write!(f, " (field {field})")?;
        }
        write!(f, "; {}", self.next_action)
    }
}
impl Error for SourceError {}

pub(crate) fn context<T>(result: Result<T>, path: &Path, field: Option<&str>) -> Result<T> {
    result.map_err(|error| {
        if error.is::<crate::candidate_validation::StructuralIssue>() {
            return error;
        }
        let mut issue = if let Some(issue) = error.downcast_ref::<SourceError>() {
            issue.clone()
        } else {
            let (kind, reason, next_action) = classify(error.as_ref());
            SourceError {
                kind,
                path: ".".into(),
                field: None,
                reason,
                next_action,
            }
        };
        if issue.path == "." {
            issue.path = path.to_string_lossy().into_owned();
        }
        if issue.field.is_none() {
            issue.field = field.map(str::to_owned);
        }
        Box::new(issue) as Box<dyn Error>
    })
}
fn classify(error: &(dyn Error + 'static)) -> (&'static str, &'static str, &'static str) {
    if let Some(error) = error.downcast_ref::<std::io::Error>() {
        return match error.kind() {
            std::io::ErrorKind::NotFound => (
                "not_found",
                "required path is missing",
                "check the supplied knowledge home and restore the missing path; do not reinitialize existing knowledge",
            ),
            std::io::ErrorKind::PermissionDenied => (
                "permission_denied",
                "permission denied",
                "check access permissions for this path; retain existing files",
            ),
            std::io::ErrorKind::StorageFull
            | std::io::ErrorKind::FileTooLarge
            | std::io::ErrorKind::OutOfMemory => (
                "resource_exhausted",
                "capture resource limit reached",
                "free resources and retry without changing authored knowledge",
            ),
            std::io::ErrorKind::TimedOut => (
                "timeout",
                "source read timed out",
                "retry when the filesystem is available; retain existing files",
            ),
            std::io::ErrorKind::WouldBlock => (
                "busy",
                "source is busy",
                "retry after the other operation finishes; retain existing files",
            ),
            _ => (
                "io",
                "unable to read path",
                "check filesystem access and retry; retain existing files",
            ),
        };
    }
    if error.is::<serde_json::Error>() {
        return (
            "invalid_capture",
            "metadata must contain a JSON value of the required type",
            "correct this metadata field in the retained file, then retry",
        );
    }
    if error.is::<std::str::Utf8Error>() {
        return (
            "invalid_capture",
            "file is not valid UTF-8",
            "repair the file encoding in the retained file, then retry",
        );
    }
    // Only native fixed guards are eligible for customer output, never arbitrary error text.
    let text = error.to_string();
    let reason = match text.as_str() {
        "missing Markdown metadata" => "missing Markdown metadata",
        "missing Markdown opening fence" => "missing Markdown opening fence",
        "missing Markdown closing fence" | "unterminated Markdown metadata" => {
            "missing Markdown closing fence"
        }
        "malformed metadata line" => "malformed metadata line",
        "malformed metadata key" => "malformed metadata key",
        "missing required metadata" => "missing required metadata",
        "duplicate metadata key" => "duplicate metadata key",
        "second authored hierarchy source" => {
            "hierarchy must use children.order and relationship links, not metadata"
        }
        "blank node name/micro-summary" => "blank node name or micro_summary",
        "blank relationship reason" => "blank relationship reason",
        "reference target is not a UUID" => "reference target is not a UUID",
        _ => "",
    };
    if !reason.is_empty() {
        return (
            "invalid_capture",
            reason,
            "correct this metadata in the retained file, then retry",
        );
    }
    let reason = match text.as_str() {
        "hard-linked authored file" => "authored file has multiple hard links",
        "unsafe or oversized authored file" | "oversized authored file" => {
            "authored file must be a regular file within the size limit"
        }
        "unsafe unrelated symlink" => "symlink is not an allowed tree relationship",
        "node directory is not a canonical full UUID" => {
            "node directory name must be a canonical full UUID"
        }
        "unsafe store directory" | "unsafe node directory" | "unsafe structural directory" => {
            "expected a real directory, not a file or symlink"
        }
        "nonempty node directory marker" => "nodes/.keep must be an empty regular file",
        "empty source-less node directory" | "unknown source-less node resource" => {
            "node.md is missing and this is not a verified deleted-node residue"
        }
        _ if text.starts_with("not a real directory: ") => {
            "expected a real directory, not a file or symlink"
        }
        _ => "",
    };
    if !reason.is_empty() {
        return (
            "unsafe_filesystem",
            reason,
            "inspect this path and repair its layout without deleting knowledge; retry the check",
        );
    }
    (
        "unknown",
        "source capture failed",
        "retain existing files and inspect this path before retrying",
    )
}

// Native workers cannot send Box<dyn Error>; preserve safe context and IO identity.
pub(crate) enum WorkerError {
    Io(std::io::Error),
    Source(SourceError),
    Other(String),
}
impl WorkerError {
    pub fn capture(error: Box<dyn Error>) -> Self {
        if let Some(issue) = error.downcast_ref::<SourceError>() {
            return Self::Source(issue.clone());
        }
        match error.downcast::<std::io::Error>() {
            Ok(error) => Self::Io(*error),
            Err(error) => Self::Other(error.to_string()),
        }
    }
    pub fn restore(self) -> Box<dyn Error> {
        match self {
            Self::Io(error) => error.into(),
            Self::Source(error) => error.into(),
            Self::Other(error) => error.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn safe_context_survives_workers_without_error_payloads() {
        for (error, kind) in [
            (
                Box::new(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
                    as Box<dyn Error>,
                "permission_denied",
            ),
            (
                Box::new(std::io::Error::from(std::io::ErrorKind::StorageFull)),
                "resource_exhausted",
            ),
            ("PRIVATE_UNTRUSTED_ERROR_PAYLOAD".into(), "unknown"),
            (
                Box::new(serde_json::from_str::<String>("123").unwrap_err()),
                "invalid_capture",
            ),
        ] {
            let error = context::<()>(Err(error), Path::new("nodes/example/node.md"), Some("name"))
                .unwrap_err();
            let error = WorkerError::capture(error).restore();
            let issue = error.downcast_ref::<SourceError>().unwrap();
            assert_eq!(issue.kind, kind);
            assert_eq!(issue.path, "nodes/example/node.md");
            assert_eq!(issue.field.as_deref(), Some("name"));
            assert!(
                !error
                    .to_string()
                    .contains("PRIVATE_UNTRUSTED_ERROR_PAYLOAD")
            );
            assert!(!serde_json::to_string(issue).unwrap().contains("123"));
        }
    }
}
