//! Review-only worktree deltas. Git supplies authored changes; the packet supplies
//! proposed artifact links, which are deliberately outside authored Git history.
use super::*;
use crate::{git_snapshot::Batch, local::{reader_command, reader_git}};
use std::path::{Component, PathBuf};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Proposal {
    #[serde(default)]
    pub base: String,
    pub artifacts: Vec<Artifact>,
    #[serde(default)]
    pub deletions: Vec<Deletion>,
    #[serde(default)]
    pub work_record: Vec<Stage>,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Artifact {
    pub owner: String,
    pub operation: Operation,
    /// Node-relative link location, including artifacts/.
    pub destination: String,
    /// Exact absolute target; rendering never creates, follows or deletes it.
    pub target: String,
    pub reason: String,
}
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum Operation { Add, Remove }
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Deletion { pub node: String, pub reason: String }
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stage { pub title: String, pub body: String }

#[derive(Clone)]
struct Record { node: Node, fields: serde_json::Map<String, Value> }
impl Record {
    fn from_fields(mut fields: serde_json::Map<String, Value>, children: Vec<String>) -> Result<Self> {
        fields.remove("children");
        Ok(Self {
            node: Node {
                name: fields["name"].as_str().ok_or("missing node name")?.to_owned(),
                status: serde_json::from_value(fields["status"].clone())?,
                children,
            },
            fields,
        })
    }
    fn from_markdown(bytes: &[u8], children: Vec<String>) -> Result<Self> {
        let parsed = crate::markdown(std::str::from_utf8(bytes)?)?;
        let mut fields = parsed.fields.iter().map(|(k,v)| Ok(((*k).to_owned(), serde_json::from_str(v)?)))
            .collect::<Result<serde_json::Map<_, _>>>()?;
        fields.insert("content".into(), json!(parsed.authored.content));
        Self::from_fields(fields, children)
    }
    fn current(connection: &Connection, id: &str) -> Result<Option<Self>> {
        let text: Option<String> = connection.query_row("SELECT record FROM nodes WHERE id=?1", [id], |r| r.get(0)).optional()?;
        text.map(|text| {
            let mut fields: serde_json::Map<String, Value> = serde_json::from_str(&text)?;
            let children = serde_json::from_value(fields.remove("children").ok_or("missing child order")?)?;
            Self::from_fields(fields, children)
        }).transpose()
    }
}

pub(super) struct Delta {
    artifact_home: PathBuf,
    before: BTreeMap<String, Record>,
    after: BTreeMap<String, Record>,
    changed: BTreeSet<String>,
    added: BTreeSet<String>,
    pub(super) deleted: BTreeSet<String>,
    moved: BTreeSet<String>,
    pub(super) old_parents: BTreeMap<String, Option<String>>,
    new_parents: BTreeMap<String, Option<String>>,
}

fn git_bytes(home: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let result = crate::checked_publication::run_command(reader_command(home, args))?;
    require(result.status.success(), &format!("Git delta read failed: {}", String::from_utf8_lossy(&result.stderr)))?;
    Ok(result.stdout)
}
fn parent(connection: &Connection, id: &str) -> Result<Option<String>> {
    Ok(connection.query_row("SELECT source FROM child_links WHERE target=?1", [id], |r| r.get(0)).optional()?)
}

/// One native diff and one batched object reader, not a full historical checkout.
fn capture(connection: &Connection, home: &Path, proposal: &Proposal) -> Result<Delta> {
    let branch = reader_git(home, &["rev-parse", "--symbolic-full-name", "HEAD"])?;
    let git_dir = reader_git(home, &["rev-parse", "--absolute-git-dir"])?;
    let common = reader_git(home, &["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
    require(branch != "refs/heads/main" && Path::new(&git_dir) != Path::new(&common),
        "proposal deltas require a private linked worktree; main supports static rendering only")?;
    require(matches!(proposal.base.len(), 40 | 64) && proposal.base.bytes().all(|b| b.is_ascii_hexdigit()),
        "proposal.base must be the exact commit returned when the worktree was allocated")?;
    let base = reader_git(home, &["rev-parse", "--verify", &format!("{}^{{commit}}", proposal.base)])?;
    require(base == proposal.base, "proposal.base must be a full immutable commit ID")?;
    reader_git(home, &["merge-base", "--is-ancestor", &base, "HEAD"])?;
    let mut paths = git_bytes(home, &["diff", "--no-ext-diff", "--no-textconv", "--no-renames", "--name-only", "-z", &base, "--", "nodes", "trees"])?;
    paths.extend(git_bytes(home, &["ls-files", "--others", "--exclude-standard", "-z", "--", "nodes", "trees"])?);
    let mut ids = BTreeSet::new();
    for path in paths.split(|b| *b == 0).filter(|b| !b.is_empty()) {
        let path = std::str::from_utf8(path)?;
        if !crate::coherent_capture::authored_path(Path::new(path)) { continue }
        let parts: Vec<_> = path.split('/').collect();
        match parts.as_slice() {
            ["nodes", id, ..] if crate::uuid(id) => { ids.insert((*id).to_owned()); },
            ["trees", id] if crate::uuid(id) => { ids.insert((*id).to_owned()); },
            _ => (),
        }
        if let ["nodes", _, "children", child] = parts.as_slice() {
            if crate::uuid(child) { ids.insert((*child).to_owned()); }
        }
    }
    ids.extend(proposal.artifacts.iter().map(|a| a.owner.clone()).filter(|id| crate::uuid(id)));
    ids.extend(proposal.deletions.iter().map(|d| d.node.clone()).filter(|id| crate::uuid(id)));
    let wanted: Vec<_> = ids.iter().flat_map(|id| [format!("nodes/{id}/node.md"), format!("nodes/{id}/children.order"), format!("nodes/{id}/children")]).collect();
    let mut files = BTreeMap::new();
    let mut batch = Batch::new(home)?;
    for group in wanted.chunks(128) {
        let mut args = vec!["--literal-pathspecs", "ls-tree", "-r", "-l", "-z", &base, "--"];
        args.extend(group.iter().map(String::as_str));
        for row in git_bytes(home, &args)?.split(|b| *b == 0).filter(|b| !b.is_empty()) {
            let (header, path) = std::str::from_utf8(row)?.split_once('\t').ok_or("invalid Git tree entry")?;
            let words: Vec<_> = header.split_ascii_whitespace().collect();
            require(words.len() == 4 && ["100644", "120000"].contains(&words[0]) && words[1] == "blob", "invalid historical authored file")?;
            files.insert(path.to_owned(), batch.bytes(words[2], words[3].parse()?)?);
        }
    }
    let child_ids: BTreeSet<_> = files.keys().filter_map(|path| {
        let parts: Vec<_> = path.split('/').collect();
        match parts.as_slice() { ["nodes", _, "children", child] if crate::uuid(child) => Some((*child).to_owned()), _ => None }
    }).collect();
    let child_paths: Vec<_> = child_ids.iter().map(|id| format!("nodes/{id}/node.md")).collect();
    for group in child_paths.chunks(128) {
        let mut args = vec!["--literal-pathspecs", "ls-tree", "-l", "-z", &base, "--"];
        args.extend(group.iter().map(String::as_str));
        for row in git_bytes(home, &args)?.split(|b| *b == 0).filter(|b| !b.is_empty()) {
            let (header, path) = std::str::from_utf8(row)?.split_once('\t').ok_or("invalid Git tree entry")?;
            let words: Vec<_> = header.split_ascii_whitespace().collect();
            require(words.len() == 4 && words[0] == "100644", "invalid historical child")?;
            files.insert(path.to_owned(), batch.bytes(words[2], words[3].parse()?)?);
        }
    }
    let mut delta = Delta {
        artifact_home: Path::new(&common).parent().ok_or("Git common directory has no parent")?.to_owned(),
        before: BTreeMap::new(), after: BTreeMap::new(), changed: BTreeSet::new(),
        added: BTreeSet::new(), deleted: BTreeSet::new(), moved: BTreeSet::new(),
        old_parents: BTreeMap::new(), new_parents: BTreeMap::new(),
    };
    for id in &ids {
        if let Some(bytes) = files.get(&format!("nodes/{id}/node.md")) {
            let children: Vec<String> = if let Some(order) = files.get(&format!("nodes/{id}/children.order")) {
                std::str::from_utf8(order)?.lines().map(str::to_owned).collect()
            } else {
                let prefix = format!("nodes/{id}/children/");
                let members = files.keys().filter_map(|path| path.strip_prefix(&prefix)).filter(|child| crate::uuid(child)).map(str::to_owned);
                crate::ordering::sort(members, |child| {
                    let text = files.get(&format!("nodes/{child}/node.md")).ok_or("missing historical child")?;
                    crate::ordering::position(std::str::from_utf8(text)?)
                })?
            };
            require(children.iter().all(|c| crate::uuid(c)), "invalid historical child order")?;
            delta.before.insert(id.clone(), Record::from_markdown(bytes, children)?);
        }
        if let Some(record) = Record::current(connection, id)? { delta.after.insert(id.clone(), record); }
    }
    // Recover old parentage from changed edge owners. Unchanged edges remain in
    // the current index and need no historical parse or whole-tree scan.
    for (id, before) in &delta.before {
        for child in &before.node.children { delta.old_parents.insert(child.clone(), Some(id.clone())); }
    }
    for id in &ids {
        let new_parent = parent(connection, id)?;
        let old_parent = if delta.old_parents.contains_key(id) {
            delta.old_parents[id].clone()
        } else if let Some(p) = &new_parent {
            if delta.after.contains_key(p) && delta.before.get(p).is_none_or(|r| !r.node.children.contains(id)) {
                None
            } else { Some(p.clone()) }
        } else { None };
        delta.old_parents.insert(id.clone(), old_parent.clone());
        delta.new_parents.insert(id.clone(), new_parent.clone());
        match (delta.before.get(id), delta.after.get(id)) {
            (None, Some(_)) => { delta.added.insert(id.clone()); delta.changed.insert(id.clone()); },
            (Some(_), None) => { delta.deleted.insert(id.clone()); delta.changed.insert(id.clone()); },
            (Some(before), Some(after)) => {
                if before.fields != after.fields { delta.changed.insert(id.clone()); }
                if old_parent != new_parent { delta.moved.insert(id.clone()); delta.changed.insert(id.clone()); }
            },
            _ => (),
        }
    }
    // A parent gaining/losing/reordering children is context, not automatically
    // changed. Reordered surviving children carry [!] without an order detail.
    let mut reordered = BTreeSet::new();
    for (id, before) in &delta.before {
        let Some(after) = delta.after.get(id) else { continue };
        let common: BTreeSet<_> = before.node.children.iter().filter(|c| after.node.children.contains(c)).collect();
        let old: Vec<_> = before.node.children.iter().filter(|c| common.contains(c)).collect();
        let new: Vec<_> = after.node.children.iter().filter(|c| common.contains(c)).collect();
        for (a,b) in old.iter().zip(&new) {
            if a != b { reordered.insert((*a).clone()); reordered.insert((*b).clone()); }
        }
    }
    for id in reordered {
        if !delta.after.contains_key(&id) {
            if let Some(record) = Record::current(connection, &id)? {
                delta.before.insert(id.clone(), record.clone());
                delta.after.insert(id.clone(), record);
            }
        }
        delta.changed.insert(id);
    }
    for artifact in &proposal.artifacts {
        if delta.after.contains_key(&artifact.owner) || delta.before.contains_key(&artifact.owner) {
            delta.changed.insert(artifact.owner.clone());
        }
    }
    Ok(delta)
}

impl Delta {
    pub(super) fn deleted_node(&self, id: &str) -> Option<Node> {
        self.deleted.contains(id).then(|| self.before[id].node.clone()).map(|mut node| {
            // Moved-out survivors get leaving stubs, never duplicate live nodes.
            node.children.retain(|c| self.deleted.contains(c));
            node
        })
    }
    pub(super) fn include_deleted_children(&self, id: &str, children: &mut Vec<String>) {
        let Some(before) = self.before.get(id) else { return };
        for (position, child) in before.node.children.iter().enumerate() {
            if !self.deleted.contains(child) || children.contains(child) { continue }
            let next = before.node.children[position + 1..].iter().find_map(|next| children.iter().position(|c| c == next));
            children.insert(next.unwrap_or(children.len()), child.clone());
        }
    }
    fn old_parent(&self, connection: &Connection, id: &str) -> Result<Option<String>> {
        match self.old_parents.get(id) { Some(p) => Ok(p.clone()), None => parent(connection, id) }
    }
    fn old_ancestors(&self, connection: &Connection, id: &str) -> Result<Vec<String>> {
        let mut result = Vec::new();
        let mut seen = BTreeSet::from([id.to_owned()]);
        let mut cursor = id.to_owned();
        while let Some(p) = self.old_parent(connection, &cursor)? {
            require(seen.insert(p.clone()), "cycle in historical parentage")?;
            result.push(p.clone()); cursor = p;
        }
        Ok(result)
    }
    fn label(&self, connection: &Connection, id: &str, old: bool) -> Result<String> {
        let record = if old { self.before.get(id) } else { self.after.get(id) };
        if let Some(record) = record { return Ok(name(&record.node)) }
        if let Some(record) = Record::current(connection, id)? { return Ok(name(&record.node)) }
        if let Some(record) = self.before.get(id) { return Ok(name(&record.node)) }
        Err(format!("unknown reference or parent node: {id}").into())
    }
    fn deleted_count(&self, id: &str) -> usize {
        let mut pending = vec![id];
        let mut count = 0;
        while let Some(id) = pending.pop() {
            if !self.deleted.contains(id) { continue }
            count += 1;
            pending.extend(self.before[id].node.children.iter().map(String::as_str));
        }
        count
    }
}

fn check_packet(delta: &Delta, proposal: &Proposal) -> Vec<Value> {
    let mut errors = Vec::new();
    let mut locations: BTreeMap<(&str, &str), BTreeMap<Operation, &Artifact>> = BTreeMap::new();
    for (i, artifact) in proposal.artifacts.iter().enumerate() {
        let path = format!("proposal.artifacts[{i}]");
        if !delta.after.contains_key(&artifact.owner) && !delta.before.contains_key(&artifact.owner) {
            problem(&mut errors, format!("{path}.owner"), "artifact owner does not exist");
        }
        if (artifact.operation == Operation::Add && delta.deleted.contains(&artifact.owner))
            || (artifact.operation == Operation::Remove && delta.added.contains(&artifact.owner)) {
            problem(&mut errors, path.clone(), "artifact action contradicts the owner's creation/deletion");
        }
        let destination = Path::new(&artifact.destination);
        if !artifact.destination.starts_with("artifacts/") || destination.components().count() < 2
            || !destination.components().all(|c| matches!(c, Component::Normal(_)))
            || artifact.destination.chars().any(char::is_control) {
            problem(&mut errors, format!("{path}.destination"), "expected a node-relative artifacts/path without traversal or control characters");
        }
        if !Path::new(&artifact.target).is_absolute() || artifact.target.chars().any(char::is_control) {
            problem(&mut errors, format!("{path}.target"), "symlink target must be an absolute path without control characters");
        }
        if artifact.reason.trim().is_empty() {
            problem(&mut errors, format!("{path}.reason"), "artifact change needs its contribution/removal reason");
        }
        if locations.entry((&artifact.owner, &artifact.destination)).or_default().insert(artifact.operation, artifact).is_some() {
            problem(&mut errors, path, "duplicate or conflicting action at the same link location");
        }
    }
    for actions in locations.values() {
        if let (Some(add), Some(remove)) = (actions.get(&Operation::Add), actions.get(&Operation::Remove)) {
            if add.target == remove.target {
                problem(&mut errors, "proposal.artifacts".into(), "adding and removing the same link target is a no-op, not a delta");
            }
        }
    }
    let mut reasons = BTreeMap::new();
    for (i, deletion) in proposal.deletions.iter().enumerate() {
        if !delta.deleted.contains(&deletion.node) || deletion.reason.trim().is_empty() {
            problem(&mut errors, format!("proposal.deletions[{i}]"), "deletion reason must name an actually deleted node and be nonblank");
        }
        if reasons.insert(&deletion.node, &deletion.reason).is_some() {
            problem(&mut errors, format!("proposal.deletions[{i}]"), "duplicate deletion reason");
        }
    }
    for id in &delta.deleted {
        if delta.old_parents.get(id).and_then(|p| p.as_ref()).is_some_and(|p| delta.deleted.contains(p)) { continue }
        if !reasons.contains_key(id) {
            problem(&mut errors, "proposal.deletions".into(), format!("missing deletion reason for {}", name(&delta.before[id].node)));
        }
    }
    for (i, stage) in proposal.work_record.iter().enumerate() {
        if stage.title.trim().is_empty() || stage.title.chars().any(char::is_control)
            || stage.title.trim_start().starts_with('#') || stage.title.trim_start().starts_with(|c: char| c.is_ascii_digit()) {
            problem(&mut errors, format!("proposal.work_record[{i}].title"), "stage title must be nonblank and unnumbered; renderer assigns ### 1.");
        }
        if stage.body.trim().is_empty() {
            problem(&mut errors, format!("proposal.work_record[{i}].body"), "stage body must be nonblank");
        }
    }
    errors
}

fn selected(reader: &mut Reader<'_>, selection: &Selection, delta: &Delta, proposal: &Proposal) -> Result<View> {
    let mut view = select(reader, selection)?;
    let requested_routes = view.routes.clone();
    let mut roots: BTreeSet<_> = view.roots.iter().cloned().collect();
    for id in &delta.changed {
        let ancestors = reader.ancestors(id)?;
        let old = delta.old_ancestors(reader.connection, id)?;
        if id != &selection.start && !ancestors.contains(&selection.start) && !old.contains(&selection.start) { continue }
        // Selected deltas never silently suppress a changed node using context rules.
        view.routes.insert(id.clone());
        view.focus.insert(id.clone());
        view.routes.extend(ancestors.iter().cloned());
        roots.insert(ancestors.last().cloned().unwrap_or_else(|| id.clone()));
        // Keep bounded first-child context, but do not expand every deleted descendant.
        if !delta.deleted.contains(id) || !delta.old_parents.get(id).and_then(|p| p.as_ref()).is_some_and(|p| delta.deleted.contains(p)) {
            if let Some(child) = reader.node(id)?.children.first() {
                view.routes.insert(child.clone());
                view.focus.insert(child.clone());
            }
        }
        if delta.moved.contains(id) {
            // Both ends are loaded centres, including cross-root moves.
            view.routes.extend(old.iter().cloned());
            if let Some(root) = old.last() { roots.insert(root.clone()); }
        }
    }
    // Descendant removals are represented by their highest deletion centre and
    // one child, not thousands of individually expanded deletion centres.
    let explicit: BTreeSet<_> = selection.centres.iter()
        .chain(requested_routes.iter())
        .chain(selection.branches.iter().map(|b| &b.node))
        .chain(proposal.artifacts.iter().map(|a| &a.owner))
        .chain(proposal.deletions.iter().map(|d| &d.node))
        .cloned().collect();
    let mut prune = Vec::new();
    for id in &view.routes {
        if !delta.deleted.contains(id) || explicit.contains(id) || id == &selection.start { continue }
        let ancestors = delta.old_ancestors(reader.connection, id)?;
        let deletion_depth = ancestors.iter().take_while(|a| delta.deleted.contains(*a)).count();
        let parent_id = delta.old_parents.get(id).and_then(|p| p.as_ref());
        let is_first = parent_id.and_then(|p| delta.deleted_node(p)).is_some_and(|p| p.children.first() == Some(id));
        if deletion_depth > 1 || (deletion_depth == 1 && !is_first) { prune.push(id.clone()); }
    }
    for id in prune { view.routes.remove(&id); }
    // Restore any route required by an explicit centre or moved-out survivor.
    let retained: Vec<_> = view.routes.iter().cloned().collect();
    for id in retained { view.routes.extend(reader.ancestors(&id)?); }
    for id in &view.routes { reader.node(id)?; }
    view.roots = roots.into_iter().collect();
    name_ancestors(reader, &mut view, selection.ascend)?;
    // The leaving stub needs its actual old parent named even if that parent is
    // outside the destination selection. Other ancestors may stay compact.
    for id in delta.moved.iter().filter(|id| view.routes.contains(*id)) {
        if let Some(parent) = delta.old_parents.get(id).and_then(|p| p.as_ref()) {
            view.named.insert(parent.clone());
        }
    }
    Ok(view)
}

fn line(output: &mut String, prefix: &str, value: &str) {
    for text in value.lines() { output.push_str(&format!("{prefix}{text}\n")); }
}
fn references(record: Option<&Record>) -> Result<Vec<crate::Reference>> {
    record.map(|r| serde_json::from_value(r.fields.get("see_also").cloned().unwrap_or(json!([]))).map_err(Into::into))
        .unwrap_or_else(|| Ok(Vec::new()))
}
fn details(output: &mut String, prefix: &str, id: &str, reader: &Reader<'_>, delta: &Delta, proposal: &Proposal, anchors: &BTreeMap<String, String>) -> Result<()> {
    if delta.moved.contains(id) {
        let source = match delta.old_parents.get(id).and_then(|p| p.as_ref()) {
            Some(parent) => delta.label(reader.connection, parent, true)?, None => "tree root".into(),
        };
        line(output, prefix, &format!("Moving from: {source}"));
    }
    if let (Some(before), Some(after)) = (delta.before.get(id), delta.after.get(id)) {
        if before.node.status != after.node.status {
            let old = serde_json::to_value(before.node.status)?;
            let new = serde_json::to_value(after.node.status)?;
            line(output, prefix, &format!("Status: {} -> {}", old.as_str().unwrap(), new.as_str().unwrap()));
        }
    }
    if let Some(reason) = proposal.deletions.iter().find(|d| d.node == id) {
        line(output, prefix, &format!("Delete reason: {}", reason.reason));
    }
    if !delta.deleted.contains(id) {
        let before = references(delta.before.get(id))?;
        let after = references(delta.after.get(id))?;
        for reference in before.iter().filter(|r| !after.iter().any(|other| other.target == r.target)) {
            let target = delta.label(reader.connection, &reference.target, true)?;
            line(output, prefix, &format!("[-][←] Remove see_also -> {target}"));
            line(output, &format!("{prefix}    "), &reference.reason);
        }
        for reference in after.iter().filter(|r| !before.iter().any(|other| other.target == r.target)) {
            let target = delta.label(reader.connection, &reference.target, false)?;
            line(output, prefix, &format!("[→] Add see_also -> {target}"));
            line(output, &format!("{prefix}    "), &reference.reason);
        }
    }
    let artifacts: Vec<_> = proposal.artifacts.iter().filter(|a| a.owner == id).collect();
    if !artifacts.is_empty() {
        line(output, prefix, "artifacts/");
        for artifact in artifacts {
            let mark = if artifact.operation == Operation::Add { "+" } else { "-" };
            let destination = artifact.destination.strip_prefix("artifacts/").unwrap_or(&artifact.destination);
            let target = match internal_target(delta, &artifact.target) {
                Some((node, suffix)) if anchors.contains_key(&node) => {
                    if suffix.is_empty() { anchors[&node].clone() } else { format!("{}/{}", anchors[&node], suffix) }
                },
                _ => artifact.target.clone(),
            };
            line(output, prefix, &format!("{mark} {destination} → {target}"));
            line(output, &format!("{prefix}  "), &artifact.reason);
        }
    }
    Ok(())
}

#[derive(Clone)]
enum Entry { Node(String), Leaving(String), Omitted(usize) }
fn entries(reader: &Reader<'_>, view: &View, delta: &Delta, id: &str) -> Vec<Entry> {
    let mut children: Vec<_> = reader.nodes[id].children.iter().map(|id| Entry::Node(id.clone())).collect();
    if let Some(before) = delta.before.get(id) {
        for (i, child) in before.node.children.iter().enumerate() {
            if !delta.moved.contains(child) || !view.routes.contains(child) { continue }
            let position = before.node.children[i+1..].iter().find_map(|next| children.iter().position(|entry| matches!(entry, Entry::Node(id) if id == next)));
            children.insert(position.unwrap_or(children.len()), Entry::Leaving(child.clone()));
        }
    }
    let mut result = Vec::new();
    for entry in children {
        if let Entry::Node(child) = &entry {
            if !view.routes.contains(child) {
                let count = if delta.deleted.contains(id) { delta.deleted_count(child) } else { 0 };
                if let Some(Entry::Omitted(total)) = result.last_mut() { *total += count; }
                else { result.push(Entry::Omitted(count)); }
                continue;
            }
        }
        result.push(entry);
    }
    result
}

fn render_delta(reader: &Reader<'_>, view: &View, delta: &Delta, proposal: &Proposal, anchors: &BTreeMap<String, String>) -> Result<String> {
    let mut output = format!("[+] added, [!] revised, [-] removed, [→][!] moved here, [←] moved away\n{STATUS_NOTATION}\n… omitted context\n");
    if proposal.artifacts.is_empty() {
        output.push_str("Artifact link changes are never derived. None declared; declare intended additions/removals explicitly.\n");
    } else {
        output.push_str("artifacts/ = file links owned by the topic above it: + new link, - removed link, @1 = shorthand for a full path already shown once in this render\n");
    }
    output.push('\n');
    for (i, stage) in proposal.work_record.iter().enumerate() {
        output.push_str(&format!("### {}. {}\n{}\n\n", i+1, stage.title, stage.body));
    }
    // Alias only actual collisions; exact identities stay in the source packet.
    let mut names: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for id in &view.named { names.entry(name(&reader.nodes[id])).or_default().push(id.clone()); }
    let mut labels = BTreeMap::new();
    for (text, ids) in names {
        for (i, id) in ids.iter().enumerate() {
            let name = if ids.len() == 1 { text.clone() } else { format!("{} - {text}", label(i)) };
            labels.insert(id.clone(), match anchors.get(id) {
                Some(anchor) => format!("{anchor} - {name}"), None => name,
            });
        }
    }
    let mut pending: Vec<_> = view.roots.iter().rev().map(|id| (Entry::Node(id.clone()), String::new(), String::new(), String::new())).collect();
    for id in delta.moved.iter().rev().filter(|id| view.routes.contains(*id)) {
        if delta.old_parents.get(id).is_none_or(|p| p.is_none()) {
            pending.push((Entry::Leaving(id.clone()), String::new(), String::new(), String::new()));
        }
    }
    while let Some((entry, prefix, branch, continuation)) = pending.pop() {
        if branch.is_empty() && !output.is_empty() && !output.ends_with("\n\n") { output.push('\n'); }
        match entry {
            Entry::Omitted(count) => {
                let text = if count == 0 { "...".into() } else { format!("[-] {count} more descendants ...") };
                line(&mut output, &format!("{prefix}{branch}"), &text);
            },
            Entry::Leaving(id) => {
                let target = match delta.new_parents.get(&id).and_then(|p| p.as_ref()) {
                    Some(parent) => delta.label(reader.connection, parent, false)?, None => "tree root".into(),
                };
                line(&mut output, &format!("{prefix}{branch}"), &format!("[←] {} - moving under {target}", labels[&id]));
            },
            Entry::Node(id) => {
                if !view.named.contains(&id) {
                    line(&mut output, &format!("{prefix}{branch}"), "...");
                    let tail = compact_tail(reader, view, &id);
                    let children: Vec<_> = reader.nodes[&tail].children.iter().filter(|c| view.routes.contains(*c)).collect();
                    let count = children.len();
                    for (i, child) in children.into_iter().enumerate().rev() {
                        let last = i + 1 == count;
                        pending.push((Entry::Node(child.clone()), format!("{prefix}{continuation}"), if last { "└── " } else { "├── " }.into(), if last { "    " } else { "│   " }.into()));
                    }
                    continue;
                }
                let mark = if delta.added.contains(&id) { "[+]" }
                    else if delta.deleted.contains(&id) { "[-]" }
                    else if delta.moved.contains(&id) { "[→][!]" }
                    else if delta.changed.contains(&id) { "[!]" } else { "" };
                line(&mut output, &format!("{prefix}{branch}"), &format!("{mark}{} {}", status_mark(reader.nodes[&id].status), labels[&id]));
                let child_prefix = format!("{prefix}{continuation}");
                if delta.changed.contains(&id) { details(&mut output, &format!("{child_prefix}    "), &id, reader, delta, proposal, anchors)?; }
                let children = entries(reader, view, delta, &id);
                let count = children.len();
                for (i, child) in children.into_iter().enumerate().rev() {
                    let last = i+1 == count;
                    pending.push((child, child_prefix.clone(), if last { "└── " } else { "├── " }.into(), if last { "    " } else { "│   " }.into()));
                }
            },
        }
    }
    // Authored changes belong in their node proposals, never repeated as outline
    // field edits. Complete replacement bodies appear once, not as an inline diff.
    let mut heading = false;
    for id in view.named.iter().filter(|id| delta.changed.contains(*id)) {
        let Some(after) = delta.after.get(id) else { continue };
        let before = delta.before.get(id);
        if before.is_some_and(|before| before.fields == after.fields) { continue }
        if !heading { output.push_str("\n## Node proposals\n"); heading = true; }
        output.push_str(&format!("\n### {}\n", labels[id]));
        if let Some(before) = before {
            let keys: BTreeSet<_> = before.fields.keys().chain(after.fields.keys()).collect();
            for key in keys {
                if ["id", "content", "see_also", "status"].contains(&key.as_str()) { continue }
                let old = before.fields.get(key).unwrap_or(&Value::Null);
                let new = after.fields.get(key).unwrap_or(&Value::Null);
                if old != new { output.push_str(&format!("{key}: {old} -> {new}\n")); }
            }
        }
        if before.is_some() {
            let old_references = references(before)?;
            for reference in references(Some(after))? {
                if let Some(old) = old_references.iter().find(|old| old.target == reference.target) {
                    if old.reason != reference.reason {
                        let target = delta.label(reader.connection, &reference.target, false)?;
                        output.push_str(&format!("see_also reason -> {target}: {} -> {}\n", json!(old.reason), json!(reference.reason)));
                    }
                }
            }
        }
        for key in ["micro_summary", "scope"] {
            if let Some(text) = after.fields.get(key).and_then(Value::as_str) {
                output.push_str(&format!("{key}: {text}\n"));
            }
        }
        if before.is_none() {
            for (key, value) in &after.fields {
                if !["id", "name", "status", "content", "see_also", "micro_summary", "scope"].contains(&key.as_str()) {
                    output.push_str(&format!("{key}: {value}\n"));
                }
            }
        }
        output.push('\n');
        output.push_str(after.fields.get("content").and_then(Value::as_str).unwrap_or(""));
        if !output.ends_with('\n') { output.push('\n'); }
    }
    Ok(output)
}

/// Recognize only this knowledge home's node paths, lexically. Do not follow
/// artifact symlinks or mistake another home's UUID-looking path for our node.
fn internal_target(delta: &Delta, target: &str) -> Option<(String, String)> {
    let path = Path::new(target).strip_prefix(delta.artifact_home.join("nodes")).ok()?;
    let mut parts = path.components();
    let Component::Normal(node) = parts.next()? else { return None };
    let node = node.to_str()?;
    if !crate::uuid(node) { return None }
    let tail = parts.as_path();
    if !tail.components().all(|part| matches!(part, Component::Normal(_))) { return None }
    Some((node.to_owned(), tail.to_str()?.to_owned()))
}

/// Load internal artifact destinations as bounded centres in the same forest.
/// Only explicit artifact actions are traversed, never the target filesystem or
/// arbitrary node references. Process each displayed owner once, even in cycles.
fn reveal_targets(reader: &mut Reader<'_>, view: &mut View, request: &Request, delta: &Delta, proposal: &Proposal) -> Result<BTreeMap<String, String>> {
    let mut by_owner: BTreeMap<&str, Vec<&Artifact>> = BTreeMap::new();
    for artifact in &proposal.artifacts { by_owner.entry(&artifact.owner).or_default().push(artifact); }
    let mut queue: std::collections::VecDeque<_> = view.named.iter().cloned().collect();
    let mut visited = BTreeSet::new();
    let mut targets = BTreeSet::new();
    let full = request.trees.iter().any(|selection| selection.ascend == Ascend::Full);
    while let Some(owner) = queue.pop_front() {
        if !visited.insert(owner.clone()) { continue }
        let Some(artifacts) = by_owner.get(owner.as_str()) else { continue };
        for artifact in artifacts {
            let Some((id, _)) = internal_target(delta, &artifact.target) else { continue };
            if !targets.insert(id.clone()) { continue }
            let node = reader.node(&id)?;
            let ancestors = reader.ancestors(&id)?;
            let root = ancestors.last().cloned().unwrap_or_else(|| id.clone());
            let selection = request.trees.iter().find(|s| s.start == id || ancestors.contains(&s.start));
            let capped = request.trees.iter().any(|s| s.branches.iter().any(|b| b.node == id && b.depth == 0));
            view.focus.insert(id.clone());
            view.routes.insert(id.clone());
            view.routes.extend(ancestors.iter().cloned());
            let mut names = vec![id, root.clone()];
            if let Some(parent) = ancestors.first() { names.push(parent.clone()); }
            if full { names.extend(ancestors); }
            if !capped {
                if let Some(child) = node.children.first() {
                    if match selection { Some(s) => reader.allowed(s, child)?, None => true } {
                        view.focus.insert(child.clone());
                        view.routes.insert(child.clone());
                        names.push(child.clone());
                    }
                }
            }
            if !view.roots.contains(&root) { view.roots.push(root); }
            for name in names {
                if view.named.insert(name.clone()) { queue.push_back(name); }
            }
        }
    }
    for id in &view.routes { reader.node(id)?; }
    Ok(targets.into_iter().enumerate().map(|(i,id)| (id, format!("@{}", i+1))).collect())
}

/// Native allocation metadata, with the original branch-creation reflog as a
/// compatibility source for older drafts. Never substitute current main.
pub(super) fn allocation_base(home: &Path) -> Result<String> {
    let stored = reader_git(home, &["config", "--worktree", "--get", "sifttext.draftBase"]);
    let base = if let Ok(base) = stored { base } else {
        let branch = reader_git(home, &["symbolic-ref", "HEAD"])?;
        require(branch.starts_with("refs/heads/sifttext/"), "automatic mode requires a native private draft; use the allocation receipt for manual rendering")?;
        let history = reader_git(home, &["reflog", "show", "--format=%H %gs", &branch])?;
        let first = history.lines().last().ok_or("allocation base unavailable; use the original receipt in manual rendering")?;
        let (sha, message) = first.split_once(' ').ok_or("invalid allocation reflog")?;
        require(message == format!("branch: Created from {sha}"), "original allocation reflog unavailable; use the receipt in manual rendering")?;
        sha.to_owned()
    };
    require(matches!(base.len(), 40 | 64) && base.bytes().all(|b| b.is_ascii_hexdigit()), "invalid draft allocation base")?;
    reader_git(home, &["merge-base", "--is-ancestor", &base, "HEAD"])?;
    Ok(base)
}

pub(super) fn documents(connection: &Connection, home: &Path, request: &Request, proposal: &Proposal, version: &str, automatic: bool) -> Result<std::result::Result<Vec<(String, usize)>, Vec<Value>>> {
    let delta = capture(connection, home, proposal)?;
    let mut reader = Reader::new(connection);
    reader.delta = Some(&delta);
    let derived;
    let request = if automatic {
        let mut roots: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for id in &delta.changed {
            let ancestors = reader.ancestors(id)?;
            let root = ancestors.last().cloned().unwrap_or_else(|| id.clone());
            roots.entry(root).or_default().push(id.clone());
        }
        require(!roots.is_empty(), "no authored changes or declared artifact changes to render")?;
        derived = Request { trees: roots.into_iter().map(|(start, centres)| Selection {
            start, depth: 0, ascend: Ascend::Compact, centres, branches: Vec::new(),
            default: Action::Include, rules: Vec::new(),
        }).collect(), proposal: None };
        &derived
    } else { request };
    let mut errors = validate(&mut reader, request)?;
    errors.extend(check_packet(&delta, proposal));
    if !errors.is_empty() { return Ok(Err(errors)) }
    // A proposal is one forest: shared nodes, anchors, timeline and bodies appear
    // once. Independent static selections still produce separate output files.
    let mut forest = View { roots: Vec::new(), routes: BTreeSet::new(), focus: BTreeSet::new(), named: BTreeSet::new() };
    for selection in &request.trees {
        let view = selected(&mut reader, selection, &delta, proposal)?;
        for root in view.roots { if !forest.roots.contains(&root) { forest.roots.push(root); } }
        forest.routes.extend(view.routes);
        forest.focus.extend(view.focus);
        forest.named.extend(view.named);
    }
    let anchors = reveal_targets(&mut reader, &mut forest, request, &delta, proposal)?;
    let key = crate::sha256(&serde_json::to_vec(&(&request.trees, &proposal.base))?);
    let input = crate::sha256(&serde_json::to_vec(&(version, proposal))?);
    let document = crate::outlines::selected_view(connection, &key, &input, processing(), || render_delta(&reader, &forest, &delta, proposal, &anchors))?;
    Ok(Ok(vec![document]))
}
