//! Read-only deterministic publication check for cooperative local dogfood.
//! This is not a Git publisher, semantic judge, or unmanaged-writer boundary.
use crate::{
    Entry, Policy, Result, Store,
    coherent_capture::{Capture, logical},
    graph_transition::{Preservation, ProvenBase, Transition},
    require, sha256,
    status::{self, State, Status},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
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
}
impl Report {
    fn observe<T>(&mut self, stage: &'static str, result: Result<T>) -> Result<T> {
        result.inspect_err(|error| {
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
    pub fn lean(&self) -> Result<String> {
        let mut value = json!({
            "status": self.status, "version": self.version, "before_version": self.before_version,
            "nodes": self.nodes, "ceiling": CEILING, "encoding": "o200k_base",
            "tokenizer_version": "tiktoken-rs/0.6.0;ordinary;crystallization/v1",
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

/// Same length-delimited logical inventory/version boundary as the native index.
/// Derived caches, metadata sidecars, outlines and unrelated repository files are excluded.
pub fn version(store: &Store) -> String {
    let mut bytes = Vec::new();
    for (path, entry) in logical(store) {
        let p = path.to_string_lossy();
        bytes.extend_from_slice(&(p.len() as u64).to_le_bytes());
        bytes.extend_from_slice(p.as_bytes());
        match entry {
            Entry::Dir => bytes.push(0),
            Entry::File(b) => {
                bytes.push(1);
                bytes.extend_from_slice(&(b.len() as u64).to_le_bytes());
                bytes.extend(b);
            }
            Entry::Link(p) => {
                let p = p.to_string_lossy();
                bytes.push(2);
                bytes.extend_from_slice(&(p.len() as u64).to_le_bytes());
                bytes.extend_from_slice(p.as_bytes());
            }
        }
    }
    sha256(&bytes)
}

fn node_bytes<'a>(store: &'a Store, id: &str) -> Result<&'a [u8]> {
    match store
        .inventory
        .get(Path::new(&format!("nodes/{id}/node.md")))
    {
        Some(Entry::File(b)) => Ok(b),
        _ => Err("missing captured source".into()),
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
    before: Option<Capture>,
    policy_identity: String,
}
impl ValidatedCapture {
    /// Consume checked immutable bytes for copying, not a live-source proof.
    /// The recipient must validate the materialized destination independently.
    pub(crate) fn into_snapshot(self) -> Store {
        self.capture.store
    }

    pub(crate) fn checked(&self, policy: &Policy) -> Result<&Capture> {
        policy.verify_stamp()?;
        require(
            policy.validation_identity()? == self.policy_identity,
            "validated capture policy changed",
        )?;
        self.capture.unchanged_all()?;
        if let Some(before) = &self.before {
            before.unchanged_all()?;
        }
        policy.verify_stamp()?;
        require(
            policy.validation_identity()? == self.policy_identity,
            "validated capture policy changed",
        )?;
        Ok(&self.capture)
    }

    pub(crate) fn into_capture(self, policy: &Policy) -> Result<Capture> {
        self.checked(policy)?;
        Ok(self.capture)
    }
}

/// Recheck the complete candidate, optionally against an immutable before view
/// and a prior logical version. Never normalize/mutate authored bytes or caches.
pub fn validate(policy: &Policy, request: &Request) -> Report {
    validate_with_transition(policy, request, None)
}

/// Only the native publisher supplies this in-memory capability, never JSON or
/// a caller's assertion that an edit is body-only. All non-graph checks remain.
pub(crate) fn validate_with_transition(
    policy: &Policy,
    request: &Request,
    proof: Option<(&Transition<'_>, &ProvenBase)>,
) -> Report {
    validate_with_capture(policy, request, proof).0
}

/// Retain only the exact capture that passed every ordinary candidate check and
/// final barrier. A rejected or stale observation never carries authority.
pub(crate) fn validate_with_capture(
    policy: &Policy,
    request: &Request,
    proof: Option<(&Transition<'_>, &ProvenBase)>,
) -> (Report, Option<ValidatedCapture>) {
    let mut report = Report {
        status: "current",
        version: None,
        before_version: None,
        nodes: 0,
        rows: vec![],
        failure: None,
        graph: GraphCheck {
            mode: "full_law",
            reason: "unproven base",
            base_version: None,
            candidate_version: None,
        },
    };
    let capture = match check(policy, request, proof, &mut report) {
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
                return (report, None);
            }
            // An unlocated failure (including helper timeout) does not establish
            // that any individual node is invalid. Keep one candidate-wide row.
            report.reject(None, "capture_or_graph_invalid", CANDIDATE_RULE, None);
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
fn graph_diagnostics(store: &Store, report: &mut Report) {
    let mut parents: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (source, children) in &store.children {
        for target in children {
            parents.entry(target).or_default().push(source);
        }
    }
    for (source, node) in &store.nodes {
        for reference in &node.see_also {
            if !store.nodes.contains_key(&reference.target) {
                report.structural(
                    Some(source),
                    Some(&reference.target),
                    "laws/Spec.bend:refs_valid",
                );
                report.rows.last_mut().unwrap().check = "dangling_reference";
            }
        }
        let ps = parents
            .get(source.as_str())
            .map(Vec::as_slice)
            .unwrap_or_default();
        let placements = ps.len() + store.roots.iter().filter(|r| *r == source).count();
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
    let mut pending: Vec<_> = store.roots.iter().map(String::as_str).collect();
    while let Some(id) = pending.pop() {
        if rooted.insert(id) {
            pending.extend(selected_children.get(id).into_iter().flatten().copied());
        }
    }
    for id in store.nodes.keys() {
        if !rooted.contains(id.as_str()) {
            report.structural(
                Some(id),
                parents.get(id.as_str()).and_then(|p| p.first()).copied(),
                "laws/Spec.bend:rooted",
            );
            report.rows.last_mut().unwrap().check = "not_rooted";
        }
    }
}

fn check(
    policy: &Policy,
    request: &Request,
    proof: Option<(&Transition<'_>, &ProvenBase)>,
    report: &mut Report,
) -> Result<Option<ValidatedCapture>> {
    // Pin before the native law/transition, but do not preempt its existing
    // failure stage or candidate capture/version observations with a new guard.
    // An initial identity-read error can never mint a successful capability.
    let policy_identity = policy.validation_identity();
    let candidate = report.observe(
        "candidate_capture",
        Capture::read_for_validation(&request.store),
    )?;
    let store = &candidate.store;
    report.nodes = store.nodes.len();
    report.version = Some(version(store));
    let before = report.observe(
        "before_capture",
        request
            .before
            .as_deref()
            .map(|p| Capture::read(p, None))
            .transpose(),
    )?;
    report.graph.candidate_version = report.version.clone();
    report.graph.base_version = before.as_ref().map(|b| version(&b.store));
    let preserved = if let Some((transition, base)) = proof {
        if report.graph.base_version.as_deref() != Some(base.version()) {
            report.graph.reason = "base version mismatch";
            false
        } else {
            // Errors are not unsupported-class fallback: helper/policy/source
            // failures reject, even if a subsequent full check might succeed.
            report.graph.mode = "transition_error";
            report.graph.reason = "transition failed";
            report.observe("graph_transition", transition.check_policy(policy))?;
            match report.observe(
                "graph_transition",
                transition.check_captured(base, base.version(), &candidate, || {}),
            )? {
                Preservation::Preserved {
                    base_version,
                    candidate_version,
                } => {
                    require(
                        Some(&candidate_version) == report.version.as_ref()
                            && Some(&base_version) == report.graph.base_version.as_ref(),
                        "transition capture mismatch",
                    )?;
                    report.graph.mode = "preserved";
                    report.graph.reason = "compiled Bend preservation executed";
                    true
                }
                Preservation::FullLawRequired(reason) => {
                    report.graph.mode = "full_law";
                    report.graph.reason = reason;
                    false
                }
            }
        }
    } else {
        false
    };
    // Structural errors carry only UUIDs/rule locators, never raw parser paths or prose.
    if !preserved && !report.observe("graph_policy", policy.graph_valid(store))? {
        graph_diagnostics(store, report);
        if report.rows.is_empty() {
            report.reject(None, "graph_invalid", CANDIDATE_RULE, None);
        }
        return Ok(None);
    }
    if request
        .expected_version
        .as_ref()
        .is_some_and(|v| Some(v) != report.version.as_ref())
    {
        report.reject(None, "receipt_version_changed", CANDIDATE_RULE, None);
    }
    if let Some(before) = &before {
        if !preserved {
            report.observe("before_graph_policy", policy.validate(&before.store))?;
        }
        report.before_version = Some(version(&before.store));
    }
    require(
        request
            .explicit
            .iter()
            .all(|id| store.nodes.contains_key(id))
            && (request.before.is_some() || request.explicit.is_empty()),
        "invalid explicit requests",
    )?;
    let tokenizer = tiktoken_rs::o200k_base_singleton();
    let mut states = BTreeMap::new();
    let mut transitions = BTreeMap::new();
    for (id, node) in &store.nodes {
        let tokens = tokenizer.lock().encode_ordinary(&node.content).len();
        report.rows.push(Row {
            id: Some(id.clone()),
            check: "crystallization_tokens",
            rule: CEILING_RULE,
            tokens: Some(tokens),
            source: None,
            target: None,
            law: None,
        });
        if tokens > CEILING {
            report.reject(
                Some(id),
                "crystallization_over_limit",
                CEILING_RULE,
                Some(tokens),
            );
        }
        let fields = &candidate.metadata[id];
        if fields.contains_key("ruled_out") {
            report.reject(Some(id), "ruled_out_removed", FIELD_RULE, None);
        }
        if fields.contains_key("content") {
            report.reject(Some(id), "body_metadata_duplicate", FIELD_RULE, None);
        }
        for key in ["scope", "warnings"] {
            if let Some(v) = fields.get(key)
                && serde_json::from_str::<String>(v).is_err()
            {
                report.reject(Some(id), "authored_field_type", FIELD_RULE, None);
            }
        }
        let mut targets = BTreeSet::new();
        if node
            .see_also
            .iter()
            .any(|r| r.target == *id || !targets.insert(&r.target))
        {
            report.reject(Some(id), "self_or_duplicate_reference", FIELD_RULE, None);
        }
        let requested = match state(&candidate, id) {
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
        let changed = before.as_ref().is_some_and(|b| {
            b.store.nodes.contains_key(id)
                && (node_bytes(&b.store, id).ok() != node_bytes(store, id).ok()
                    || b.store.children[id] != store.children[id])
        });
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
        if requested.status == Status::Resolved && node.content.trim().is_empty() {
            report.reject(Some(id), "resolved_body_blank", STATUS_RULE, None);
        }
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
        before,
        policy_identity,
    }))
}
