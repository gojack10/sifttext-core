// Pure graph-to-outline-row projection: authored preorder, coverage membership, counts, guardrails.
//
// One complete logical forest in, one metadata-plus-rows document out. No Markdown parsing, no
// SQLite hydration, no file access, no LEAN encoding and no token measurement. The later integrated
// outline operation feeds these rows to the accepted renderer and adds its own generated-view costs;
// this contribution stops exactly where a row stops being a scalar navigation record.
//
// Hierarchy comes only from authored-order `children`. Input array order is never traversal order,
// and no parent pointer is repeated in a row. Authored fields a row must not carry (scope,
// crystallization except as a resolved guard, timestamps, parents, backlinks, and any `ruled_out`
// text, which the target Core does not have) are ignored here and never emitted.
//
// Library and standalone observers share this projection. Artifact observers use the prepared
// forest directly; the row observer retains the one-request entry point.

use serde::{Deserialize, Serialize};
use sifttext_file_graph::{status::Status, uuid};
use std::collections::{HashMap, HashSet};

pub const PATH_TEMPLATE: &str = "nodes/{id}/node.md";
pub const TOKEN_ENCODING: &str = "o200k_base";

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Coverage {
    Shallow,
    Full,
}

/// One complete logical node record. Only the fields a row can carry are typed; every other
/// authored field is ignored, so a real hydrated record needs no stripping before projection.
#[derive(Deserialize)]
pub struct Node {
    pub id: String,
    pub name: String,
    pub status: Status,
    pub micro_summary: String,
    pub children: Vec<String>,
    #[serde(default)]
    pub warnings: Option<String>,
    #[serde(default)]
    pub blocker: Option<String>,
    #[serde(default)]
    pub deferred_reason: Option<String>,
    #[serde(default)]
    pub discard_reason: Option<String>,
    #[serde(default)]
    pub crystallization: Option<String>,
}

#[allow(dead_code)] // artifact observers use Forest directly; row observer uses Request/project
#[derive(Deserialize)]
pub struct Request {
    pub owner: String,
    pub coverage: Coverage,
    pub source_version: String,
    pub nodes: Vec<Node>,
}

#[derive(Serialize)]
pub struct Response<'a> {
    pub coverage: Coverage,
    pub source_version: &'a str,
    pub source_version_scope: &'static str,
    pub path_template: &'a str,
    pub token_encoding: &'a str,
    pub rows: Vec<Row<'a>>,
}

#[derive(Serialize)]
pub struct Row<'a> {
    pub depth: usize,
    pub id: &'a str,
    pub name: &'a str,
    pub status: Status,
    pub children: usize,
    pub descendants: usize,
    pub micro_summary: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warnings: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocker: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deferred_reason: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discard_reason: Option<&'a str>,
}

#[allow(dead_code)] // artifact observers use Forest directly; row observer uses this entry point
pub fn project(request: &Request) -> Result<Response<'_>, String> {
    if request.source_version.trim().is_empty() {
        return Err("a request needs a nonblank supplied source_version".to_owned());
    }
    Forest::new(&request.nodes)?.project(&request.owner, request.coverage, &request.source_version)
}

/// Identity, ownership and authored-text guards over the complete input forest, including nodes
/// this owner never displays. Nothing here is repaired: an invalid store is rejected.
fn index<'a>(nodes: &'a [Node]) -> Result<Forest<'a>, String> {
    let mut forest = Forest {
        nodes: HashMap::new(),
        parents: HashMap::new(),
        preorder: Vec::with_capacity(nodes.len()),
        positions: HashMap::with_capacity(nodes.len()),
        descendants: HashMap::with_capacity(nodes.len()),
    };
    for node in nodes {
        if !uuid(&node.id) {
            return Err(format!("malformed node id {:?}", node.id));
        }
        if forest.nodes.insert(node.id.as_str(), node).is_some() {
            return Err(format!("duplicate node id {}", node.id));
        }
    }
    for node in nodes {
        validate(node)?;
        let mut listed = HashSet::new();
        for child in &node.children {
            if !listed.insert(child.as_str()) {
                return Err(format!("node {} lists child {child} twice", node.id));
            }
            if !uuid(child) {
                return Err(format!(
                    "node {} has a malformed child id {child:?}",
                    node.id
                ));
            }
            if !forest.nodes.contains_key(child.as_str()) {
                return Err(format!("node {} names missing child {child}", node.id));
            }
            if forest
                .parents
                .insert(child.as_str(), node.id.as_str())
                .is_some()
            {
                return Err(format!("child {child} has more than one parent"));
            }
        }
    }
    Ok(forest)
}

impl<'a> Forest<'a> {
    /// Validate the complete forest once. Root walks are iterative and every subtree occupies
    /// one contiguous preorder range. A node not reached from a root belongs to a cycle because
    /// all references resolve and each child has at most one parent.
    pub(crate) fn new(nodes: &'a [Node]) -> Result<Self, String> {
        let mut forest = index(nodes)?;
        let mut pending = Vec::new();
        for node in nodes {
            if forest.parents.contains_key(node.id.as_str()) {
                continue;
            }
            pending.push((node, 0));
            while let Some((node, depth)) = pending.pop() {
                forest
                    .positions
                    .insert(node.id.as_str(), forest.preorder.len());
                forest.preorder.push((node, depth));
                pending.extend(
                    node.children
                        .iter()
                        .rev()
                        .map(|id| (forest.nodes[id.as_str()], depth + 1)),
                );
            }
        }
        if forest.preorder.len() != nodes.len() {
            return Err("cycle in supplied forest".to_owned());
        }
        for (node, _) in forest.preorder.iter().rev() {
            let total = node
                .children
                .iter()
                .map(|id| 1 + forest.descendants[id.as_str()])
                .sum();
            forest.descendants.insert(node.id.as_str(), total);
        }
        Ok(forest)
    }

    /// A subtree-relative fingerprint over exactly the displayed rows, authored order and
    /// processing identity. Moving a subtree does not change its own relative-depth view.
    #[allow(dead_code)] // native publication uses fingerprints; pure supplied-version observers do not
    pub(crate) fn full_identities(
        &self,
        processing: &str,
    ) -> Result<HashMap<&'a str, String>, String> {
        let mut identities = HashMap::with_capacity(self.nodes.len());
        for (node, _) in self.preorder.iter().rev() {
            let children: Vec<_> = node
                .children
                .iter()
                .map(|id| &identities[id.as_str()])
                .collect();
            let input =
                serde_json::to_vec(&(processing, row(node, 0, &self.descendants), children))
                    .map_err(|e| e.to_string())?;
            identities.insert(node.id.as_str(), sifttext_file_graph::sha256(&input));
        }
        Ok(identities)
    }

    pub(crate) fn project(
        &self,
        owner: &str,
        coverage: Coverage,
        source_version: &'a str,
    ) -> Result<Response<'a>, String> {
        let node = self
            .nodes
            .get(owner)
            .ok_or_else(|| format!("owner {owner} is not a supplied node"))?;
        let start = self.positions[owner];
        let base_depth = self.preorder[start].1;
        let rows = match coverage {
            Coverage::Full => self.preorder[start..=start + self.descendants[owner]]
                .iter()
                .map(|(node, depth)| row(node, depth - base_depth, &self.descendants))
                .collect(),
            Coverage::Shallow => std::iter::once(row(node, 0, &self.descendants))
                .chain(
                    node.children
                        .iter()
                        .map(|id| row(self.nodes[id.as_str()], 1, &self.descendants)),
                )
                .collect(),
        };
        Ok(Response {
            coverage,
            source_version,
            source_version_scope: "outline_view",
            path_template: PATH_TEMPLATE,
            token_encoding: TOKEN_ENCODING,
            rows,
        })
    }
}

fn validate(node: &Node) -> Result<(), String> {
    let id = &node.id;
    if node.micro_summary.trim().is_empty() {
        return Err(format!("node {id} has a blank authored summary"));
    }
    if let Some(key) = required_reason(node.status)
        && !text_is_present(reason_field(node, key))
    {
        let status = node.status;
        return Err(format!(
            "node {id} in status {status:?} needs a nonblank {key}"
        ));
    }
    // A resolved node must carry its own answer, even when this view never shows the body.
    if node.status == Status::Resolved && !text_is_present(node.crystallization.as_deref()) {
        return Err(format!("node {id} is resolved with blank crystallization"));
    }
    Ok(())
}

fn row<'a>(node: &'a Node, depth: usize, descendants: &HashMap<&'a str, usize>) -> Row<'a> {
    Row {
        depth,
        id: &node.id,
        name: &node.name,
        status: node.status,
        children: node.children.len(),
        descendants: descendants[node.id.as_str()],
        micro_summary: &node.micro_summary,
        warnings: nonempty(node.warnings.as_deref()),
        blocker: active_reason(node, "blocker"),
        deferred_reason: active_reason(node, "deferred_reason"),
        discard_reason: active_reason(node, "discard_reason"),
    }
}

pub(crate) struct Forest<'a> {
    nodes: HashMap<&'a str, &'a Node>,
    parents: HashMap<&'a str, &'a str>,
    preorder: Vec<(&'a Node, usize)>,
    positions: HashMap<&'a str, usize>,
    descendants: HashMap<&'a str, usize>,
}

/// The active-reason key an accepted status requires. Mirrors the authored status model in
/// `src/status.rs`, whose own mapping is private to that crate module.
fn required_reason(status: Status) -> Option<&'static str> {
    match status {
        Status::Stuck => Some("blocker"),
        Status::Deferred => Some("deferred_reason"),
        Status::Discarded => Some("discard_reason"),
        Status::NotExamined | Status::InProgress | Status::Resolved => None,
    }
}

fn reason_field<'a>(node: &'a Node, key: &str) -> Option<&'a str> {
    let value = match key {
        "blocker" => &node.blocker,
        "deferred_reason" => &node.deferred_reason,
        "discard_reason" => &node.discard_reason,
        _ => return None,
    };
    value.as_deref()
}

/// Active reasons only: a reason key that the node's own status does not carry is never emitted,
/// even when the record still holds text for it.
fn active_reason<'a>(node: &'a Node, key: &str) -> Option<&'a str> {
    if required_reason(node.status) != Some(key) {
        return None;
    }
    reason_field(node, key)
}

fn text_is_present(text: Option<&str>) -> bool {
    text.is_some_and(|value| !value.trim().is_empty())
}

/// An empty optional authored field is absent from a row; a nonempty one is preserved exactly.
fn nonempty(text: Option<&str>) -> Option<&str> {
    text.filter(|value| !value.is_empty())
}
