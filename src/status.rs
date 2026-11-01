//! Candidate-bound authored status validation. No semantic completion inference.
use crate::{Policy, Result, markdown, require, uuid};
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
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

    pub(crate) fn from_fields<'a>(field: impl Fn(&str) -> Option<&'a str>) -> Result<Self> {
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
    require(
        states.keys().all(|id| children.contains_key(id))
            && children.iter().all(|(id, kids)| {
                states.contains_key(id) && kids.iter().all(|child| states.contains_key(child))
            }),
        "missing status relationship endpoint",
    )?;
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
                &format!("node {id}: active direct child blocks resolution"),
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

struct Patch {
    bytes: Vec<u8>,
    state: State,
}

/// Pure normalization shared by filesystem drafts and immutable Git candidates.
pub(crate) fn normalized_states(
    before: &BTreeMap<String, State>,
    requested: &BTreeMap<String, State>,
    before_children: &BTreeMap<String, Vec<String>>,
    children: &BTreeMap<String, Vec<String>>,
    edited: &BTreeSet<String>,
    explicit: &BTreeSet<String>,
) -> Result<BTreeMap<String, State>> {
    status_depths(requested.keys(), children)?;
    require(
        explicit.iter().all(|id| uuid(id) && requested.contains_key(id)),
        "explicit request needs a candidate UUID",
    )?;
    for (id, state) in before {
        state.validate().map_err(|e| format!("node {id}: {e}"))?;
    }
    let mut result = BTreeMap::new();
    for id in requested.keys() {
        let changed = before.contains_key(id)
            && (edited.contains(id) || before_children.get(id) != children.get(id));
        let next = normalized_state(id, before.get(id), requested, children, changed, explicit)?;
        result.insert(id.clone(), next);
    }
    child_gate(&mut result, children, explicit)?;
    Ok(result)
}

// Shared by the map planner and the streaming baseline planner: retries must
// use exactly the same transition and reopening fallback in both paths.
fn normalized_state(
    id: &str,
    old: Option<&State>,
    requested: &BTreeMap<String, State>,
    children: &BTreeMap<String, Vec<String>>,
    changed: bool,
    explicit: &BTreeSet<String>,
) -> Result<State> {
    let state = &requested[id];
    match transition(old, state, changed, explicit.contains(id)) {
        Ok(next) => Ok(next),
        // A previously normalized reopening is valid on retry. Restore the
        // prior resolved state only for the same direct-child closure rule.
        Err(_)
            if !explicit.contains(id)
                && old.is_some_and(|s| s.status == Status::Resolved)
                && state.status == Status::InProgress
                && children[id].iter().any(|child| requested[child].status.blocks()) =>
        {
            let mut prior = state.clone();
            prior.status = Status::Resolved;
            Ok(prior)
        }
        Err(error) => Err(format!("node {id}: {error}").into()),
    }
}

fn status_depths<'a>(
    ids: impl Iterator<Item = &'a String>,
    children: &BTreeMap<String, Vec<String>>,
) -> Result<BTreeMap<String, usize>> {
    let ids: BTreeSet<_> = ids.collect();
    if let Some(id) = ids.iter().find(|id| !children.contains_key(**id)) {
        return Err(format!("node {id}: missing status children").into());
    }
    let mut placed = BTreeSet::new();
    for (id, kids) in children {
        require(ids.contains(id), &format!("node {id}: missing status parent"))?;
        for child in kids {
            require(ids.contains(child), &format!("node {id}: missing status child {child}"))?;
            require(placed.insert(child), &format!("node {child}: ambiguous status parent"))?;
        }
    }
    let mut depths = BTreeMap::new();
    let mut pending: VecDeque<_> = ids.iter()
        .filter(|id| !placed.contains(**id))
        .map(|id| ((*id).clone(), 0usize))
        .collect();
    while let Some((id, depth)) = pending.pop_front() {
        require(depths.insert(id.clone(), depth).is_none(), "cyclic status relationships")?;
        pending.extend(children[&id].iter().map(|child| (child.clone(), depth + 1)));
    }
    if let Some(id) = ids.iter().find(|id| !depths.contains_key(**id)) {
        return Err(format!("node {id}: cyclic status relationships").into());
    }
    Ok(depths)
}

fn patched_bytes(original: &[u8], raw: &State, state: &State) -> Result<Vec<u8>> {
    let mut text = std::str::from_utf8(original)?.to_owned();
    if state.status != raw.status {
        text = patch(&text, "status", Some(&serde_json::to_string(&state.status)?))?;
    }
    for key in raw.reasons.keys().filter(|key| !state.reasons.contains_key(*key)) {
        text = patch(&text, key, None)?;
    }
    require(text.len() <= crate::LIMIT, "oversized normalized node")?;
    require(State::read(&text)? == *state, "status patch did not match model")?;
    Ok(text.into_bytes())
}

// Only authored status inputs live beyond a node read. In particular, bodies,
// arbitrary metadata and generated/asset inventories are never retained.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Fingerprint {
    mode: u32,
    dev: u64,
    ino: u64,
    len: u64,
    mtime: (i64, i64),
    ctime: (i64, i64),
    links: u64,
}
impl Fingerprint {
    fn of(m: &fs::Metadata) -> Self {
        Self { mode: m.mode(), dev: m.dev(), ino: m.ino(), len: m.len(),
            mtime: (m.mtime(), m.mtime_nsec()), ctime: (m.ctime(), m.ctime_nsec()),
            links: m.nlink() }
    }
    fn at(path: &Path) -> Result<Self> { Ok(Self::of(&fs::symlink_metadata(path)?)) }
    fn unchanged(self, path: &Path) -> Result<()> {
        require(self == Self::at(path)?, &format!("changed status input: {}", path.display()))
    }
}

// Container timestamps are not inputs: a generated outline or unrelated asset
// can change them. Retain real-directory device/inode, and enumerate only the
// dynamic authored nodes/children namespaces at barriers.
#[derive(Clone, Copy, PartialEq, Eq)]
struct Directory(u64, u64);
impl Directory {
    fn at(path: &Path) -> Result<Self> {
        let m = fs::symlink_metadata(path)?;
        require(m.is_dir(), &format!("not a real status directory: {}", path.display()))?;
        Ok(Self(m.dev(), m.ino()))
    }
    fn unchanged(self, path: &Path) -> Result<()> {
        require(self == Self::at(path)?, "status owner directory changed")
    }
}

fn status_file(path: &Path, expected: Option<Fingerprint>) -> Result<(Vec<u8>, Fingerprint)> {
    let before = Fingerprint::at(path)?;
    if let Some(expected) = expected { require(before == expected, "changed status file")?; }
    let file = fs::OpenOptions::new().read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).open(path)?;
    let metadata = file.metadata()?;
    require(metadata.is_file() && metadata.nlink() == 1 && metadata.len() <= crate::LIMIT as u64,
        "unsafe or oversized status file")?;
    require(before == Fingerprint::of(&metadata), "status file replaced before read")?;
    let mut bytes = Vec::new();
    (&file).take(crate::LIMIT as u64 + 1).read_to_end(&mut bytes)?;
    require(bytes.len() <= crate::LIMIT, "oversized status file")?;
    require(before == Fingerprint::of(&file.metadata()?), "status file changed during read")?;
    before.unchanged(path)?;
    Ok((bytes, before))
}

struct StatusInput {
    hash: [u8; 32],
    text_hash: [u8; 32],
    node: Fingerprint,
    order: Option<Fingerprint>,
    owner: Directory,
    children: Directory,
    links: Vec<Fingerprint>,
    marker: Option<Fingerprint>,
}
struct StatusSnapshot {
    path: PathBuf,
    parents: Vec<(PathBuf, Directory)>,
    nodes_dir: Directory,
    marker: Option<Fingerprint>,
    states: BTreeMap<String, State>,
    children: BTreeMap<String, Vec<String>>,
    inputs: BTreeMap<String, StatusInput>,
}

fn status_members(path: &Path) -> Result<(BTreeSet<String>, bool)> {
    let mut names = BTreeSet::new();
    let mut marker = false;
    for entry in fs::read_dir(path)? {
        let name = entry?.file_name().into_string().map_err(|_| "non-UTF-8 status path")?;
        if name == ".keep" { marker = true; }
        else {
            require(uuid(&name), "status member is not a canonical UUID")?;
            names.insert(name);
        }
    }
    Ok((names, marker))
}
fn status_marker(path: &Path, present: bool) -> Result<Option<Fingerprint>> {
    if !present { return Ok(None); }
    let (bytes, fingerprint) = status_file(&path.join(".keep"), None)?;
    require(bytes.is_empty(), "nonempty status directory marker")?;
    Ok(Some(fingerprint))
}
impl StatusSnapshot {
    fn empty(path: &Path) -> Result<Self> {
        // Reject redirected ancestors rather than granting implicit sparse or
        // symlink inheritance. Keep these identities through every barrier.
        let absolute = if path.is_absolute() { path.to_owned() } else { std::env::current_dir()?.join(path) };
        let mut current = PathBuf::new();
        let mut parents = Vec::new();
        for component in absolute.components() {
            require(!matches!(component, std::path::Component::ParentDir), "unsafe status home path")?;
            current.push(component);
            parents.push((current.clone(), Directory::at(&current)?));
        }
        let path = fs::canonicalize(&absolute)?;
        let nodes = path.join("nodes");
        let nodes_dir = Directory::at(&nodes)?;
        let (ids, marker) = status_members(&nodes)?;
        let marker = if marker { Some(Fingerprint::at(&nodes.join(".keep"))?) } else { None };
        drop(ids);
        Ok(Self { path, parents, nodes_dir, marker,
            states: BTreeMap::new(), children: BTreeMap::new(), inputs: BTreeMap::new() })
    }

    fn read(path: &Path) -> Result<Self> {
        let mut snapshot = Self::empty(path)?;
        let nodes = snapshot.path.join("nodes");
        status_marker(&nodes, snapshot.marker.is_some())?;
        let (ids, _) = status_members(&nodes)?;
        for id in ids {
            let (state, ordered, input) = read_status_node(&nodes, &id)?;
            snapshot.states.insert(id.clone(), state);
            snapshot.children.insert(id.clone(), ordered);
            snapshot.inputs.insert(id, input);
        }
        Ok(snapshot)
    }

    fn owner_unchanged(&self, id: &str) -> Result<()> {
        for (path, directory) in &self.parents { directory.unchanged(path)?; }
        self.nodes_dir.unchanged(&self.path.join("nodes"))?;
        self.inputs[id].owner.unchanged(&self.path.join("nodes").join(id))
    }

    fn unchanged(&self) -> Result<()> {
        for (path, directory) in &self.parents { directory.unchanged(path)?; }
        let nodes = self.path.join("nodes");
        self.nodes_dir.unchanged(&nodes)?;
        let (ids, marker) = status_members(&nodes)?;
        require(ids.iter().eq(self.inputs.keys()) && marker == self.marker.is_some(),
            "status node membership changed")?;
        if let Some(marker) = self.marker { marker.unchanged(&nodes.join(".keep"))?; }
        for (id, input) in &self.inputs {
            let dir = nodes.join(id);
            input.owner.unchanged(&dir)?;
            input.node.unchanged(&dir.join("node.md"))?;
            if let Some(order) = input.order { order.unchanged(&dir.join("children.order"))?; }
            else { require(!dir.join("children.order").try_exists()?, "legacy order appeared during normalization")?; }
            let children = dir.join("children");
            input.children.unchanged(&children)?;
            let (members, marker) = status_members(&children)?;
            require(members.iter().eq(self.children[id].iter().collect::<BTreeSet<_>>().into_iter())
                && marker == input.marker.is_some(), "status child membership changed")?;
            if let Some(marker) = input.marker { marker.unchanged(&children.join(".keep"))?; }
            for (child, fingerprint) in self.children[id].iter().zip(&input.links) {
                fingerprint.unchanged(&children.join(child))?;
            }
        }
        Ok(())
    }
}

fn read_status_node(nodes: &Path, id: &str) -> Result<(State, Vec<String>, StatusInput)> {
    let dir = nodes.join(id);
    let owner = Directory::at(&dir)?;
    let children_path = dir.join("children");
    let children_dir = Directory::at(&children_path)?;
    let (bytes, node) = status_file(&dir.join("node.md"), None)?;
    let hash = crate::Sha256::digest(&bytes).into();
    let text_hash = crate::Sha256::digest(crate::ordering::without_position(std::str::from_utf8(&bytes)?).as_bytes()).into();
    let parsed = markdown(std::str::from_utf8(&bytes)?)
        .map_err(|e| format!("node {id}: {e}"))?;
    require(parsed.authored.id == id, "mismatched status node UUID")?;
    let state = State::from_fields(|key| parsed.fields.get(key).copied())
        .map_err(|e| format!("node {id}: {e}"))?;
    drop(parsed);
    drop(bytes);
    let order = if dir.join("children.order").try_exists()? {
        Some(status_file(&dir.join("children.order"), None)?.1)
    } else { None };
    let ordered = crate::ordering::children(nodes.parent().ok_or("missing status home")?, id)?;
    let (members, marker) = status_members(&children_path)?;
    require(ordered.iter().all(|id| uuid(id))
        && ordered.len() == members.len()
        && ordered.iter().collect::<BTreeSet<_>>() == members.iter().collect(),
        "missing, duplicate or non-child status order member")?;
    let marker = status_marker(&children_path, marker)?;
    let mut links = Vec::new();
    for child in &ordered {
        let link = children_path.join(child);
        let fingerprint = Fingerprint::at(&link)?;
        let destination = nodes.join(child);
        Directory::at(&destination)?;
        crate::safe_link(&link, &PathBuf::from(format!("../../{child}")), &destination)?;
        fingerprint.unchanged(&link)?;
        links.push(fingerprint);
    }
    owner.unchanged(&dir)?;
    children_dir.unchanged(&children_path)?;
    Ok((state, ordered, StatusInput { hash, text_hash, node, order, owner,
        children: children_dir, links, marker }))
}

// A baseline is never written. Keep one digest, not a second store-sized set of
// states, hashes, relationships and leaf fingerprints. Every barrier re-enumerates
// all relevant namespaces but never rereads node/order/marker bodies.
struct Baseline {
    path: PathBuf,
    source: PathBuf,
    fingerprint: [u8; 32],
}

fn digest_field(hash: &mut crate::Sha256, domain: &[u8], bytes: &[u8]) {
    hash.update((domain.len() as u64).to_le_bytes());
    hash.update(domain);
    hash.update((bytes.len() as u64).to_le_bytes());
    hash.update(bytes);
}
fn digest_directory(hash: &mut crate::Sha256, domain: &[u8], directory: Directory) {
    digest_field(hash, domain, &[]);
    hash.update(directory.0.to_le_bytes());
    hash.update(directory.1.to_le_bytes());
}
fn digest_leaf(hash: &mut crate::Sha256, domain: &[u8], leaf: Option<Fingerprint>) {
    digest_field(hash, domain, &[u8::from(leaf.is_some())]);
    if let Some(f) = leaf {
        // mode includes kind; signed time components keep their exact bit pattern.
        for value in [f.mode as u64, f.dev, f.ino, f.len, f.mtime.0 as u64,
            f.mtime.1 as u64, f.ctime.0 as u64, f.ctime.1 as u64, f.links] {
            hash.update(value.to_le_bytes());
        }
    }
}

impl Baseline {
    fn scan(
        source: &Path,
        read: bool,
        mut visit: impl FnMut(&str, &State, &[String], &StatusInput) -> Result<()>,
    ) -> Result<Self> {
        let home = StatusSnapshot::empty(source)?;
        let mut hash = crate::Sha256::new();
        digest_field(&mut hash, b"sifttext-status-baseline-v1", &[]);
        for (path, directory) in &home.parents {
            digest_field(&mut hash, b"ancestor-path", path.as_os_str().as_encoded_bytes());
            digest_directory(&mut hash, b"ancestor", *directory);
        }
        digest_field(&mut hash, b"ancestors-end", &[]);
        digest_directory(&mut hash, b"nodes", home.nodes_dir);
        digest_leaf(&mut hash, b"nodes-marker", home.marker);
        let nodes = home.path.join("nodes");
        if read { status_marker(&nodes, home.marker.is_some())?; }
        let (ids, marker) = status_members(&nodes)?;
        require(marker == home.marker.is_some(), "status node membership changed")?;
        digest_field(&mut hash, b"node-count", &(ids.len() as u64).to_le_bytes());
        for id in ids {
            let dir = nodes.join(&id);
            let children = dir.join("children");
            let (ordered, input) = if read {
                let (state, ordered, input) = read_status_node(&nodes, &id)?;
                visit(&id, &state, &ordered, &input)?;
                (ordered, input)
            } else {
                let owner = Directory::at(&dir)?;
                let children_dir = Directory::at(&children)?;
                let (members, marker) = status_members(&children)?;
                let ordered: Vec<_> = members.into_iter().collect();
                let links = ordered.iter().map(|child| Fingerprint::at(&children.join(child)))
                    .collect::<Result<Vec<_>>>()?;
                let input = StatusInput {
                    hash: [0; 32], text_hash: [0; 32], node: Fingerprint::at(&dir.join("node.md"))?,
                    order: if dir.join("children.order").try_exists()? { Some(Fingerprint::at(&dir.join("children.order"))?) } else { None },
                    owner, children: children_dir, links,
                    marker: if marker { Some(Fingerprint::at(&children.join(".keep"))?) } else { None },
                };
                (ordered, input)
            };
            digest_field(&mut hash, b"node-id", id.as_bytes());
            digest_directory(&mut hash, b"owner", input.owner);
            digest_directory(&mut hash, b"children", input.children);
            digest_leaf(&mut hash, b"node.md", Some(input.node));
            digest_leaf(&mut hash, b"children.order", input.order);
            digest_leaf(&mut hash, b"children-marker", input.marker);
            digest_field(&mut hash, b"child-count", &(ordered.len() as u64).to_le_bytes());
            // children.order bytes are guarded by their exact metadata fingerprint;
            // namespace traversal itself is canonical, independent of authored order.
            let mut links: Vec<_> = ordered.iter().zip(&input.links).collect();
            links.sort_by_key(|(child, _)| *child);
            for (child, fingerprint) in links {
                let link = children.join(child);
                let target = fs::read_link(&link)?;
                fingerprint.unchanged(&link)?;
                digest_field(&mut hash, b"child-name", child.as_bytes());
                digest_field(&mut hash, b"literal-target", target.as_os_str().as_encoded_bytes());
                digest_leaf(&mut hash, b"child-link", Some(*fingerprint));
            }
            input.owner.unchanged(&dir)?;
            input.children.unchanged(&children)?;
        }
        for (path, directory) in &home.parents { directory.unchanged(path)?; }
        home.nodes_dir.unchanged(&nodes)?;
        Ok(Self { path: home.path, source: source.to_owned(), fingerprint: hash.finalize().into() })
    }

    fn unchanged(&self) -> Result<()> {
        let current = Self::scan(&self.source, false, |_, _, _, _| Ok(()))?;
        require(current.fingerprint == self.fingerprint, "changed baseline status input")
    }
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
pub(crate) fn apply_changes(changes: &[Change], mut apply: impl FnMut(&str) -> Result<()>) -> Result<()> {
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

/// Plan exact Markdown replacements without touching a mutable working tree.
pub(crate) fn normalize_git(
    reader: &mut crate::git_snapshot::GitReader,
    before: &crate::git_snapshot::GitView,
    candidate: &crate::git_snapshot::GitView,
    explicit: &BTreeSet<String>,
) -> Result<(Acceptance, BTreeMap<String, Vec<u8>>)> {
    let old = before.nodes.iter().map(|(id, node)| (id.clone(), node.state.clone())).collect();
    let requested: BTreeMap<_, _> = candidate.nodes.iter()
        .map(|(id, node)| (id.clone(), node.state.clone())).collect();
    let edited = candidate.nodes.iter()
        .filter(|(id, node)| before.nodes.get(*id).is_none_or(|old| old.text_hash != node.text_hash))
        .map(|(id, _)| id.clone()).collect();
    let result = normalized_states(
        &old, &requested, &before.children, &candidate.children, &edited, explicit,
    )?;
    drop(old);
    drop(edited);
    let mut patches = BTreeMap::new();
    for (id, state) in &result {
        if state != &requested[id] {
            let original = reader.bytes(candidate, id)?;
            patches.insert(id.clone(), patched_bytes(&original, &requested[id], state)
                .map_err(|e| format!("node {id}: {e}"))?);
        }
    }
    let depths = status_depths(candidate.nodes.keys(), &candidate.children)?;
    let mut ids: Vec<_> = patches.keys().cloned().collect();
    ids.sort_by_key(|id| (depths[id], id.clone()));
    drop(depths);
    let reopened_ids = ids.iter()
        .filter(|id| !explicit.contains(*id)
            && requested[*id].status != Status::InProgress
            && result[*id].status == Status::InProgress)
        .cloned().collect();
    let changes = ids.iter().map(|id| Change {
        id: id.clone(), from: requested[id].status, to: result[id].status,
        removed_reason_fields: requested[id].reasons.keys()
            .filter(|key| !result[id].reasons.contains_key(*key)).cloned().collect(),
    }).collect();
    drop(requested);
    Ok((Acceptance {
        status: "current", normalized_ids: ids, reopened_ids, changes,
        nodes: result.into_iter().map(|(id, state)| (id, StateView { status: state.status })).collect(),
    }, patches))
}

/// Read the exact before/candidate stores and apply only prescribed status
/// patches. This edits a draft; graph acceptance belongs to submit. All planning
/// and staging precede mutation. Parent reopenings are
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
    policy.verify_stamp()?;
    let sparse = |path: &Path| -> Result<bool> {
        Ok(fs::symlink_metadata(path.join(".git")).is_ok() && crate::local::sparse_checkout(path)?)
    };
    if sparse(after)? || sparse(before)? {
        return normalize_sparse(policy, before, after, explicit, pause, preflight);
    }
    let mut candidate = StatusSnapshot::read(after)?;
    let depths = status_depths(candidate.states.keys(), &candidate.children)?;
    require(explicit.iter().all(|id| uuid(id) && candidate.states.contains_key(id)),
        "explicit request needs a candidate UUID")?;
    // Creation is not an edit. Existing nodes use the same pure transition
    // helper as the immutable Git planner while the baseline streams.
    let mut result = BTreeMap::new();
    let before = Baseline::scan(before, true, |id, old, children, input| {
        old.validate().map_err(|e| format!("node {id}: {e}"))?;
        if let Some(requested) = candidate.inputs.get(id) {
            let changed = input.text_hash != requested.text_hash || children != candidate.children[id];
            result.insert(id.to_owned(), normalized_state(id, Some(old), &candidate.states,
                &candidate.children, changed, explicit)?);
        }
        Ok(())
    })?;
    for id in candidate.states.keys() {
        if !result.contains_key(id) {
            result.insert(id.clone(), normalized_state(id, None, &candidate.states,
                &candidate.children, false, explicit)?);
        }
    }
    child_gate(&mut result, &candidate.children, explicit)?;
    let mut patches = BTreeMap::new();
    for (id, state) in &result {
        let raw = &candidate.states[id];
        if state == raw { continue; }
        candidate.owner_unchanged(id)?;
        let (original, _) = status_file(&candidate.path.join(node_path(id)), Some(candidate.inputs[id].node))?;
        require(<[u8; 32]>::from(crate::Sha256::digest(&original)) == candidate.inputs[id].hash,
            "node changed before patch planning")?;
        patches.insert(id.clone(), Patch {
            bytes: patched_bytes(&original, raw, state).map_err(|e| format!("node {id}: {e}"))?,
            state: state.clone(),
        });
    }
    let mut ids: Vec<_> = patches.keys().cloned().collect();
    ids.sort_by_key(|id| (depths[id], id.clone()));
    let reopened_ids = ids.iter()
        .filter(|id| !explicit.contains(*id)
            && candidate.states[*id].status != Status::InProgress
            && result[*id].status == Status::InProgress)
        .cloned().collect();
    let changes = ids.iter().map(|id| Change {
        id: id.clone(), from: candidate.states[id].status, to: patches[id].state.status,
        removed_reason_fields: candidate.states[id].reasons.keys()
            .filter(|key| !patches[id].state.reasons.contains_key(*key)).cloned().collect(),
    }).collect();
    let receipt = Acceptance {
        status: "current", normalized_ids: ids.clone(), reopened_ids, changes,
        nodes: result.into_iter().map(|(id, state)| (id, StateView { status: state.status })).collect(),
    };
    drop(depths);
    // No expected/old working maps survive into staging. Only write guards,
    // hierarchy, receipt and changed complete-file patches remain.
    candidate.states.clear();
    crate::release_unused_memory();
    preflight(&receipt)?;
    // Complete changed files are durable before the first rename; unrelated
    // resources are neither copied, certified nor written.
    let staging = tempfile::tempdir_in(candidate.path.parent().ok_or("missing store parent")?)?;
    for id in &ids {
        candidate.owner_unchanged(id)?;
        candidate.inputs[id].node.unchanged(&candidate.path.join(node_path(id)))?;
        let mut file = fs::OpenOptions::new().create_new(true).write(true).open(staging.path().join(id))?;
        file.write_all(&patches[id].bytes)?;
        file.set_permissions(fs::Permissions::from_mode(candidate.inputs[id].node.mode))?;
        file.sync_all()?;
    }
    if let Some(pause) = pause { pause.wait(&[&before.path, &candidate.path])?; }
    before.unchanged()?;
    candidate.unchanged()?;
    policy.verify_stamp()?;
    apply_changes(&receipt.changes, |id| {
        candidate.owner_unchanged(id)?;
        let destination = candidate.path.join(node_path(id));
        let (actual, _) = status_file(&destination, Some(candidate.inputs[id].node))?;
        require(<[u8; 32]>::from(crate::Sha256::digest(&actual)) == candidate.inputs[id].hash,
            "node changed before patch")?;
        candidate.owner_unchanged(id)?;
        candidate.inputs[id].node.unchanged(&destination)?;
        fs::rename(staging.path().join(id), &destination)?;
        // A rename is an own write even if this read or a later barrier fails.
        let (actual, fingerprint) = status_file(&destination, None)?;
        require(actual == patches[id].bytes, "written status patch changed")?;
        candidate.inputs.get_mut(id).unwrap().node = fingerprint;
        fs::File::open(destination.parent().ok_or("missing node parent")?)?.sync_all()?;
        candidate.owner_unchanged(id)?;
        Ok(())
    })?;
    let verify = || -> Result<()> {
        if before.path != candidate.path { before.unchanged()?; }
        candidate.unchanged()?;
        policy.verify_stamp()
    };
    verify().map_err(|cause| {
        if receipt.changes.is_empty() { cause }
        else { Box::new(Incomplete { changes: receipt.changes.clone(), failed_id: None, cause })
            as Box<dyn std::error::Error> }
    })?;
    Ok(receipt)
}

// Sparse normalization consumes the same complete logical source as queries.
// It does not scan/hydrate omitted directories or maintain a parallel cache.
struct IndexedStatuses {
    version: String,
    states: BTreeMap<String, State>,
    children: BTreeMap<String, Vec<String>>,
    hashes: BTreeMap<String, String>,
    positions: BTreeMap<String, (Option<serde_json::Value>, String)>,
}
impl IndexedStatuses {
    fn read(policy: &Policy, path: &Path) -> Result<Self> {
        let (version, (states, children, hashes, positions)) = crate::index::with_current(policy, path, |tx, _| {
            let mut states = BTreeMap::new();
            let mut children = BTreeMap::new();
            let mut hashes = BTreeMap::new();
            let mut positions = BTreeMap::new();
            let mut stmt = tx.prepare("SELECT n.id,n.record,s.content_hash FROM nodes n
                JOIN sources s ON s.path='nodes/'||n.id||'/node.md'")?;
            let mut rows = stmt.query([])?;
            while let Some(row) = rows.next()? {
                let id: String = row.get(0)?;
                let mut record: serde_json::Value = serde_json::from_str(&row.get::<_, String>(1)?)?;
                let state = State {
                    status: serde_json::from_value(record["status"].clone())?,
                    reasons: ["blocker", "deferred_reason", "discard_reason"].into_iter()
                        .filter_map(|key| record.get(key).map(|value| (key.to_owned(), value)))
                        .map(|(key, value)| Ok((key, serde_json::from_value(value.clone())?)))
                        .collect::<Result<_>>()?,
                };
                children.insert(id.clone(), serde_json::from_value(record["children"].clone())?);
                hashes.insert(id.clone(), row.get(2)?);
                let object = record.as_object_mut().ok_or("invalid status record")?;
                let position = object.remove("position");
                object.remove("children");
                positions.insert(id.clone(), (position, crate::sha256(&serde_json::to_vec(&record)?)));
                states.insert(id, state);
            }
            let count: i64 = tx.query_row("SELECT count(*) FROM nodes", [], |r| r.get(0))?;
            require(states.len() as i64 == count, "missing logical status source identity")?;
            Ok((states, children, hashes, positions))
        })?;
        Ok(Self { version, states, children, hashes, positions })
    }
    fn unchanged(&self, policy: &Policy, path: &Path) -> Result<()> {
        let (version, ()) = crate::index::with_current(policy, path, |_, _| Ok(()))?;
        require(version == self.version, "logical status source changed")
    }
}

fn normalize_sparse(
    policy: &Policy, before_path: &Path, after_path: &Path,
    explicit: &BTreeSet<String>, pause: Option<&PauseBeforeAccept>,
    preflight: impl FnOnce(&Acceptance) -> Result<()>,
) -> Result<Acceptance> {
    let before_owner = StatusSnapshot::empty(before_path)?;
    let after_owner = StatusSnapshot::empty(after_path)?;
    let before_path = &before_owner.path;
    let after_path = &after_owner.path;
    let before = IndexedStatuses::read(policy, &before_path)?;
    let mut candidate = IndexedStatuses::read(policy, &after_path)?;
    let edited = candidate.hashes.iter().filter(|(id, hash)| {
        if before.hashes.get(*id) == Some(*hash) { return false; }
        // Position-only changes are display changes, not a restart of the work.
        !before.positions.get(*id).is_some_and(|old| {
            let new = &candidate.positions[*id];
            old.0 != new.0 && old.1 == new.1
        })
    }).map(|(id, _)| id.clone()).collect();
    let result = normalized_states(&before.states, &candidate.states, &before.children,
        &candidate.children, &edited, explicit)?;
    let depths = status_depths(candidate.states.keys(), &candidate.children)?;
    let mut ids: Vec<_> = result.iter().filter(|(id, state)| candidate.states[*id] != **state)
        .map(|(id, _)| id.clone()).collect();
    ids.sort_by_key(|id| (depths[id], id.clone()));
    let receipt = Acceptance {
        status: "current",
        normalized_ids: ids.clone(),
        reopened_ids: ids.iter().filter(|id| !explicit.contains(*id)
            && candidate.states[*id].status != Status::InProgress
            && result[*id].status == Status::InProgress).cloned().collect(),
        changes: ids.iter().map(|id| Change {
            id: id.clone(), from: candidate.states[id].status, to: result[id].status,
            removed_reason_fields: candidate.states[id].reasons.keys()
                .filter(|key| !result[id].reasons.contains_key(*key)).cloned().collect(),
        }).collect(),
        nodes: result.iter().map(|(id, state)| (id.clone(), StateView { status: state.status })).collect(),
    };
    preflight(&receipt)?;
    crate::local::materialize_nodes(&after_path, &ids)?;
    let mut staged = BTreeMap::new();
    for id in &ids {
        let destination = after_path.join(node_path(id));
        let owner = Directory::at(destination.parent().ok_or("missing node parent")?)?;
        let (bytes, fingerprint) = status_file(&destination, None)?;
        require(crate::sha256(&bytes) == candidate.hashes[id], "node changed before sparse status planning")?;
        let patch = patched_bytes(&bytes, &candidate.states[id], &result[id])?;
        let mut file = tempfile::NamedTempFile::new_in(destination.parent().ok_or("missing node parent")?)?;
        file.write_all(&patch)?;
        file.as_file().set_permissions(fs::Permissions::from_mode(fingerprint.mode))?;
        file.as_file().sync_all()?;
        candidate.hashes.insert(id.clone(), crate::sha256(&patch));
        staged.insert(id.clone(), (file, fingerprint, owner));
    }
    if let Some(pause) = pause { pause.wait(&[&before_path, &after_path])?; }
    before.unchanged(policy, &before_path)?;
    candidate.unchanged(policy, &after_path)?;
    policy.verify_stamp()?;
    apply_changes(&receipt.changes, |id| {
        let destination = after_path.join(node_path(id));
        let (file, fingerprint, owner) = staged.remove(id).ok_or("missing staged status patch")?;
        for (path, identity) in &after_owner.parents { identity.unchanged(path)?; }
        after_owner.nodes_dir.unchanged(&after_path.join("nodes"))?;
        owner.unchanged(destination.parent().ok_or("missing node parent")?)?;
        fingerprint.unchanged(&destination)?;
        file.persist(&destination)?;
        fs::File::open(destination.parent().ok_or("missing node parent")?)?.sync_all()?;
        Ok(())
    })?;
    let verify = || -> Result<()> {
        if before_path != after_path { before.unchanged(policy, &before_path)?; }
        let actual = IndexedStatuses::read(policy, &after_path)?;
        require(actual.hashes == candidate.hashes && actual.children == candidate.children,
            "logical source changed during sparse normalization")?;
        policy.verify_stamp()
    };
    verify().map_err(|cause| if receipt.changes.is_empty() { cause } else {
        Box::new(Incomplete { changes: receipt.changes.clone(), failed_id: None, cause }) as Box<dyn std::error::Error>
    })?;
    Ok(receipt)
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
