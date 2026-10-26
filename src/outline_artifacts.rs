// Pure generated-outline artifact assembly: accepted rows plus the accepted LEAN renderer become
// complete shallow/full bytes and non-self-referential cost metadata.
//
// One complete logical forest in, both generated views out with the costs measured from them, for
// one owner (`assemble`) or for every owner the same forest supplies (`assemble_forest`). No
// Markdown parsing, no SQLite hydration, no file or cache access, no publication, no stale-source
// verification and no freshness claim: this contribution stops exactly at the bytes a later
// same-version outline refresh has to write, and at the costs a reader needs to choose a read.
//
// `outline_rows::project` supplies membership, authored preorder, counts and complete-forest
// validation, and `lean_render::render` supplies every encoded byte. That renderer writes the
// `rows` block after the scalar metadata, so each top-level key is rendered on its own here to keep
// the canonical lexicographic key order the measured boundary is defined on.
//
// `src/lib.rs` registers this module for the library, and each standalone observer still includes
// it by path, as the other successor observers do. Each observer uses only one entry point, so the
// other is dead code inside that binary: the two allowances below keep the silence to exactly the
// forest request and its operation, not a module-wide or project-wide suppression.

use crate::{
    lean_render::{cell, write_field_into},
    outline_rows::{self, Coverage, TOKEN_ENCODING},
};
use serde::Deserialize;
use serde_json::{Map, Value, json};
use std::{collections::HashMap, fmt::Write};

/// One complete logical forest plus the version its generated views are measured at.
#[allow(dead_code)] // forest observer uses ForestRequest/assemble_forest, not the single-owner API
#[derive(Deserialize)]
pub struct Request {
    pub owner: String,
    pub source_version: String,
    // The accepted projection owns Node typing and complete-forest validation.
    pub nodes: Vec<Value>,
}

/// `{full:{text,meta},shallow:{text,meta}}` for one owner. Full bytes are computed first, so no
/// counted cost ever depends on the shallow bytes that report it.
#[allow(dead_code)] // forest observer uses batch assembly, not the single-owner entry point
pub fn assemble(request: &Request) -> Result<Value, String> {
    let nodes = typed(&request.nodes)?;
    let forest = outline_rows::Forest::new(&nodes)?;
    let full = full_artifact(&forest, &request.owner, &request.source_version)?;
    let mut costs = HashMap::new();
    costs.insert(
        request.owner.as_str(),
        full["meta"]["tokens"].as_u64().ok_or("missing cost")? as usize,
    );
    // One-owner callers need only their owner and direct-child complete-view costs.
    let shallow = document(
        &forest,
        &request.owner,
        Coverage::Shallow,
        &request.source_version,
    )?;
    let owner = nodes
        .iter()
        .find(|node| node.id == request.owner)
        .ok_or("missing owner")?;
    for id in &owner.children {
        let id = id.as_str();
        let artifact = full_artifact(&forest, id, &request.source_version)?;
        costs.insert(
            id,
            artifact["meta"]["tokens"].as_u64().ok_or("missing cost")? as usize,
        );
    }
    bundle(shallow, full, &request.owner, &costs)
}

fn bundle(
    mut shallow: Value,
    full: Value,
    owner: &str,
    costs: &HashMap<&str, usize>,
) -> Result<Value, String> {
    let full_tokens = costs[owner];
    let mut branch_full_tokens = Map::new();
    let Value::Object(fields) = &mut shallow else {
        return Err("projected outline document must be a JSON object".to_owned());
    };
    let Some(Value::Array(items)) = fields.get_mut("rows") else {
        return Err("projected outline document lost its rows array".to_owned());
    };
    for item in items {
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .ok_or("projected outline row lost its id")?
            .to_owned();
        let tokens = costs[id.as_str()];
        if id != owner {
            branch_full_tokens.insert(id.clone(), json!(tokens));
        }
        let Value::Object(row) = item else {
            return Err("each projected outline row must be a JSON object".to_owned());
        };
        row.insert("full_view_tokens".to_owned(), json!(tokens));
    }
    let shallow_text = bytes(&shallow)?;
    let (shallow_hash, shallow_tokens) = cost(&shallow_text);

    Ok(json!({
        "full": full,
        "shallow": {
            "text": shallow_text,
            "meta": {
                "hash": shallow_hash,
                "tokens": shallow_tokens,
                "encoding": TOKEN_ENCODING,
                "full_view_tokens": full_tokens,
                "branch_full_tokens": branch_full_tokens,
            },
        },
    }))
}

/// One complete logical forest and the single version every bundle in it is measured at.
///
/// There is no `owner` here on purpose: the forest supplies every owner itself, and a batch request
/// that named one would leave the reader to guess what happened to the rest. Unknown top-level keys
/// are rejected rather than ignored, because an ignored `coverage` or `owner` would silently change
/// what the later writer publishes.
#[allow(dead_code)] // used by `src/bin/outline-forest-observe.rs`, not by the one-owner observer
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForestRequest {
    pub source_version: String,
    // Raw logical records for the same reason as `Request::nodes`: the accepted projection owns
    // Node typing and complete-forest validation.
    pub nodes: Vec<Value>,
}

/// `{source_version, owners}` with exactly one bundle per supplied node, leaves included, so a later
/// refresh cannot miss a branch and every leaf full-view cost is reproducible from the same forest.
///
/// Each bundle is identical to the one-owner `assemble` result. Shared whole-forest validation
/// precedes all rendering, so invalid off-owner records never produce a partial artifact set.
#[allow(dead_code)] // used by `src/bin/outline-forest-observe.rs`, not by the one-owner observer
pub fn assemble_forest(request: &ForestRequest) -> Result<Value, String> {
    if request.source_version.trim().is_empty() {
        return Err("a forest request needs a nonblank supplied source_version".to_owned());
    }
    let mut owners = Map::new();
    visit_forest(request, |id, bundle| {
        owners.insert(id.to_owned(), bundle);
        Ok(())
    })?;
    Ok(json!({"source_version": request.source_version, "owners": owners}))
}

/// Share one validated typed forest and compute each owner's full bytes/cost exactly once.
/// Native staging consumes bundles instead of retaining another complete forest JSON document.
#[allow(dead_code)] // standalone one-owner observer does not use batch assembly
pub(crate) fn visit_forest(
    request: &ForestRequest,
    mut emit: impl FnMut(&str, Value) -> Result<(), String>,
) -> Result<(), String> {
    if request.source_version.trim().is_empty() {
        return Err("a forest request needs a nonblank supplied source_version".to_owned());
    }
    let nodes = typed(&request.nodes)?;
    let forest = outline_rows::Forest::new(&nodes)?;
    let mut full_views = HashMap::new();
    let mut costs = HashMap::new();
    for node in &nodes {
        let artifact = full_artifact(&forest, &node.id, &request.source_version)?;
        costs.insert(
            node.id.as_str(),
            artifact["meta"]["tokens"].as_u64().ok_or("missing cost")? as usize,
        );
        full_views.insert(node.id.as_str(), artifact);
    }
    for node in &nodes {
        let shallow = document(
            &forest,
            &node.id,
            Coverage::Shallow,
            &request.source_version,
        )?;
        let full = full_views
            .remove(node.id.as_str())
            .ok_or("missing full view")?;
        emit(&node.id, bundle(shallow, full, &node.id, &costs)?)?;
    }
    Ok(())
}

fn typed(nodes: &[Value]) -> Result<Vec<outline_rows::Node>, String> {
    nodes
        .iter()
        .map(|node| {
            serde_json::from_value(node.clone())
                .map_err(|error| format!("malformed logical node records: {error}"))
        })
        .collect()
}

fn document<'a>(
    forest: &outline_rows::Forest<'a>,
    owner: &str,
    coverage: Coverage,
    version: &'a str,
) -> Result<Value, String> {
    if version.trim().is_empty() {
        return Err("a request needs a nonblank supplied source_version".to_owned());
    }
    serde_json::to_value(forest.project(owner, coverage, version)?)
        .map_err(|error| format!("projected rows are not encodable: {error}"))
}

fn full_artifact<'a>(
    forest: &outline_rows::Forest<'a>,
    owner: &str,
    version: &'a str,
) -> Result<Value, String> {
    let text = response_bytes(&forest.project(owner, Coverage::Full, version)?)?;
    let (hash, tokens) = cost(&text);
    Ok(json!({"text": text, "meta": {"hash": hash, "tokens": tokens, "encoding": TOKEN_ENCODING}}))
}

/// The pinned LEAN rendering of a generated document in canonical lexicographic key order, plus
/// exactly one final LF. Cost metadata is a sidecar and never enters a counted document.
pub(crate) fn bytes(document: &Value) -> Result<String, String> {
    let Value::Object(fields) = document else {
        return Err("generated outline document must be a JSON object".to_owned());
    };
    let mut text = String::new();
    for (key, value) in fields {
        write_field_into(key, value, &mut text)?;
        text.push('\n');
    }
    // The historical borrowed empty-object boundary is one LF.
    if fields.is_empty() { text.push('\n'); }
    Ok(text)
}

/// Consume one native projected document without cloning its complete rows array.
#[allow(dead_code)] // indexed native publication; standalone observers use borrowed bytes
pub(crate) fn owned_bytes(document: Value) -> Result<String, String> {
    let Value::Object(fields) = document else {
        return Err("generated outline document must be a JSON object".to_owned());
    };
    let mut text = String::new();
    for (key, value) in fields {
        write_field_into(&key, &value, &mut text)?;
        text.push('\n');
    }
    Ok(text)
}

/// Render borrowed native rows in canonical root-field order. Only one row-sized
/// JSON object is temporary at a time; no complete Value array or full-text copy.
pub(crate) fn response_bytes(response: &outline_rows::Response<'_>) -> Result<String, String> {
    let metadata = json!({
        "coverage":response.coverage, "path_template":response.path_template,
        "rows":null, "source_version":response.source_version,
        "source_version_scope":response.source_version_scope,
        "token_encoding":response.token_encoding,
    });
    let mut text = String::new();
    for (key, value) in metadata.as_object().ok_or("missing response metadata")? {
        if key == "rows" {
            response_rows(response, &mut text)?;
        } else {
            write_field_into(key, value, &mut text)?;
        }
        text.push('\n');
    }
    Ok(text)
}

fn response_rows(response: &outline_rows::Response<'_>, text: &mut String) -> Result<(), String> {
    let rows = &response.rows;
    write!(text, "rows[{}]:", rows.len()).unwrap();
    let Some(first) = rows.first() else { return Ok(()) };
    let first = serde_json::to_value(first).map_err(|e| e.to_string())?;
    let fields: Vec<String> = first.as_object().ok_or("missing response row")?.keys().cloned().collect();
    drop(first);
    let mut shared = fields.clone();
    let mut uniform = true;
    for row in rows.iter().skip(1) {
        let value = serde_json::to_value(row).map_err(|e| e.to_string())?;
        let object = value.as_object().ok_or("missing response row")?;
        uniform &= object.keys().eq(fields.iter());
        shared.retain(|key| object.contains_key(key));
    }
    // Typed rows always share their mandatory navigation fields.
    text.push_str(&shared.join("\t"));
    if !uniform { text.push_str("\t~"); }
    for row in rows {
        let value = serde_json::to_value(row).map_err(|e| e.to_string())?;
        let object = value.as_object().ok_or("missing response row")?;
        text.push_str("\n  ");
        for (i, key) in shared.iter().enumerate() {
            if i > 0 { text.push('\t'); }
            text.push_str(&cell(&object[key])?);
        }
        for (key, value) in object.iter().filter(|(key, _)| !shared.contains(key)) {
            text.push('\t');
            text.push_str(key);
            text.push(':');
            text.push_str(&cell(value)?);
        }
    }
    Ok(())
}

/// SHA-256 of the complete UTF-8 text and its ordinary `o200k_base` token count.
pub(crate) fn cost(text: &str) -> (String, usize) {
    let tokens = sifttext_file_graph::token_count::ordinary(text);
    (sifttext_file_graph::sha256(text.as_bytes()), tokens)
}
