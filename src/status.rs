//! Candidate-bound authored status validation. No semantic completion inference.
use crate::coherent_capture::{Capture, Metadata, Relationships};
use crate::{Entry, Policy, Result, Store, markdown, require, uuid};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    NotExamined,
    InProgress,
    Stuck,
    Deferred,
    Discarded,
    Resolved,
}

impl Status {
    pub fn blocks(self) -> bool {
        matches!(self, Self::NotExamined | Self::InProgress | Self::Stuck)
    }

    fn reason(self) -> Option<&'static str> {
        match self {
            Self::Stuck => Some("blocker"),
            Self::Deferred => Some("deferred_reason"),
            Self::Discarded => Some("discard_reason"),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub status: Status,
    pub reasons: BTreeMap<String, String>,
}

impl State {
    fn read(text: &str) -> Result<Self> {
        // Reuse the native metadata grammar, including duplicate/type checks.
        let parsed = markdown(text)?;
        Self::from_fields(|key| parsed.fields.get(key).copied())
    }

    fn from_fields<'a>(field: impl Fn(&str) -> Option<&'a str>) -> Result<Self> {
        let status = serde_json::from_str(field("status").ok_or("missing authored status")?)?;
        let mut reasons = BTreeMap::new();
        for key in ["blocker", "deferred_reason", "discard_reason"] {
            if let Some(value) = field(key) {
                reasons.insert(key.to_owned(), serde_json::from_str(value)?);
            }
        }
        Ok(Self { status, reasons })
    }

    fn validate(&self) -> Result<()> {
        if let Some(key) = self.status.reason() {
            require(
                self.reasons.get(key).is_some_and(|s| !s.trim().is_empty()),
                "missing or blank active status reason",
            )?;
        }
        Ok(())
    }
}

/// Pure first-examination/explicit-transition model. Creation is not an edit.
/// Unlisted state changes are refused except the prescribed automatic activation.
pub fn transition(
    old: Option<&State>,
    requested: &State,
    authored_changed: bool,
    explicit: bool,
) -> Result<State> {
    let mut result = requested.clone();
    if let Some(old) = old {
        if !explicit {
            result.status = if old.status == Status::NotExamined && authored_changed {
                Status::InProgress
            } else {
                old.status
            };
            require(
                requested.status == old.status || requested.status == result.status,
                "status change needs an explicit request",
            )?;
        }
        if old.status != result.status
            && let Some(key) = old.status.reason()
        {
            result.reasons.remove(key);
        }
    }
    result.validate()?;
    Ok(result)
}

/// Pure direct-child closure. Reopening propagates upward, never downward.
/// Explicit resolution is rejected rather than silently changed into activation.
pub fn child_gate(
    states: &mut BTreeMap<String, State>,
    children: &BTreeMap<String, Vec<String>>,
    explicit: &BTreeSet<String>,
) -> Result<()> {
    let mut blocked: Vec<_> = states
        .iter()
        .filter(|(id, state)| {
            state.status == Status::Resolved
                && children[*id].iter().any(|c| states[c].status.blocks())
        })
        .map(|(id, _)| id.clone())
        .collect();
    if blocked.is_empty() {
        return Ok(());
    }
    // Build only when reopening is needed. Each reopened node visits its parents
    // once, rather than scanning the entire store at every ancestor level.
    let mut parents: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (id, kids) in children {
        if states.get(id).is_some_and(|s| s.status == Status::Resolved) {
            for child in kids {
                parents.entry(child).or_default().push(id);
            }
        }
    }
    while !blocked.is_empty() {
        let mut next = BTreeSet::new();
        for id in blocked {
            require(
                !explicit.contains(&id),
                "active direct child blocks resolution",
            )?;
            states.get_mut(&id).unwrap().status = Status::InProgress;
            for parent in parents.get(id.as_str()).into_iter().flatten() {
                next.insert((*parent).to_owned());
            }
        }
        // Preserve sorted waves, including the original partial state on error.
        blocked = next
            .into_iter()
            .filter(|id| states[id].status == Status::Resolved)
            .collect();
    }
    Ok(())
}

fn node_path(id: &str) -> PathBuf {
    PathBuf::from(format!("nodes/{id}/node.md"))
}

fn bytes<'a>(store: &'a Store, id: &str) -> Result<&'a [u8]> {
    match store.inventory.get(&node_path(id)) {
        Some(Entry::File(bytes)) => Ok(bytes),
        _ => Err("missing captured node bytes".into()),
    }
}

fn states(metadata: &Metadata) -> Result<BTreeMap<String, State>> {
    metadata
        .keys()
        .map(|id| {
            Ok((
                id.clone(),
                State::from_fields(|key| metadata[id].get(key).map(String::as_str))?,
            ))
        })
        .collect()
}

// The native parser has already validated these lines. Patch only a value or
// remove the departing reason line; preserve all other spellings and body bytes.
fn patch(text: &str, key: &str, value: Option<&str>) -> Result<String> {
    let parsed = markdown(text)?;
    require(parsed.fields.contains_key(key), "missing patched metadata")?;
    let mut offset = text.find('\n').ok_or("missing opening fence")? + 1;
    for raw in text[offset..].split_inclusive('\n') {
        let line = raw.trim_end_matches(['\n', '\r']);
        if line == "---" {
            break;
        }
        if let Some((k, v)) = line.split_once(':')
            && k.trim_matches([' ', '\t']) == key
        {
            let mut out = text.to_owned();
            if let Some(value) = value {
                let start = offset + line.find(':').unwrap() + 1 + v.len()
                    - v.trim_start_matches([' ', '\t']).len();
                let end = start + v.trim_matches([' ', '\t']).len();
                out.replace_range(start..end, value);
            } else {
                out.replace_range(offset..offset + raw.len(), "");
            }
            return Ok(out);
        }
        offset += raw.len();
    }
    Err("missing patched metadata line".into())
}

fn plan(before: &Capture, candidate: &Capture, explicit: &BTreeSet<String>) -> Result<Store> {
    let old = states(&before.metadata)?;
    let requested = states(&candidate.metadata)?;
    let before = &before.store;
    let candidate = &candidate.store;
    require(
        explicit
            .iter()
            .all(|id| uuid(id) && candidate.nodes.contains_key(id)),
        "explicit request needs a candidate UUID",
    )?;
    for state in old.values() {
        state.validate()?;
    }
    let mut result = BTreeMap::new();
    for (id, state) in &requested {
        let changed = before.nodes.contains_key(id)
            && (bytes(before, id)? != bytes(candidate, id)?
                || before.children[id] != candidate.children[id]);
        result.insert(
            id.clone(),
            transition(old.get(id), state, changed, explicit.contains(id))?,
        );
    }
    child_gate(&mut result, &candidate.children, explicit)?;
    let mut normalized = candidate.clone();
    for (id, state) in result {
        let raw = &requested[&id];
        let mut text = std::str::from_utf8(bytes(candidate, &id)?)?.to_owned();
        if state.status != raw.status {
            text = patch(
                &text,
                "status",
                Some(&serde_json::to_string(&state.status)?),
            )?;
        }
        for key in raw
            .reasons
            .keys()
            .filter(|key| !state.reasons.contains_key(*key))
        {
            text = patch(&text, key, None)?;
        }
        if text.as_bytes() == bytes(candidate, &id)? {
            continue;
        }
        require(text.len() <= crate::LIMIT, "oversized normalized node")?;
        require(
            State::read(&text)? == state,
            "status patch did not match model",
        )?;
        normalized
            .nodes
            .insert(id.clone(), markdown(&text)?.authored);
        normalized
            .inventory
            .insert(node_path(&id), Entry::File(text.into_bytes()));
    }
    Ok(normalized)
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PauseBeforeAccept {
    pub ready: PathBuf,
    pub release: PathBuf,
}

impl PauseBeforeAccept {
    fn wait(&self, stores: &[&Path]) -> Result<()> {
        for p in [&self.ready, &self.release] {
            let parent = fs::canonicalize(p.parent().ok_or("missing barrier parent")?)?;
            require(
                stores.iter().all(|s| !parent.starts_with(s)),
                "barrier must be outside stores",
            )?;
        }
        require(self.ready != self.release, "barrier paths must differ")?;
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.ready)?
            .write_all(b"captured\n")?;
        let start = Instant::now();
        while !self.release.try_exists()? {
            require(
                start.elapsed() < Duration::from_secs(10),
                "inspection barrier expired",
            )?;
            thread::sleep(Duration::from_millis(5));
        }
        Ok(())
    }
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Acceptance {
    pub status: &'static str,
    pub normalized_ids: Vec<String>,
    pub reopened_ids: Vec<String>,
    pub nodes: BTreeMap<String, StateView>,
    pub changes: Vec<Change>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct Change {
    pub id: String,
    pub from: Status,
    pub to: Status,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub removed_reason_fields: Vec<String>,
}

impl Acceptance {
    /// Own writes only: neither an unchanged draft nor publication approval.
    pub fn cli_json(&self) -> serde_json::Value {
        if self.changes.is_empty() {
            serde_json::json!({"status": "unchanged"})
        } else {
            serde_json::json!({"status": "normalized", "changes": self.changes})
        }
    }

    pub fn report_reopenings(&self) {
        for id in &self.reopened_ids {
            eprintln!("[{id}] set to in_progress.");
        }
    }
}

#[derive(Debug)]
struct Incomplete {
    changes: Vec<Change>,
    failed_id: Option<String>,
    cause: Box<dyn std::error::Error>,
}

impl std::fmt::Display for Incomplete {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut value = serde_json::json!({
            "status": "incomplete", "write_state": "unknown",
            "changes": self.changes, "reason": self.cause.to_string(),
            "next_action": "retain and inspect the draft; some status patches may already be applied; recheck before retrying",
        });
        if let Some(id) = &self.failed_id {
            value["failed_id"] = serde_json::json!(id);
        }
        write!(f, "{value}")
    }
}
impl std::error::Error for Incomplete {}

// A successful rename is an own write even if a later patch or verification fails.
fn apply_changes(changes: &[Change], mut apply: impl FnMut(&str) -> Result<()>) -> Result<()> {
    for (index, change) in changes.iter().enumerate() {
        apply(&change.id).map_err(|cause| Incomplete {
            changes: changes[..index].to_vec(),
            failed_id: Some(change.id.clone()),
            cause,
        })?;
    }
    Ok(())
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct StateView {
    pub status: Status,
}

/// Validate the exact before/candidate stores and apply only prescribed status
/// patches. All validation and staging precede mutation. Parent reopenings are
/// written before child activations, so a write failure cannot introduce a stale
/// resolved ancestor. This is a bounded private-draft operation, not publication
/// approval or a general multi-writer filesystem transaction.
pub fn normalize(
    policy: &Policy,
    before: &Path,
    after: &Path,
    explicit: &BTreeSet<String>,
    pause: Option<&PauseBeforeAccept>,
) -> Result<Acceptance> {
    normalize_with_preflight(policy, before, after, explicit, pause, |_| Ok(()))
}

/// Checks the exact normalized receipt before applying its status changes.
pub fn normalize_with_preflight(
    policy: &Policy,
    before: &Path,
    after: &Path,
    explicit: &BTreeSet<String>,
    pause: Option<&PauseBeforeAccept>,
    preflight: impl FnOnce(&Acceptance) -> Result<()>,
) -> Result<Acceptance> {
    let before_capture = Capture::read(before, None)?;
    let candidate_capture = Capture::read(after, None)?;
    let before = &before_capture.store;
    let candidate = &candidate_capture.store;
    policy.validate(before)?;
    // The checked verdict is a function of the captured inventory alone: validate ->
    // graph_valid -> encode reads nodes/roots/children/inventory and never Store::path.
    // Reuse the preceding verdict only for a byte-identical complete inventory.
    if candidate.inventory != before.inventory {
        policy.validate(candidate)?;
    }
    let normalized = plan(&before_capture, &candidate_capture, explicit)?;
    // plan changes parsed nodes only together with their captured node.md bytes.
    // Reuse the preceding verdict only for the exact complete inventory.
    if normalized.inventory != candidate.inventory {
        policy.validate(&normalized)?;
    }
    let mut ids: Vec<_> = candidate
        .nodes
        .keys()
        .filter(|id| candidate.inventory[&node_path(id)] != normalized.inventory[&node_path(id)])
        .cloned()
        .collect();
    let relationships = Relationships::new(candidate);
    let depth = |id: &str| {
        let mut depth = 0;
        let mut child = id;
        while let Some(parent) = relationships.parents.get(child) {
            depth += 1;
            child = parent;
        }
        depth
    };
    ids.sort_by_key(|id| (depth(id), id.clone()));
    let candidate_states = states(&candidate_capture.metadata)?;
    // `ids` is exactly the set whose captured bytes differ, so every other node keeps
    // the status already parsed into the candidate metadata. Re-reading them here would
    // re-parse the whole store on every normalize just to build a preflight receipt.
    let changed: BTreeSet<&str> = ids.iter().map(String::as_str).collect();
    let nodes = normalized
        .nodes
        .keys()
        .map(|id| {
            let status = if changed.contains(id.as_str()) {
                State::read(std::str::from_utf8(bytes(&normalized, id)?)?)?.status
            } else {
                candidate_states
                    .get(id.as_str())
                    .ok_or("missing captured node state")?
                    .status
            };
            Ok((id.clone(), StateView { status }))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let reopened_ids = ids
        .iter()
        .filter(|id| {
            !explicit.contains(*id)
                && candidate_states[*id].status != Status::InProgress
                && nodes[*id].status == Status::InProgress
        })
        .cloned()
        .collect();
    let changes = ids
        .iter()
        .map(|id| {
            let from = &candidate_states[id];
            let to = State::read(std::str::from_utf8(bytes(&normalized, id)?)?)?;
            Ok(Change {
                id: id.clone(),
                from: from.status,
                to: to.status,
                removed_reason_fields: from
                    .reasons
                    .keys()
                    .filter(|key| !to.reasons.contains_key(*key))
                    .cloned()
                    .collect(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let receipt = Acceptance {
        status: "current",
        normalized_ids: ids.clone(),
        reopened_ids,
        nodes,
        changes,
    };
    preflight(&receipt)?;
    // ponytail: <=256 nodes; stage complete files instead of in-place writes.
    // Publication remains single-writer after the final identity check; a broader
    // unmanaged-writer guarantee needs Core's separately owned workspace boundary.
    let staging = tempfile::tempdir_in(candidate.path.parent().ok_or("missing store parent")?)?;
    for id in &ids {
        let path = staging.path().join(id);
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)?;
        file.write_all(bytes(&normalized, id)?)?;
        file.sync_all()?;
        fs::set_permissions(
            &path,
            fs::metadata(candidate.path.join(node_path(id)))?.permissions(),
        )?;
    }
    if let Some(pause) = pause {
        pause.wait(&[&before.path, &candidate.path])?;
    }
    before_capture.unchanged_all()?;
    candidate_capture.unchanged_all()?;
    policy.verify_stamp()?;
    apply_changes(&receipt.changes, |id| {
        let destination = candidate.path.join(node_path(id));
        require(
            crate::read_regular(&destination)? == bytes(candidate, id)?,
            "node changed before patch",
        )?;
        fs::rename(staging.path().join(id), destination)?;
        Ok(())
    })?;
    let verify = || -> Result<Acceptance> {
        let (accepted, metadata) = Store::capture_view(&candidate.path, None)?;
        require(
            accepted.inventory == normalized.inventory,
            "normalized candidate changed during acceptance",
        )?;
        let after_states = states(&metadata)?;
        let accepted = Acceptance {
            status: "current",
            normalized_ids: ids,
            reopened_ids: receipt.reopened_ids.clone(),
            changes: receipt.changes.clone(),
            nodes: after_states
                .into_iter()
                .map(|(id, state)| {
                    (
                        id,
                        StateView {
                            status: state.status,
                        },
                    )
                })
                .collect(),
        };
        require(
            accepted == receipt,
            "normalized receipt changed during acceptance",
        )?;
        Ok(accepted)
    };
    verify().map_err(|cause| {
        if receipt.changes.is_empty() {
            cause
        } else {
            Box::new(Incomplete {
                changes: receipt.changes,
                failed_id: None,
                cause,
            }) as Box<dyn std::error::Error>
        }
    })
}

#[cfg(test)]
mod output_tests {
    use super::*;

    #[test]
    fn a_later_write_failure_keeps_confirmed_changes_and_names_the_uncertain_patch() {
        let changes: Vec<_> = ["first", "second", "third"]
            .into_iter()
            .map(|id| Change {
                id: id.into(),
                from: Status::NotExamined,
                to: Status::InProgress,
                removed_reason_fields: vec![],
            })
            .collect();
        let mut writes = vec![];
        let error = apply_changes(&changes, |id| {
            require(id != "second", "synthetic write refusal")?;
            writes.push(id.to_owned());
            Ok(())
        })
        .unwrap_err();
        assert_eq!(writes, ["first"]);
        let output: serde_json::Value = serde_json::from_str(&error.to_string()).unwrap();
        assert_eq!(output["status"], "incomplete");
        assert_eq!(output["write_state"], "unknown");
        assert_eq!(output["failed_id"], "second");
        assert_eq!(output["changes"], serde_json::json!([changes[0]]));
        assert!(output["next_action"].as_str().unwrap().contains("retain"));
    }
}
