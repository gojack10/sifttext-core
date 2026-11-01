//! Read-only deterministic publication check for cooperative local dogfood.
//! This is not a Git publisher, semantic judge, or unmanaged-writer boundary.
use crate::{
    Entry, Policy, Result, Store,
    coherent_capture::{Capture, authored_path},
    require,
    status::{self, State, Status},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

pub const FIELD_RULE: &str = "52da9df4-4651-4a01-93ae-2ae5ba038a90";
pub const STATUS_RULE: &str = "ca9d414a-a0be-4024-8aad-da1af63cbf77";
pub const CEILING_RULE: &str = "302b4549-ef27-4ab7-a9a7-7f240ffe058e";
pub const CANDIDATE_RULE: &str = "e8234ea5-3467-4f2b-8ca2-2bcc5cc80d11";
pub const COMMIT_RULE: &str = "c72d552d-3499-445b-a8d5-05d0ff7824f2";
pub const DECOMPOSITION_RULE: &str = "5a1e32e2-ed01-4a72-bd17-eedce98b961f";
pub const CEILING: usize = 5000;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub store: PathBuf,
    #[serde(default)]
    pub before: Option<PathBuf>,
    #[serde(default)]
    pub explicit: BTreeSet<String>,
    #[serde(default)]
    pub expected_version: Option<String>,
    #[serde(default)]
    pub pause_before_accept: Option<PauseBeforeAccept>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PauseBeforeAccept {
    pub ready: PathBuf,
    pub release: PathBuf,
}
impl PauseBeforeAccept {
    fn wait(&self, stores: &[&Path]) -> Result<()> {
        require(self.ready != self.release, "barrier paths must differ")?;
        for p in [&self.ready, &self.release] {
            let parent = fs::canonicalize(p.parent().ok_or("missing barrier parent")?)?;
            require(
                stores.iter().all(|s| !parent.starts_with(s)),
                "barrier outside stores",
            )?;
        }
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.ready)?
            .write_all(b"computed\n")?;
        let start = Instant::now();
        while !self.release.try_exists()? {
            require(start.elapsed() < Duration::from_secs(10), "barrier expired")?;
            thread::sleep(Duration::from_millis(5));
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
pub struct Row {
    pub id: Option<String>,
    pub check: &'static str,
    pub rule: &'static str,
    pub tokens: Option<usize>,
    /// Canonical identities only; null means the source could not establish it.
    pub source: Option<String>,
    pub target: Option<String>,
    /// Exact checked predicate, or native encoding guard before Bend can run.
    pub law: Option<&'static str>,
}

#[derive(Debug)]
pub(crate) struct StructuralIssue {
    source: Option<String>,
    target: Option<String>,
    law: &'static str,
    cause: Box<dyn std::error::Error>,
}
impl std::fmt::Display for StructuralIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.cause.fmt(f)
    }
}
impl std::error::Error for StructuralIssue {}
impl StructuralIssue {
    fn problem(&self) -> crate::source_error::SourceError {
        let mut path = self
            .source
            .as_ref()
            .map(|id| format!("nodes/{id}"))
            .unwrap_or_else(|| "trees".into());
        let (field, reason) = match self.law {
            "src/lib.rs:Store.read_view/node_uuid_identity" => {
                path.push_str("/node.md");
                (
                    Some("id".into()),
                    "metadata id must match the node directory UUID",
                )
            }
            "src/lib.rs:Store.read_view/order_uuid"
            | "src/lib.rs:Store.read_view/order_membership" => {
                path.push_str("/children.order");
                (
                    None,
                    "children.order must list each child UUID exactly once and match the child links",
                )
            }
            _ => {
                if self.source.is_some() {
                    path.push_str("/children");
                }
                if let Some(target) = &self.target {
                    path.push('/');
                    path.push_str(target);
                }
                (
                    None,
                    "relationship must name an existing canonical node using the expected relative symlink",
                )
            }
        };
        crate::source_error::SourceError {
            kind: "invalid_candidate",
            path,
            field,
            reason,
            next_action: "repair this relationship or metadata in the retained files, then retry",
        }
    }
}

pub(crate) fn structural_guard(
    ok: bool,
    source: Option<&str>,
    target: Option<&str>,
    law: &'static str,
    message: &'static str,
) -> Result<()> {
    structural_context(require(ok, message), source, target, law)
}

pub(crate) fn structural_context(
    result: Result<()>,
    source: Option<&str>,
    target: Option<&str>,
    law: &'static str,
) -> Result<()> {
    result.map_err(|cause| {
        Box::new(StructuralIssue {
            source: source.filter(|id| crate::uuid(id)).map(str::to_owned),
            target: target.filter(|id| crate::uuid(id)).map(str::to_owned),
            law,
            cause,
        }) as Box<dyn std::error::Error>
    })
}

/// Observation only, never a deserializable proof or acceptance capability.
#[derive(Clone, Debug, Serialize)]
pub struct GraphCheck {
    pub mode: &'static str,
    pub reason: &'static str,
    pub base_version: Option<String>,
    pub candidate_version: Option<String>,
}

/// Bounded same-invocation provenance, not an error-text dump or a verdict.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
pub struct Failure {
    pub stage: &'static str,
    pub kind: &'static str,
}

fn failure_kind(stage: &str, error: &(dyn std::error::Error + 'static)) -> &'static str {
    if error.is::<StructuralIssue>() {
        return "invalid_candidate";
    }
    if let Some(issue) = error.downcast_ref::<crate::source_error::SourceError>() {
        return issue.kind;
    }
    if let Some(error) = error.downcast_ref::<std::io::Error>() {
        return match error.kind() {
            std::io::ErrorKind::NotFound => "not_found",
            std::io::ErrorKind::PermissionDenied => "permission_denied",
            std::io::ErrorKind::StorageFull
            | std::io::ErrorKind::FileTooLarge
            | std::io::ErrorKind::OutOfMemory => "resource_exhausted",
            std::io::ErrorKind::TimedOut => "timeout",
            _ => "io",
        };
    }
    if stage == "policy_identity" {
        let text = error.to_string();
        if text.starts_with(crate::CANDIDATE_CHANGED_PREFIX) {
            return "identity_changed";
        }
        return match text.as_str() {
            "candidate file set changed; rebuild and recheck"
            | "validated capture policy changed" => "identity_changed",
            "empty build stamp"
            | "incorrect executable stamp"
            | "malformed build stamp"
            | "unsafe build stamp path"
            | "duplicate build stamp entry" => "invalid_configuration",
            _ => "unknown",
        };
    }
    if matches!(
        stage,
        "candidate_capture" | "before_capture" | "source_recheck"
    ) {
        if error.is::<serde_json::Error>() || error.is::<std::str::Utf8Error>() {
            return "invalid_capture";
        }
        // Only fixed native guards. Never expose or interpret arbitrary prose.
        return match error.to_string().as_str() {
            "missing Markdown metadata"
            | "missing Markdown opening fence"
            | "missing Markdown closing fence"
            | "missing required metadata"
            | "duplicate metadata key"
            | "blank node name/micro-summary"
            | "blank relationship reason" => "invalid_capture",
            "changed source identity"
            | "changed base identity"
            | "changed authored bytes"
            | "changed store bytes"
            | "changed base bytes"
            | "changed residue identity"
            | "changed historical Git source" => "source_changed",
            _ => "unknown",
        };
    }
    if stage == "accept_barrier" && error.to_string() == "barrier expired" {
        return "timeout";
    }
    if matches!(
        stage,
        "graph_policy" | "before_graph_policy" | "graph_transition"
    ) {
        if error.is::<std::str::Utf8Error>() {
            return "invalid_helper_response";
        }
        if let Some(error) = error.downcast_ref::<std::sync::mpsc::RecvTimeoutError>() {
            return match error {
                std::sync::mpsc::RecvTimeoutError::Timeout => "helper_pipe_timeout",
                std::sync::mpsc::RecvTimeoutError::Disconnected => "unknown",
            };
        }
        // These exact identities originate in the checked native helper path,
        // not its stderr/output. A malformed helper cannot supply a category.
        let text = error.to_string();
        if text.starts_with(crate::CANDIDATE_CHANGED_PREFIX) {
            return "identity_changed";
        }
        return match text.as_str() {
            "checked graph rejected or malformed verdict" => "invalid_candidate",
            "policy process timed out" => "timeout",
            "policy process exited nonzero" => "helper_nonzero",
            "malformed checked graph verdict"
            | "malformed checked transition verdict"
            | "unexpected policy stderr" => "invalid_helper_response",
            "policy process output exceeds limit"
            | "wire exceeds slice limit"
            | "transition wire exceeds slice limit"
            | "wire exceeds frozen backend input limit" => "resource_exhausted",
            "candidate file set changed; rebuild and recheck"
            | "transition policy identity changed"
            | "transition executable changed" => "identity_changed",
            _ => "unknown",
        };
    }
    "unknown"
}

#[cfg(test)]
mod framing_failure_tests {
    use super::*;

    #[test]
    fn frozen_input_refusal_is_resource_exhaustion_not_unknown_or_timeout() {
        let input = vec![b' '; 1_048_577];
        // A missing executable proves refusal precedes process creation.
        let error = crate::invoke(Path::new("not-an-executable"), &input).unwrap_err();
        assert_eq!(error.to_string(), "wire exceeds frozen backend input limit");
        for stage in ["graph_policy", "before_graph_policy", "graph_transition"] {
            assert_eq!(failure_kind(stage, error.as_ref()), "resource_exhausted");
        }
        assert_eq!(failure_kind("candidate_capture", error.as_ref()), "unknown");
    }
}

#[derive(Debug, Serialize)]
pub struct Report {
    pub status: &'static str,
    pub version: Option<String>,
    pub before_version: Option<String>,
    pub nodes: usize,
    pub rows: Vec<Row>,
    pub graph: GraphCheck,
    /// Absent on success so successful observations retain their exact shape.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<Failure>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<crate::source_error::SourceError>,
}
impl Report {
    fn observe<T>(&mut self, stage: &'static str, result: Result<T>) -> Result<T> {
        result.inspect_err(|error| {
            self.problem = error
                .downcast_ref::<crate::source_error::SourceError>()
                .cloned();
            self.failure = Some(Failure {
                stage,
                kind: failure_kind(stage, error.as_ref()),
            });
        })
    }
    fn reject(
        &mut self,
        id: Option<&str>,
        check: &'static str,
        rule: &'static str,
        tokens: Option<usize>,
    ) {
        self.status = "rejected";
        self.rows.push(Row {
            id: id.map(str::to_owned),
            check,
            rule,
            tokens,
            source: None,
            target: None,
            law: None,
        });
    }
    fn structural(&mut self, source: Option<&str>, target: Option<&str>, law: &'static str) {
        self.reject(source, "structural_invalid", CANDIDATE_RULE, None);
        let row = self.rows.last_mut().unwrap();
        row.source = source.filter(|id| crate::uuid(id)).map(str::to_owned);
        row.target = target.filter(|id| crate::uuid(id)).map(str::to_owned);
        row.law = Some(law);
    }
    /// The public CLI reports actionable problems; observer serialization retains diagnostics.
    pub fn cli_lean(&self) -> Result<String> {
        if self.status == "current" {
            return Ok(crate::lean_render::render(&json!({
                "nodes": self.nodes, "status": self.status, "version": self.version,
            }))
            .map_err(std::io::Error::other)?);
        }
        let mut value = serde_json::to_value(self)?;
        customer_report(&mut value);
        Ok(crate::lean_render::render(&value).map_err(std::io::Error::other)?)
    }
    pub fn lean(&self) -> Result<String> {
        let mut value = json!({
            "status": self.status, "version": self.version, "before_version": self.before_version,
            "nodes": self.nodes, "ceiling": CEILING, "encoding": "o200k_base",
            "tokenizer_version": "tiktoken-rs/0.6.0;ordinary;content/v1",
            "commit_rule": COMMIT_RULE, "decomposition_rule": DECOMPOSITION_RULE,
            "rows": self.rows,
        });
        if let Some(failure) = self.failure {
            value["failure_stage"] = json!(failure.stage);
            value["failure_kind"] = json!(failure.kind);
        }
        Ok(crate::lean_render::render(&value).map_err(std::io::Error::other)?)
    }
}

/// One customer projection for direct checks and nested publication receipts.
pub(crate) fn customer_report(value: &mut serde_json::Value) {
    let Some(object) = value.as_object_mut() else {
        return;
    };
    object.remove("graph");
    if object.get("status").is_some_and(|s| s == "current") {
        return;
    }
    let failure = object.remove("failure").filter(|v| !v.is_null());
    if let Some(failure) = &failure {
        object.insert("failure_kind".into(), failure["kind"].clone());
        if !(failure["stage"] == "candidate" && failure["kind"] == "invalid_candidate") {
            object.insert("failure_stage".into(), failure["stage"].clone());
        }
    }
    let problem = object.remove("problem").filter(|v| !v.is_null());
    if let Some(problem) = problem.as_ref().and_then(|p| p.as_object()) {
        for (key, val) in problem {
            if key != "kind" {
                object.insert(key.clone(), val.clone());
            }
        }
    } else {
        let kind = object
            .get("failure_kind")
            .and_then(|v| v.as_str())
            .unwrap_or("invalid_candidate");
        let (reason, action) = match kind {
            "not_found" => (
                "required path is missing",
                "check the supplied knowledge home and restore missing files; do not reinitialize existing knowledge",
            ),
            "permission_denied" => (
                "permission denied",
                "check permissions and retry; retain existing files",
            ),
            "invalid_configuration" => (
                "checked application configuration is invalid",
                "rebuild or reinstall the checked application; retain existing knowledge",
            ),
            "resource_exhausted" => (
                "validation resource limit reached",
                "free resources or use a smaller operation, then retry; retain existing files",
            ),
            "identity_changed" => (
                "checked software or configuration changed",
                "rebuild or reinstall the checked application, then retry; retain knowledge",
            ),
            "source_changed" => (
                "source changed during validation",
                "retry against a stable source; retain the draft",
            ),
            "invalid_candidate" => (
                "knowledge validation rejected",
                "inspect the reported node IDs and checks, repair the retained files, then retry",
            ),
            _ => (
                "validation could not complete",
                "retain existing files and inspect machine-readable diagnostics before retrying",
            ),
        };
        object.insert("reason".into(), json!(reason));
        object.insert("next_action".into(), json!(action));
    }
    object.remove("nodes");
    let rows = object.remove("rows").unwrap_or_else(|| json!([]));
    let rows = rows.as_array().map(Vec::as_slice).unwrap_or_default();
    if object.get("status").is_none_or(|s| s != "stale")
        && !failure
            .as_ref()
            .is_some_and(|f| f["stage"].as_str().is_some_and(|s| s.starts_with("before")))
        && !rows.iter().any(|row| {
            matches!(
                row["check"].as_str(),
                Some(
                    "status_transition_or_reason"
                        | "active_child_blocks_resolution"
                        | "parent_requires_reopening"
                )
            )
        })
    {
        object.remove("before_version");
    }
    object.retain(|_, value| !value.is_null());
    let mut seen = BTreeSet::new();
    let rows = rows
        .iter()
        .filter_map(|row| {
            if row["check"] == "content_tokens"
                || (row["check"] == "validation_unavailable" && failure.is_some())
                || (row["check"] == "structural_invalid" && problem.is_some())
                || (row["id"].is_null()
                    && problem.as_ref().is_some_and(|p| p["kind"] == row["check"]))
            {
                return None;
            }
            let mut row = row.clone();
            let fields = row.as_object_mut()?;
            fields.remove("rule");
            fields.remove("law");
            fields.retain(|_, value| !value.is_null());
            if fields.get("id") == fields.get("source") {
                fields.remove("source");
            }
            if fields
                .get("check")
                .is_some_and(|v| v == "content_over_limit")
            {
                fields.insert("limit".into(), json!(CEILING));
            }
            // Transition and closure can repeat this exact predicate. Other
            // checks may be independent even when their rows look identical.
            if fields
                .get("check")
                .is_some_and(|v| v == "status_transition_or_reason")
                && !seen.insert(row.to_string())
            {
                None
            } else {
                Some(row)
            }
        })
        .collect::<Vec<_>>();
    if !rows.is_empty() {
        object.insert("rows".into(), json!(rows));
    }
}

/// Same length-delimited logical inventory/version boundary as the native index.
/// Derived caches, metadata sidecars, outlines and unrelated repository files are excluded.
pub fn version(store: &Store) -> Result<String> {
    use crate::source_version::{self, Part};
    let mut digest = source_version::start();
    for (path, entry) in store.inventory.iter().filter(|(p, _)| authored_path(p)) {
        match entry {
            Entry::Dir => source_version::add(&mut digest, path, Part::Directory)?,
            Entry::File(_) | Entry::FileRef { .. } => {
                let (len, hash) = entry.file_identity().ok_or("missing file identity")?;
                source_version::add(&mut digest, path, Part::File { len, hash: &hash })?;
            }
            Entry::Link(target) => {
                let bytes = target.to_str().ok_or("non-UTF-8 link target")?.as_bytes();
                let hash = Sha256::digest(bytes).into();
                source_version::add(&mut digest, path,
                    Part::Link { len: bytes.len() as u64, hash: &hash })?;
            }
        }
    }
    Ok(format!("{:x}", digest.finalize()))
}
pub(crate) fn intrinsic_fields(fields: &BTreeMap<&str, &str>) -> u8 {
    u8::from(fields.contains_key("ruled_out"))
        | (u8::from(fields.contains_key("content")) << 1)
        | (u8::from(["scope", "warnings"].iter().any(|key|
            fields.get(key).is_some_and(|value| serde_json::from_str::<String>(value).is_err()))) << 2)
        | (u8::from(["name", "micro_summary"].iter().any(|key|
            fields.get(key).is_none_or(|value| serde_json::from_str::<String>(value)
                .map_or(true, |text| text.trim().is_empty())))) << 3)
}

fn field_rows(report: &mut Report, id: &str, errors: u8) {
    for (bit, check) in [(1, "ruled_out_removed"), (2, "body_metadata_duplicate"), (4, "authored_field_type"), (8, "authored_field_blank")] {
        if errors & bit != 0 { report.reject(Some(id), check, FIELD_RULE, None); }
    }
}

fn intrinsic_node_rows(report: &mut Report, id: &str, field_errors: u8,
    references: impl Iterator<Item = impl AsRef<str>>, state: Option<&State>, body_nonblank: bool, tokens: usize) {
    field_rows(report, id, field_errors);
    if tokens > CEILING {
        report.reject(Some(id), "content_over_limit", CEILING_RULE, Some(tokens));
    }
    let mut targets = BTreeSet::new();
    if references.map(|target| target.as_ref().to_owned()).any(|target| target == id || !targets.insert(target)) {
        report.reject(Some(id), "self_or_duplicate_reference", FIELD_RULE, None);
    }
    if state.is_some_and(|state| state.status == Status::Resolved) && !body_nonblank {
        report.reject(Some(id), "resolved_body_blank", STATUS_RULE, None);
    }
}

fn state(capture: &Capture, id: &str) -> Result<State> {
    let fields = &capture.metadata[id];
    let status = serde_json::from_str(fields.get("status").ok_or("missing status")?)?;
    let mut reasons = BTreeMap::new();
    for key in ["blocker", "deferred_reason", "discard_reason"] {
        if let Some(v) = fields.get(key) {
            reasons.insert(key.to_owned(), serde_json::from_str(v)?);
        }
    }
    Ok(State { status, reasons })
}

/// Opaque, invocation-local successful validation. Neither public Reports nor
/// caller-supplied captures/versions can manufacture this capability.
pub(crate) struct ValidatedCapture {
    capture: Capture,
}
impl ValidatedCapture {
    /// Consume checked immutable bytes for copying, not a live-source proof.
    /// The recipient must validate the materialized destination independently.
    pub(crate) fn into_snapshot(self) -> Result<Store> {
        self.capture.unchanged_all()?;
        let full = Store::read(&self.capture.store.path)?;
        require(full.inventory == self.capture.store.inventory, "snapshot source changed")?;
        self.capture.unchanged_all()?;
        Ok(full)
    }
}

/// Recheck the complete candidate, optionally against an immutable before view
/// and a prior logical version. Never normalize/mutate authored bytes or caches.
pub fn validate(policy: &Policy, request: &Request) -> Report {
    validate_with_capture(policy, request).0
}

/// Retain only the exact capture that passed every ordinary candidate check and
/// final barrier. A rejected or stale observation never carries authority.
pub(crate) fn validate_with_capture(
    policy: &Policy,
    request: &Request,
) -> (Report, Option<ValidatedCapture>) {
    let mut report = Report {
        status: "current",
        version: None,
        before_version: None,
        nodes: 0,
        rows: vec![],
        failure: None,
        problem: None,
        graph: GraphCheck {
            mode: "full_law",
            reason: "complete structural law (same-input cache eligible)",
            base_version: None,
            candidate_version: None,
        },
    };
    let capture = match check(policy, request, &mut report) {
        Ok(capture) => capture,
        Err(error) => {
            if report.failure.is_none() {
                report.failure = Some(Failure {
                    stage: "candidate",
                    kind: "unknown",
                });
            }
            if let Some(issue) = error.downcast_ref::<StructuralIssue>() {
                report.structural(issue.source.as_deref(), issue.target.as_deref(), issue.law);
                report.problem = Some(issue.problem());
                return (report, None);
            }
            // An unlocated failure (including helper timeout) does not establish
            // that any individual node is invalid. Keep one candidate-wide row.
            report.problem = error
                .downcast_ref::<crate::source_error::SourceError>()
                .cloned();
            let check = report
                .problem
                .as_ref()
                .map_or("validation_unavailable", |p| p.kind);
            report.reject(None, check, CANDIDATE_RULE, None);
            None
        }
    };
    if report.status != "current" && report.failure.is_none() {
        report.failure = Some(Failure {
            stage: "candidate",
            kind: "invalid_candidate",
        });
    }
    (report, capture)
}
// Explanations of an already rejected graph, never an acceptance path. Keep the
// full checked Spec.valid call above these diagnostics; unknown failures stay generic.
fn graph_diagnostics<'a>(
    nodes: impl Iterator<Item = (&'a str, Vec<&'a str>)>,
    children: &BTreeMap<String, Vec<String>>, roots: &[String], report: &mut Report,
) {
    let nodes: BTreeMap<_, _> = nodes.collect();
    let mut parents: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (source, children) in children {
        for target in children {
            parents.entry(target).or_default().push(source);
        }
    }
    for (&source, references) in &nodes {
        for &target in references {
            if !nodes.contains_key(target) {
                report.structural(
                    Some(source),
                    Some(target),
                    "laws/Spec.bend:refs_valid",
                );
                report.rows.last_mut().unwrap().check = "dangling_reference";
            }
        }
        let ps = parents
            .get(source)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let placements = ps.len() + roots.iter().filter(|r| r.as_str() == source).count();
        if placements != 1 {
            if ps.is_empty() {
                report.structural(Some(source), None, "laws/Spec.bend:nodes_valid");
                report.rows.last_mut().unwrap().check = "placement_count";
            }
            for parent in ps {
                report.structural(Some(parent), Some(source), "laws/Spec.bend:nodes_valid");
                report.rows.last_mut().unwrap().check = "placement_count";
            }
        }
    }
    // Spec.parent chooses the first matching node in the UUID-sorted wire. Walk
    // its reverse edges from explicit roots to explain Spec.rooted in linear space.
    let mut selected_children: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (child, ps) in &parents {
        if let Some(parent) = ps.first() {
            selected_children.entry(parent).or_default().push(child);
        }
    }
    let mut rooted = BTreeSet::new();
    let mut pending: Vec<_> = roots.iter().map(String::as_str).collect();
    while let Some(id) = pending.pop() {
        if rooted.insert(id) {
            pending.extend(selected_children.get(id).into_iter().flatten().copied());
        }
    }
    for &id in nodes.keys() {
        if !rooted.contains(id) {
            report.structural(
                Some(id),
                parents.get(id).and_then(|p| p.first()).copied(),
                "laws/Spec.bend:rooted",
            );
            report.rows.last_mut().unwrap().check = "not_rooted";
        }
    }
}

fn check(
    policy: &Policy,
    request: &Request,
    report: &mut Report,
) -> Result<Option<ValidatedCapture>> {
    // Pin before the native law, but do not preempt its existing
    // failure stage or candidate capture/version observations with a new guard.
    // An initial identity-read error can never mint a successful capability.
    let policy_identity = policy.validation_identity();
    let candidate = report.observe(
        "candidate_capture",
        Capture::read_for_validation(&request.store),
    )?;
    let store = &candidate.store;
    report.nodes = store.nodes.len();
    report.version = Some(report.observe("candidate_version", version(store))?);
    report.graph.candidate_version = report.version.clone();
    // Compact captures always use the full law on its proved normalized input
    // (or the same-input shape cache), before retaining the baseline capture.
    // Structural errors carry only UUIDs/rule locators, never raw parser paths or prose.
    if !report.observe("graph_policy", policy.graph_valid(store))? {
        graph_diagnostics(store.nodes.iter().map(|(id, node)|
            (id.as_str(), node.see_also.iter().map(|r| r.target.as_str()).collect())),
            &store.children, &store.roots, report);
        if report.rows.is_empty() {
            report.reject(None, "graph_invalid", CANDIDATE_RULE, None);
        }
        return Ok(None);
    }
    let mut before = report.observe(
        "before_capture",
        request
            .before
            .as_deref()
            .map(Capture::read_for_validation)
            .transpose(),
    )?;
    report.graph.base_version = report.observe("before_version",
        before.as_ref().map(|b| version(&b.store)).transpose())?;
    if request
        .expected_version
        .as_ref()
        .is_some_and(|v| Some(v) != report.version.as_ref())
    {
        report.reject(None, "receipt_version_changed", CANDIDATE_RULE, None);
    }
    if before.is_some() {
        // The complete candidate law establishes its
        // graph. The baseline supplies status/version history, not another gate.
        report.before_version = report.graph.base_version.clone();
    }
    require(
        request
            .explicit
            .iter()
            .all(|id| store.nodes.contains_key(id))
            && (request.before.is_some() || request.explicit.is_empty()),
        "invalid explicit requests",
    )?;
    let mut states = BTreeMap::new();
    let mut transitions = BTreeMap::new();
    for (id, node) in &store.nodes {
        // Run after graph-policy exits: tokenizer and Bend working sets do not overlap.
        let path = std::path::PathBuf::from(format!("nodes/{id}/node.md"));
        let bytes = crate::source_error::context(
            store.inventory.get(&path).ok_or("missing captured source")?.bytes(), &path, None)?;
        let parsed = crate::source_error::context(
            (|| crate::markdown(std::str::from_utf8(&bytes)?))(), &path, None)?;
        let tokens = crate::token_count::ordinary(&parsed.authored.content);
        let fields = &candidate.metadata[id];
        let requested = state(&candidate, id);
        intrinsic_node_rows(report, id, intrinsic_fields(&fields.iter()
            .map(|(key, value)| (key.as_str(), value.as_str())).collect()),
            node.see_also.iter().map(|reference| &reference.target), requested.as_ref().ok(),
            !parsed.authored.content.trim().is_empty(), tokens);
        let requested = match requested {
            Ok(s) => s,
            Err(_) => {
                report.reject(Some(id), "status_or_reason_type", STATUS_RULE, None);
                continue;
            }
        };
        let old = before
            .as_ref()
            .filter(|b| b.store.nodes.contains_key(id))
            .map(|b| state(b, id))
            .transpose()?;
        let changed = if let Some(b) = before.as_ref().filter(|b| b.store.nodes.contains_key(id)) {
            let prior = b.store.inventory.get(&path).ok_or("missing before source")?;
            let text_changed = if Some(prior) == store.inventory.get(&path) { false } else {
                let prior = prior.bytes()?;
                crate::ordering::without_position(std::str::from_utf8(&prior)?)
                    != crate::ordering::without_position(std::str::from_utf8(&bytes)?)
            };
            text_changed || b.store.children.get(id) != store.children.get(id)
        } else { false };
        let transitioned = match status::transition(
            old.as_ref(),
            &requested,
            changed,
            request.explicit.contains(id),
        ) {
            Ok(transitioned) => transitioned,
            Err(_)
                if !request.explicit.contains(id)
                    && old
                        .as_ref()
                        .is_some_and(|old| old.status == Status::Resolved)
                    && requested.status == Status::InProgress
                    && store.children[id]
                        .iter()
                        .any(|child| state(&candidate, child).is_ok_and(|s| s.status.blocks())) =>
            {
                let mut prior = requested.clone();
                prior.status = Status::Resolved;
                prior
            }
            Err(_) => {
                report.reject(Some(id), "status_transition_or_reason", STATUS_RULE, None);
                requested.clone()
            }
        };
        transitions.insert(id.clone(), transitioned);
        states.insert(id.clone(), requested);
    }
    if transitions.len() == store.nodes.len() {
        if status::child_gate(&mut transitions, &store.children, &request.explicit).is_err() {
            for (id, state) in &states {
                if state.status == Status::Resolved
                    && store.children[id].iter().any(|c| states[c].status.blocks())
                {
                    report.reject(
                        Some(id),
                        "active_child_blocks_resolution",
                        STATUS_RULE,
                        None,
                    );
                }
            }
        } else {
            for (id, requested) in &states {
                if transitions[id] != *requested {
                    let check = if transitions[id].status == Status::InProgress
                        && requested.status == Status::Resolved
                    {
                        "parent_requires_reopening"
                    } else {
                        "status_transition_or_reason"
                    };
                    report.reject(Some(id), check, STATUS_RULE, None);
                }
            }
        }
    }
    drop(states);
    drop(transitions);
    // Retain only the baseline's compact inventory/identities for later barriers.
    if let Some(before) = &mut before {
        before.metadata.clear();
        before.store.nodes.clear();
        before.store.children.clear();
        before.store.roots.clear();
        before.store.compact_bodies = None;
    }
    if let Some(pause) = &request.pause_before_accept {
        let mut paths = vec![store.path.as_path()];
        if let Some(b) = &before {
            paths.push(&b.store.path);
        }
        report.observe("accept_barrier", pause.wait(&paths))?;
    }
    report.observe("policy_identity", policy.verify_stamp())?;
    let policy_identity = report.observe("policy_identity", policy_identity)?;
    report.observe(
        "policy_identity",
        policy.validation_identity().and_then(|identity| {
            require(
                identity == policy_identity,
                "validated capture policy changed",
            )
        }),
    )?;
    let unchanged = candidate
        .unchanged_all()
        .and_then(|()| before.as_ref().map_or(Ok(()), |b| b.unchanged_all()));
    if let Err(error) = unchanged {
        report.reject(None, "source_changed_before_accept", CANDIDATE_RULE, None);
        report.status = "stale";
        report.failure = Some(Failure {
            stage: "source_recheck",
            kind: failure_kind("source_recheck", error.as_ref()),
        });
    }
    Ok((report.status == "current").then_some(ValidatedCapture {
        capture: candidate,
    }))
}

/// Exact immutable Git inputs: no mutable filesystem capture or source sweep.
pub(crate) fn validate_git(
    policy: &Policy, reader: &mut crate::git_snapshot::GitReader,
    before: &crate::git_snapshot::GitView, candidate: &crate::git_snapshot::GitView,
    explicit: &BTreeSet<String>,
) -> Report {
    let mut report = Report {
        status: "current", version: Some(candidate.version.clone()),
        before_version: Some(before.version.clone()), nodes: candidate.nodes.len(),
        rows: vec![], failure: None, problem: None,
        graph: GraphCheck { mode: "full_law",
            reason: "complete structural law on immutable Git shape (same-input cache eligible)",
            base_version: Some(before.version.clone()), candidate_version: Some(candidate.version.clone()) },
    };
    if let Err(error) = check_git(policy, reader, before, candidate, explicit, &mut report) {
        if let Some(issue) = error.downcast_ref::<StructuralIssue>() {
            report.structural(issue.source.as_deref(), issue.target.as_deref(), issue.law);
            report.problem = Some(issue.problem());
        } else {
            report.problem = error.downcast_ref::<crate::source_error::SourceError>().cloned();
            let check = report.problem.as_ref().map_or("validation_unavailable", |p| p.kind);
            report.reject(None, check, CANDIDATE_RULE, None);
        }
        if report.failure.is_none() {
            report.failure = Some(Failure { stage: "candidate", kind: failure_kind("candidate", error.as_ref()) });
        }
    }
    if report.status != "current" && report.failure.is_none() {
        report.failure = Some(Failure { stage: "candidate", kind: "invalid_candidate" });
    }
    report
}

fn check_git(
    policy: &Policy, reader: &mut crate::git_snapshot::GitReader,
    before: &crate::git_snapshot::GitView, candidate: &crate::git_snapshot::GitView,
    explicit: &BTreeSet<String>, report: &mut Report,
) -> Result<()> {
    report.observe("policy_identity", policy.verify_stamp())?;
    let identity = report.observe("policy_identity", policy.validation_identity())?;
    report.observe("policy_identity", require(reader.policy_identity == identity, "validated capture policy changed"))?;
    require(explicit.iter().all(|id| crate::uuid(id) && candidate.nodes.contains_key(id)), "invalid explicit requests")?;
    reader.suspend_objects();
    // Drop the graph before the first cold BPE allocation. This is the actual
    // existing checked Bend validator, not a Rust structural acceptance test.
    let valid = {
        let graph = candidate.graph();
        report.observe("graph_policy", policy.graph_valid_input(&graph, &reader.repo))?
    };
    if !valid {
        graph_diagnostics(candidate.nodes.iter().map(|(id, facts)|
            (id.as_str(), facts.reference_targets.iter().map(String::as_str).collect())),
            &candidate.children, &candidate.roots, report);
        if report.rows.is_empty() { report.reject(None, "graph_invalid", CANDIDATE_RULE, None); }
        return Ok(());
    }
    for (id, facts) in &candidate.nodes {
        let tokens = report.observe("candidate_tokens", reader.tokens(candidate, id))?;
        intrinsic_node_rows(report, id, facts.field_errors, facts.reference_targets.iter(),
            Some(&facts.state), facts.body_nonblank, tokens);
    }
    reader.suspend_objects();
    crate::token_count::release();
    crate::release_unused_memory();
    let old: BTreeMap<_, _> = before.nodes.iter().map(|(id, node)| (id.clone(), node.state.clone())).collect();
    let requested: BTreeMap<_, _> = candidate.nodes.iter().map(|(id, node)| (id.clone(), node.state.clone())).collect();
    let edited: BTreeSet<_> = candidate.nodes.iter().filter(|(id, node)|
        before.nodes.get(*id).is_some_and(|prior| prior.text_hash != node.text_hash))
        .map(|(id, _)| id.clone()).collect();
    let normalized = status::normalized_states(&old, &requested, &before.children,
        &candidate.children, &edited, explicit);
    match normalized {
        Ok(expected) => {
            for (id, state) in &requested {
                if expected.get(id) != Some(state) {
                    let check = if state.status == Status::Resolved
                        && expected.get(id).is_some_and(|state| state.status == Status::InProgress) {
                        "parent_requires_reopening"
                    } else { "status_transition_or_reason" };
                    report.reject(Some(id), check, STATUS_RULE, None);
                }
            }
        }
        Err(_) => {
            let start = report.rows.len();
            for (id, state) in &requested {
                let changed = edited.contains(id) || before.children.get(id) != candidate.children.get(id);
                if status::transition(old.get(id), state, changed, explicit.contains(id)).is_err() {
                    report.reject(Some(id), "status_transition_or_reason", STATUS_RULE, None);
                }
                if state.status == Status::Resolved && candidate.children[id].iter()
                    .any(|child| requested[child].status.blocks()) {
                    report.reject(Some(id), "active_child_blocks_resolution", STATUS_RULE, None);
                }
            }
            if report.rows.len() == start { report.reject(None, "status_transition_or_reason", STATUS_RULE, None); }
        }
    }
    report.observe("policy_identity", policy.verify_stamp())?;
    let final_identity = report.observe("policy_identity", policy.validation_identity())?;
    report.observe("policy_identity", require(final_identity == identity, "validated capture policy changed"))?;
    // Caller still binds the exact tree and performs the conditional Git advance.
    // An immutable content identity is never permission to skip filesystem safety
    // at that caller's checkout/cleanup boundary.
    Ok(())
}
