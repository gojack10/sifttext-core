//! Unix-tree views over the native same-version index. No publication or model calls.
mod proposal;
use crate::{Policy, Result, require, status::Status, tree_selection::{Action, Rule, includes}};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{collections::{BTreeMap, BTreeSet}, io::Write, path::Path, sync::OnceLock};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub trees: Vec<Selection>,
    #[serde(default)]
    pub proposal: Option<proposal::Proposal>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Selection {
    #[serde(alias = "root")]
    pub start: String,
    #[serde(default = "default_depth")]
    pub depth: usize,
    #[serde(default)]
    pub ascend: Ascend,
    #[serde(default)]
    pub centres: Vec<String>,
    #[serde(default)]
    pub branches: Vec<Branch>,
    #[serde(default = "default_action")]
    pub default: Action,
    #[serde(default)]
    pub rules: Vec<Rule>,
}
#[derive(Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Ascend {
    #[default]
    Compact,
    Full,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Branch {
    pub node: String,
    pub depth: usize,
}

fn default_depth() -> usize { 1 }
fn default_action() -> Action { Action::Include }

#[derive(Clone, Serialize, PartialEq, Eq)]
struct Node {
    name: String,
    status: Status,
    children: Vec<String>,
}
struct Reader<'a> {
    connection: &'a Connection,
    nodes: BTreeMap<String, Node>,
    parents: BTreeMap<String, Option<String>>,
    delta: Option<&'a proposal::Delta>,
}
impl<'a> Reader<'a> {
    fn new(connection: &'a Connection) -> Self {
        Self { connection, nodes: BTreeMap::new(), parents: BTreeMap::new(), delta: None }
    }
    fn node(&mut self, id: &str) -> Result<Node> {
        if let Some(node) = self.nodes.get(id) { return Ok(node.clone()) }
        if let Some(delta) = self.delta {
            if let Some(node) = delta.deleted_node(id) {
                self.nodes.insert(id.to_owned(), node.clone());
                return Ok(node);
            }
        }
        let (name, status, children): (String, String, String) = self.connection.query_row(
            "SELECT name,status,children FROM nodes WHERE id=?1", [id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        ).optional()?.ok_or_else(|| format!("node not found: {id}"))?;
        let mut node = Node {
            name,
            status: serde_json::from_value(json!(status))?,
            children: serde_json::from_str(&children)?,
        };
        if let Some(delta) = self.delta { delta.include_deleted_children(id, &mut node.children); }
        self.nodes.insert(id.to_owned(), node.clone());
        Ok(node)
    }
    fn parent(&mut self, id: &str) -> Result<Option<String>> {
        if let Some(parent) = self.parents.get(id) { return Ok(parent.clone()) }
        let parent = if let Some(delta) = self.delta.filter(|d| d.deleted.contains(id)) {
            delta.old_parents.get(id).cloned().flatten()
        } else {
            self.connection.query_row(
                "SELECT source FROM child_links WHERE target=?1", [id], |row| row.get(0),
            ).optional()?
        };
        self.parents.insert(id.to_owned(), parent.clone());
        Ok(parent)
    }
    fn ancestors(&mut self, id: &str) -> Result<Vec<String>> {
        let mut result = Vec::new();
        let mut seen = BTreeSet::from([id.to_owned()]);
        let mut cursor = id.to_owned();
        while let Some(parent) = self.parent(&cursor)? {
            require(seen.insert(parent.clone()), "cycle in tree parentage")?;
            cursor = parent.clone();
            result.push(parent);
        }
        Ok(result)
    }
    fn allowed(&mut self, selection: &Selection, id: &str) -> Result<bool> {
        let ancestors = self.ancestors(id)?;
        Ok(includes(&selection.rules, selection.default, id, ancestors.iter().map(String::as_str)))
    }
}

fn problem(errors: &mut Vec<Value>, path: String, message: impl Into<String>) {
    errors.push(json!({"code":"invalid_selection", "path":path, "message":message.into()}));
}

fn validate(reader: &mut Reader<'_>, request: &Request) -> Result<Vec<Value>> {
    let mut errors = Vec::new();
    if request.trees.is_empty() {
        problem(&mut errors, "trees".into(), "select at least one tree");
    }
    for (i, selection) in request.trees.iter().enumerate() {
        let mut ids = vec![(format!("trees[{i}].start"), selection.start.as_str())];
        ids.extend(selection.centres.iter().enumerate().map(|(j, id)| (format!("trees[{i}].centres[{j}]"), id.as_str())));
        ids.extend(selection.rules.iter().enumerate().map(|(j, rule)| (format!("trees[{i}].rules[{j}].node"), rule.node.as_str())));
        ids.extend(selection.branches.iter().enumerate().map(|(j, branch)| (format!("trees[{i}].branches[{j}].node"), branch.node.as_str())));
        let mut valid = true;
        for (path, id) in ids {
            let exists: bool = reader.connection.query_row("SELECT EXISTS(SELECT 1 FROM nodes WHERE id=?1)", [id], |row| row.get(0))?;
            let deleted = reader.delta.is_some_and(|delta| delta.deleted.contains(id));
            if !crate::uuid(id) || !(exists || deleted) {
                problem(&mut errors, path, format!("unknown node: {id}"));
                valid = false;
            }
        }
        let mut rules = BTreeMap::new();
        for (j, rule) in selection.rules.iter().enumerate() {
            if rules.insert(&rule.node, rule).is_some_and(|old| old != rule) {
                problem(&mut errors, format!("trees[{i}].rules[{j}]"), "conflicting rules for one node");
                valid = false;
            }
        }
        let mut branches = BTreeMap::new();
        for (j, branch) in selection.branches.iter().enumerate() {
            if branches.insert(&branch.node, branch.depth).is_some_and(|old| old != branch.depth) {
                problem(&mut errors, format!("trees[{i}].branches[{j}]"), "conflicting depths for one branch");
                valid = false;
            }
        }
        if !valid { continue }
        for (j, branch) in selection.branches.iter().enumerate() {
            if branch.node != selection.start && !reader.ancestors(&branch.node)?.contains(&selection.start) {
                problem(&mut errors, format!("trees[{i}].branches[{j}].node"), "branch is outside the selected subtree");
            }
            if !reader.allowed(selection, &branch.node)? {
                problem(&mut errors, format!("trees[{i}].branches[{j}].node"), "branch is excluded by selection rules");
            }
        }
        for (j, id) in selection.centres.iter().enumerate() {
            let ancestors = reader.ancestors(id)?;
            if id != &selection.start && !ancestors.contains(&selection.start) {
                problem(&mut errors, format!("trees[{i}].centres[{j}]"), "centre is outside the selected subtree");
            }
            if !reader.allowed(selection, id)? {
                problem(&mut errors, format!("trees[{i}].centres[{j}]"), "centre is excluded by selection rules");
            }
        }
        if !reader.allowed(selection, &selection.start)? {
            problem(&mut errors, format!("trees[{i}].start"), "selected start is excluded by selection rules");
        }
    }
    Ok(errors)
}

/// Only explicit centres expand context. Context supplied by code never seeds more context.
fn neighbourhood(reader: &mut Reader<'_>, selection: &Selection, visible: &mut BTreeSet<String>, id: &str) -> Result<()> {
    if !reader.allowed(selection, id)? { return Ok(()) }
    visible.insert(id.to_owned());
    if let Some(child) = reader.node(id)?.children.first() {
        if reader.allowed(selection, child)? { visible.insert(child.clone()); }
    }
    // Ancestor paths are supplied by select; unselected siblings stay omitted.
    Ok(())
}

struct View {
    roots: Vec<String>,
    routes: BTreeSet<String>,
    // Direct selection/preview nodes, before ancestor context is added.
    focus: BTreeSet<String>,
    named: BTreeSet<String>,
}

fn name_ancestors(reader: &mut Reader<'_>, view: &mut View, ascend: Ascend) -> Result<()> {
    view.focus.retain(|id| view.routes.contains(id));
    view.named = if ascend == Ascend::Full { view.routes.clone() } else { view.focus.clone() };
    view.named.extend(view.roots.iter().cloned());
    if ascend == Ascend::Compact {
        for id in &view.focus {
            if let Some(parent) = reader.parent(id)? {
                if view.routes.contains(&parent) { view.named.insert(parent); }
            }
        }
    }
    Ok(())
}

/// Hide an unselected single-path ancestor chain behind one ellipsis. Branch
/// points retain their hierarchy; no omitted node is presented as a real parent.
fn compact_tail(reader: &Reader<'_>, view: &View, start: &str) -> String {
    let mut id = start.to_owned();
    while !view.named.contains(&id) {
        let children: Vec<_> = reader.nodes[&id].children.iter().filter(|c| view.routes.contains(*c)).collect();
        if children.len() != 1 || view.named.contains(children[0]) { break }
        id = children[0].clone();
    }
    id
}
fn select(reader: &mut Reader<'_>, selection: &Selection) -> Result<View> {
    let mut visible = BTreeSet::new();
    let depths: BTreeMap<_, _> = selection.branches.iter().map(|b| (b.node.as_str(), b.depth)).collect();
    let mut pending = vec![(selection.start.clone(), selection.depth)];
    pending.extend(selection.branches.iter().map(|b| (b.node.clone(), b.depth)));
    let mut visited = BTreeMap::new();
    while let Some((id, remaining)) = pending.pop() {
        // A specific branch replaces its inherited budget; it can expand or cap it.
        let remaining = depths.get(id.as_str()).copied().unwrap_or(remaining);
        if visited.get(&id).is_some_and(|previous| *previous >= remaining) { continue }
        visited.insert(id.clone(), remaining);
        if reader.allowed(selection, &id)? { visible.insert(id.clone()); }
        if remaining == 0 { continue }
        // Prune recursively excluded subtrees rather than walking them just to hide
        // their output. Explicit deeper includes/branches are independent seeds.
        let ancestors = reader.ancestors(&id)?;
        let inherited = std::iter::once(id.as_str()).chain(ancestors.iter().map(String::as_str))
            .find_map(|ancestor| selection.rules.iter().find(|r| r.node == ancestor && r.recurse))
            .map_or(selection.default, |rule| rule.action);
        if inherited == Action::Exclude { continue }
        for child in reader.node(&id)?.children {
            pending.push((child, remaining - 1));
        }
    }
    // Explicit deeper include rules are centres as well, without scanning excluded subtrees.
    let mut centres: BTreeSet<_> = selection.centres.iter().cloned().collect();
    for rule in &selection.rules {
        if rule.action == Action::Include {
            let ancestors = reader.ancestors(&rule.node)?;
            if rule.node == selection.start || ancestors.contains(&selection.start) {
                centres.insert(rule.node.clone());
            }
        }
    }
    for centre in &centres {
        // A branch's explicit depth (including zero) supersedes the one-child preview.
        if !depths.contains_key(centre.as_str()) {
            neighbourhood(reader, selection, &mut visible, centre)?;
        }
    }
    let ancestors = reader.ancestors(&selection.start)?;
    let root = ancestors.last().cloned().unwrap_or_else(|| selection.start.clone());
    visible.insert(root.clone());
    let mut routes = visible.clone();
    for id in &visible { routes.extend(reader.ancestors(id)?); }
    // Load only node labels and ordered edges used by this view. Never load bodies.
    for id in &routes { reader.node(id)?; }
    let mut view = View { roots: vec![root], routes, focus: visible, named: BTreeSet::new() };
    name_ancestors(reader, &mut view, selection.ascend)?;
    Ok(view)
}

fn status_mark(status: Status) -> &'static str {
    match status {
        Status::NotExamined => "[ ]",
        Status::InProgress => "[~]",
        Status::Resolved => "[✓]",
        Status::Stuck => "[/]",
        Status::Deferred => "[=]",
        Status::Discarded => "[x]",
    }
}
fn label(index: usize) -> String {
    let mut n = index + 1;
    let mut text = Vec::new();
    while n > 0 { n -= 1; text.push((b'A' + (n % 26) as u8) as char); n /= 26; }
    text.into_iter().rev().collect()
}
fn name(node: &Node) -> String {
    // A name is one tree line, never terminal control sequences or a hidden extra row.
    node.name.chars().map(|c| if c.is_control() { ' ' } else { c }).collect::<String>()
        .replace(['—', '–'], "-")
}

const STATUS_NOTATION: &str = "[ ] not examined, [~] in progress, [✓] resolved, [/] blocked, [=] deferred, [x] discarded";

fn render(reader: &Reader<'_>, view: &View) -> Result<String> {
    let mut names: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for id in &view.named { names.entry(name(&reader.nodes[id])).or_default().push(id.clone()); }
    let mut aliases = BTreeMap::new();
    for ids in names.values().filter(|ids| ids.len() > 1) {
        for (i, id) in ids.iter().enumerate() { aliases.insert(id.clone(), label(i)); }
    }
    let mut output = format!("{STATUS_NOTATION}\n\n");
    // Iterative traversal avoids stack growth on a very deep authored tree.
    let mut pending: Vec<_> = view.roots.iter().rev().map(|root|
        (Some(root.clone()), String::new(), String::new(), String::new())
    ).collect();
    while let Some((id, prefix, branch, continuation)) = pending.pop() {
        let Some(id) = id else {
            output.push_str(&format!("{prefix}{branch}...\n"));
            continue;
        };
        let named = view.named.contains(&id);
        let id = if named {
            let node = &reader.nodes[&id];
            let alias = aliases.get(&id).map(|a| format!("{a} - ")).unwrap_or_default();
            output.push_str(&format!("{prefix}{branch}{} {alias}{}\n", status_mark(node.status), name(node)));
            id
        } else {
            output.push_str(&format!("{prefix}{branch}...\n"));
            compact_tail(reader, view, &id)
        };
        let mut entries = Vec::new();
        for child in &reader.nodes[&id].children {
            if view.routes.contains(child) { entries.push(Some(child.clone())); }
            else if named && !matches!(entries.last(), Some(None)) { entries.push(None); }
        }
        let child_prefix = format!("{prefix}{continuation}");
        let length = entries.len();
        for (i, child) in entries.into_iter().enumerate().rev() {
            let last = i + 1 == length;
            pending.push((child, child_prefix.clone(), if last { "└── " } else { "├── " }.into(), if last { "    " } else { "│   " }.into()));
        }
    }
    Ok(output)
}

fn processing() -> &'static str {
    static ID: OnceLock<String> = OnceLock::new();
    ID.get_or_init(|| crate::sha256(concat!(
        include_str!("tree_render.rs"), include_str!("tree_render/proposal.rs"), include_str!("tree_selection.rs"),
        include_str!("token_count.rs"), include_str!("../Cargo.lock"),
    ).as_bytes()))
}

/// Render all requested static views at one accepted index version, then create fresh files.
/// Temp files are not persisted until every selection has validated and rendered successfully.
pub fn run(policy: &Policy, home: &Path, request: &Request) -> Result<Value> {
    run_mode(policy, home, request, false)
}

/// Derive selection and allocation base; ignored artifact links remain declared.
pub fn automatic(policy: &Policy, home: &Path, declarations: Option<&Path>) -> Result<Value> {
    let mut proposal: proposal::Proposal = match declarations {
        Some(path) => serde_json::from_slice(&crate::read_regular(path)?)?,
        None => serde_json::from_value(json!({"artifacts":[]}))?,
    };
    let base = proposal::allocation_base(home)?;
    require(proposal.base.is_empty() || proposal.base == base, "automatic proposal base differs from draft allocation")?;
    proposal.base = base;
    run_mode(policy, home, &Request { trees: Vec::new(), proposal: Some(proposal) }, true)
}

fn run_mode(policy: &Policy, home: &Path, request: &Request, automatic: bool) -> Result<Value> {
    let (_, result) = crate::index::with_current(policy, home, |tx, version| {
        if let Some(proposal) = &request.proposal {
            return proposal::documents(tx, home, request, proposal, version, automatic);
        }
        let mut reader = Reader::new(tx);
        let errors = validate(&mut reader, request)?;
        if !errors.is_empty() { return Ok(Err(errors)) }
        let mut documents = Vec::new();
        for selection in &request.trees {
            let mut reader = Reader::new(tx);
            let view = select(&mut reader, selection)?;
            let key = crate::sha256(&serde_json::to_vec(selection)?);
            let input = crate::sha256(&serde_json::to_vec(&(&reader.nodes, &reader.parents))?);
            documents.push(crate::outlines::selected_view(tx, &key, &input, processing(), || render(&reader, &view))?);
        }
        Ok(Ok(documents))
    })?;
    let documents = match result {
        Ok(documents) => documents,
        Err(errors) => return Ok(json!({"status":"rejected", "errors":errors})),
    };
    let mut files = Vec::new();
    for (text, tokens) in documents {
        let mut file = tempfile::Builder::new().prefix("sifttext-tree-render-").suffix(".md").tempfile_in("/tmp")?;
        file.write_all(text.as_bytes())?;
        file.flush()?;
        files.push((file, tokens));
    }
    let outputs: Vec<_> = files.iter().map(|(file, tokens)| json!({"output":file.path(), "tokens":tokens})).collect();
    let mut retained = Vec::new();
    for (file, _) in files {
        match file.keep() {
            Ok((_, path)) => retained.push(path),
            Err(error) => {
                for path in retained { let _ = std::fs::remove_file(path); }
                return Err(error.error.into());
            }
        }
    }
    Ok(json!({"status":"render-success", "encoding":"o200k_base", "outputs":outputs}))
}
