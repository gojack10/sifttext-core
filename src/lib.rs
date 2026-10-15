use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

// Standalone observers and reusable modules share this public crate identity.
extern crate self as sifttext_file_graph;

pub mod graph_transition;

#[cfg(test)]
#[path = "../tests/successor/candidate_refusals.rs"]
mod candidate_refusals_tests;
pub mod candidate_validation;
#[cfg(test)]
#[path = "../tests/successor/candidate_validation.rs"]
mod candidate_validation_tests;
#[cfg(test)]
#[path = "../tests/successor/capture_residue.rs"]
mod capture_residue_tests;
#[path = "vendor/leviathan/card.rs"]
pub mod card;
pub mod checked_publication;
mod checked_publication_attribution;
#[cfg(test)]
#[path = "../tests/successor/checked_publication.rs"]
mod checked_publication_tests;
mod coherent_capture;
#[cfg(test)]
#[path = "../tests/successor/coherent_capture.rs"]
mod coherent_capture_tests;
#[cfg(test)]
#[path = "../tests/successor/incremental_outlines.rs"]
mod incremental_outlines_tests;
pub mod index;
pub mod lean_render;
#[path = "vendor/leviathan/query_text.rs"]
pub mod leviathan_query;
#[path = "vendor/leviathan/search.rs"]
pub mod leviathan_search;
pub mod local;
#[cfg(test)]
#[path = "../tests/successor/local_onramp.rs"]
mod local_onramp_tests;
pub mod native_kb;
pub mod neighborhoods;
pub mod outline_artifacts;
#[cfg(test)]
#[path = "../tests/successor/outline_residue.rs"]
mod outline_residue_tests;
pub mod outline_rows;
pub mod outlines;
#[cfg(test)]
#[path = "../tests/successor/prepared_publication.rs"]
mod prepared_publication_tests;
#[cfg(test)]
#[path = "../tests/successor/publication_graph_transition.rs"]
mod publication_graph_transition_tests;
#[cfg(test)]
#[path = "../tests/successor/structural_diagnostics.rs"]
mod structural_diagnostics_tests;

#[cfg(test)]
#[path = "../tests/successor/outline_scaling.rs"]
mod outline_scaling_tests;
#[cfg(test)]
#[path = "../tests/successor/outlines.rs"]
mod outlines_tests;
#[path = "vendor/leviathan/relevance.rs"]
pub mod relevance;
pub mod search;
pub mod search_document;
#[path = "vendor/leviathan/snippet.rs"]
pub mod snippet;

#[cfg(test)]
#[path = "../tests/successor/search.rs"]
mod search_tests;

#[cfg(test)]
#[path = "../tests/successor/incremental_retrieval.rs"]
mod incremental_retrieval_tests;
#[cfg(test)]
#[path = "../tests/successor/index.rs"]
mod index_tests;
#[cfg(test)]
#[path = "../tests/successor/prepared_index_reuse.rs"]
mod prepared_index_reuse_tests;
#[cfg(test)]
#[path = "../tests/successor/retrieval_contention.rs"]
mod retrieval_contention_tests;
pub mod status;
pub mod token_metadata;

#[cfg(test)]
#[path = "../tests/successor/token_metadata.rs"]
mod token_metadata_tests;

#[cfg(test)]
#[path = "../tests/successor/generated_metadata.rs"]
mod generated_metadata_tests;

#[cfg(test)]
#[path = "../tests/successor/status.rs"]
mod status_tests;

#[cfg(test)]
#[path = "../tests/successor/neighborhoods.rs"]
mod neighborhood_tests;

#[cfg(test)]
#[path = "../tests/successor/cli_outcomes.rs"]
mod cli_outcomes_tests;

#[cfg(test)]
#[path = "../tests/successor/retrieval_refusals.rs"]
mod retrieval_refusals_tests;

#[cfg(test)]
#[path = "../tests/successor/wire_encoding.rs"]
mod wire_encoding_tests;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const LIMIT: usize = 1_048_576;
const PROCESS_DEADLINE: Duration = Duration::from_secs(10);
const MAX_POLICY_DEADLINE: Duration = Duration::from_secs(300);
// Frozen Runtime.bend cannot see a suffix beyond this read boundary.
const BACKEND_INPUT_BYTES: usize = 1_048_576;

#[cfg(test)]
#[path = "../tests/successor/process_framing.rs"]
mod process_framing_tests;

#[cfg(test)]
#[path = "../tests/successor/policy_deadline.rs"]
mod policy_deadline_tests;

#[cfg(test)]
#[path = "../tests/successor/store_caps.rs"]
mod store_caps_tests;

fn reserved<T>(count: usize) -> Result<Vec<T>> {
    let mut values = Vec::new();
    values.try_reserve_exact(count)?;
    Ok(values)
}

fn require(ok: bool, message: &str) -> Result<()> {
    if ok { Ok(()) } else { Err(message.into()) }
}

pub fn uuid(s: &str) -> bool {
    s.len() == 36
        && s.bytes().enumerate().all(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                c == b'-'
            } else {
                c.is_ascii_digit() || (b'a'..=b'f').contains(&c)
            }
        })
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Reference {
    pub target: String,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Authored {
    pub id: String,
    pub name: String,
    pub micro_summary: String,
    pub content: String,
    pub see_also: Vec<Reference>,
}

// A deliberately small front matter grammar: one key and JSON value per line.
// Body and unknown metadata remain opaque bytes; rename patches only the name value.
struct Markdown<'a> {
    authored: Authored,
    name_range: std::ops::Range<usize>,
    fields: BTreeMap<&'a str, &'a str>,
}

fn markdown(text: &str) -> Result<Markdown<'_>> {
    #[cfg(test)]
    coherent_capture::count(|c| c.parses += 1);
    let mut lines = text.split_inclusive('\n');
    let first = lines.next().ok_or("missing Markdown metadata")?;
    require(
        first == "---\n" || first == "---\r\n",
        "missing Markdown opening fence",
    )?;
    let mut offset = first.len();
    let mut fields = BTreeMap::new();
    let mut name_range = None;
    let mut body = None;
    for raw in lines {
        let line = raw
            .strip_suffix('\n')
            .ok_or("unterminated Markdown metadata")?;
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line == "---" {
            body = Some(&text[offset + raw.len()..]);
            break;
        }
        if !line.trim().is_empty() {
            let (key, value) = line.split_once(':').ok_or("malformed metadata line")?;
            let key = key.trim_matches([' ', '\t']);
            require(
                !key.is_empty()
                    && key.bytes().all(|c| {
                        c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_' || c == b'-'
                    }),
                "malformed metadata key",
            )?;
            require(
                !["parent", "parents", "children"].contains(&key),
                "second authored hierarchy source",
            )?;
            let trimmed = value.trim_matches([' ', '\t']);
            // Validate even unknown metadata, but never reserialize it.
            let _: serde_json::Value = serde_json::from_str(trimmed)?;
            require(
                fields.insert(key, trimmed).is_none(),
                "duplicate metadata key",
            )?;
            if key == "name" {
                let start = offset + line.find(':').unwrap() + 1 + value.len()
                    - value.trim_start_matches([' ', '\t']).len();
                name_range = Some(start..start + trimmed.len());
            }
        }
        offset += raw.len();
    }
    let field = |key| fields.get(key).copied().ok_or("missing required metadata");
    let authored = Authored {
        id: serde_json::from_str(field("id")?)?,
        name: serde_json::from_str(field("name")?)?,
        micro_summary: serde_json::from_str(field("micro_summary")?)?,
        see_also: serde_json::from_str(field("see_also")?)?,
        content: body.ok_or("missing Markdown closing fence")?.to_owned(),
    };
    Ok(Markdown {
        authored,
        name_range: name_range.ok_or("missing name metadata")?,
        fields,
    })
}

pub fn parse_markdown(text: &str) -> Result<Authored> {
    Ok(markdown(text)?.authored)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Entry {
    File(Vec<u8>),
    Link(PathBuf),
    Dir,
}

#[derive(Clone, Debug)]
pub struct Store {
    pub path: PathBuf,
    pub nodes: BTreeMap<String, Authored>,
    pub children: BTreeMap<String, Vec<String>>,
    pub roots: Vec<String>,
    pub inventory: BTreeMap<PathBuf, Entry>,
}

fn real_dir(path: &Path) -> Result<()> {
    require(
        fs::symlink_metadata(path)?.is_dir(),
        &format!("not a real directory: {}", path.display()),
    )
}

fn entries(path: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(path)? {
        paths.try_reserve(1)?;
        paths.push(entry?.path());
    }
    paths.sort();
    Ok(paths)
}

fn read_regular(path: &Path) -> Result<Vec<u8>> {
    let meta = fs::symlink_metadata(path)?;
    require(
        meta.is_file() && meta.len() <= LIMIT as u64,
        "unsafe or oversized authored file",
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        require(meta.nlink() == 1, "hard-linked authored file")?;
    }
    let bytes = fs::read(path)?;
    require(bytes.len() <= LIMIT, "oversized authored file")?;
    Ok(bytes)
}

fn name(path: &Path) -> Result<String> {
    Ok(path
        .file_name()
        .ok_or("missing filename")?
        .to_str()
        .ok_or("non-UTF-8 path")?
        .to_owned())
}

fn safe_link(path: &Path, expected: &Path, destination: &Path) -> Result<()> {
    require(
        fs::symlink_metadata(path)?.file_type().is_symlink(),
        "child/root entry must be a symlink",
    )?;
    require(
        fs::read_link(path)? == expected,
        "unsafe child/root link target",
    )?;
    require(
        fs::canonicalize(path)? == destination,
        "link does not name its canonical node",
    )
}

fn inventory(path: &Path, root: &Path, result: &mut BTreeMap<PathBuf, Entry>) -> Result<()> {
    for p in entries(path)? {
        let rel = p.strip_prefix(root)?.to_path_buf();
        if rel == Path::new(".git") {
            continue;
        }
        // SQLite is derived, can exceed the authored-file limit, and changes
        // during refresh. Never feed cache bytes back into authored validation.
        if [
            ".sifttext/index.sqlite",
            ".sifttext/index.sqlite-journal",
            ".sifttext/index.sqlite-wal",
            ".sifttext/index.sqlite-shm",
        ]
        .iter()
        .any(|cache| rel == Path::new(cache))
        {
            let meta = fs::symlink_metadata(&p)?;
            require(meta.is_file(), "unsafe derived cache entry")?;
            continue;
        }
        if token_metadata::derived(&rel) && token_metadata::safe_derived(&p)? {
            continue;
        }
        if outlines::derived(&rel) {
            outlines::safe_derived(&p)?;
            continue;
        }
        let parts: Vec<_> = rel.components().collect();
        if rel.parent() == Some(Path::new("nodes")) {
            real_dir(&p)?;
            require(
                uuid(&name(&p)?),
                "node directory is not a canonical full UUID",
            )?;
        }
        let authored_file = parts.len() == 3
            && parts[0].as_os_str() == "nodes"
            && ["node.md", "children.order"]
                .iter()
                .any(|n| parts[2].as_os_str() == *n);
        let meta = fs::symlink_metadata(&p)?;
        let e = if authored_file {
            Entry::File(read_regular(&p)?)
        } else if meta.file_type().is_symlink() {
            // Only relationship directories may contain links, already checked individually.
            let parts: Vec<_> = rel.components().collect();
            require(
                (parts.len() == 2 && parts[0].as_os_str() == "trees")
                    || (parts.len() == 4
                        && parts[0].as_os_str() == "nodes"
                        && parts[2].as_os_str() == "children"),
                "unsafe unrelated symlink",
            )?;
            Entry::Link(fs::read_link(&p)?)
        } else if meta.is_dir() {
            inventory(&p, root, result)?;
            Entry::Dir
        } else {
            Entry::File(read_regular(&p)?)
        };
        result.insert(rel, e);
    }
    Ok(())
}

impl Store {
    pub fn read(path: &Path) -> Result<Self> {
        Self::read_view(path, None)
    }

    /// Compose complete immutable base nodes with whole-node sparse overrides.
    /// A missing overlay node is inherited, never deleted or hydrated on disk.
    pub fn read_view(path: &Path, base: Option<&Store>) -> Result<Self> {
        Ok(Self::capture_view(path, base)?.0)
    }

    // Keep ephemeral metadata with its capture, preserving Store's public shape.
    pub(crate) fn capture_view(
        path: &Path,
        base: Option<&Store>,
    ) -> Result<(Self, coherent_capture::Metadata)> {
        real_dir(path)?;
        let path = fs::canonicalize(path)?;
        let residues = coherent_capture::residues(&path)?;
        Self::capture_classified(&path, base, &residues)
    }

    pub(crate) fn capture_classified(
        path: &Path,
        base: Option<&Store>,
        residues: &coherent_capture::Residues,
    ) -> Result<(Self, coherent_capture::Metadata)> {
        real_dir(path)?;
        let path = fs::canonicalize(path)?;
        let (files, dirs) = Self::inventory_classified(
            &path,
            base.map(|b| (b.path.as_path(), &b.inventory)),
            residues,
        )?;
        let captured = coherent_capture::from_inventory(path, files, dirs, base.is_some())?;
        residues.unchanged()?;
        Ok(captured)
    }

    // Shared by initial parsing and freshness-only recapture. Sparse inheritance
    // selects whole nodes from inventoried directories, including their assets.
    pub(crate) fn inventory_classified(
        path: &Path,
        base: Option<(&Path, &BTreeMap<PathBuf, Entry>)>,
        residues: &coherent_capture::Residues,
    ) -> Result<(BTreeMap<PathBuf, Entry>, BTreeMap<String, PathBuf>)> {
        real_dir(path)?;
        real_dir(&path.join("nodes"))?;
        real_dir(&path.join("trees"))?;
        residues.unchanged()?;
        #[cfg(test)]
        coherent_capture::count(|c| c.inventories += 1);
        let mut files = BTreeMap::new();
        inventory(path, path, &mut files)?;
        files.retain(|p, _| {
            let mut parts = p.components();
            !(parts.next().is_some_and(|p| p.as_os_str() == "nodes")
                && parts
                    .next()
                    .and_then(|p| p.as_os_str().to_str())
                    .is_some_and(|id| residues.contains(id)))
        });
        residues.unchanged()?;
        #[cfg(test)]
        coherent_capture::after_inventory();
        let mut dirs = BTreeMap::new();
        if let Some((base_path, base_files)) = base {
            for (rel, entry) in base_files {
                if rel.parent() == Some(Path::new("nodes")) {
                    require(matches!(entry, Entry::Dir), "not a real node directory")?;
                    dirs.insert(name(rel)?, base_path.join(rel));
                }
            }
        }
        for (rel, entry) in &files {
            if rel.parent() == Some(Path::new("nodes")) {
                require(matches!(entry, Entry::Dir), "not a real node directory")?;
                dirs.insert(name(rel)?, path.join(rel));
            }
        }
        if let Some((base_path, base_files)) = base {
            for (rel, entry) in base_files {
                let parts: Vec<_> = rel.components().collect();
                if parts.len() >= 2 && parts[0].as_os_str() == "nodes" {
                    let id = parts[1].as_os_str().to_str().ok_or("non-UTF-8 node")?;
                    if dirs.get(id).is_some_and(|dir| dir.starts_with(base_path)) {
                        files.insert(rel.clone(), entry.clone());
                    }
                }
            }
        }
        residues.unchanged()?;
        Ok((files, dirs))
    }

    pub fn resolve(&self, prefix: &str) -> Result<String> {
        require(!prefix.is_empty(), "empty UUID prefix")?;
        let matches: Vec<_> = self
            .nodes
            .keys()
            .filter(|id| id.starts_with(prefix))
            .cloned()
            .collect();
        require(matches.len() == 1, "UUID prefix absent or ambiguous")?;
        Ok(matches[0].clone())
    }

    pub fn view(&self) -> serde_json::Value {
        let node_path = |id: &str| self.path.join(format!("nodes/{id}/node.md"));
        let nodes: Vec<_> = self.nodes.values().map(|n| {
            let navigation: Vec<_> = n.see_also.iter().filter_map(|r| self.nodes.get(&r.target).map(|t|
                serde_json::json!({"target": r.target, "name": t.name, "micro_summary": t.micro_summary, "reason": r.reason, "path": node_path(&t.id)}))).collect();
            serde_json::json!({"id": n.id, "name": n.name, "micro_summary": n.micro_summary,
                "content": n.content, "children": self.children[&n.id], "see_also": n.see_also, "navigation": navigation, "path": node_path(&n.id)})
        }).collect();
        serde_json::json!({"roots": self.roots, "nodes": nodes})
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct WireNode {
    id: usize,
    payload: usize,
    children: Vec<usize>,
    refs: Vec<(usize, usize)>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Graph {
    assets: usize,
    roots: Vec<usize>,
    nodes: Vec<WireNode>,
}

#[derive(Default)]
struct Tokens(HashMap<Vec<u8>, usize>);
impl Tokens {
    fn intern(&mut self, bytes: &[u8]) -> usize {
        if let Some(&id) = self.0.get(bytes) {
            return id;
        }
        // Symbols follow encounter order, never the map's iteration order.
        let id = self.0.len() + 1;
        self.0.insert(bytes.to_vec(), id);
        id
    }
}

fn asset_bytes(s: &Store) -> Vec<u8> {
    // Exact, length-delimited inventory of every non-relationship file/directory.
    let mut out = Vec::new();
    for (p, e) in &s.inventory {
        let components: Vec<_> = p.components().collect();
        if components.first().is_some_and(|c| c.as_os_str() == "trees")
            || (components.len() >= 3
                && components[0].as_os_str() == "nodes"
                && ["children", "children.order", "node.md"]
                    .iter()
                    .any(|n| components[2].as_os_str() == *n))
        {
            continue;
        }
        let path = p.to_string_lossy();
        out.extend_from_slice(&(path.len() as u64).to_le_bytes());
        out.extend_from_slice(path.as_bytes());
        match e {
            Entry::Dir => out.push(0),
            Entry::File(b) => {
                out.push(1);
                out.extend_from_slice(&(b.len() as u64).to_le_bytes());
                out.extend_from_slice(b);
            }
            Entry::Link(_) => unreachable!("unrelated symlinks rejected"),
        }
    }
    out
}

fn encode(s: &Store, ids: &[String], tokens: &mut Tokens) -> Graph {
    let mut symbols = HashMap::with_capacity(ids.len());
    for (i, id) in ids.iter().enumerate() {
        // Retain the old position() semantics even for duplicate input IDs.
        symbols.entry(id.as_str()).or_insert(i + 1);
    }
    let id = |s: &str| symbols.get(s).copied().unwrap_or(0);
    Graph {
        assets: tokens.intern(&asset_bytes(s)),
        roots: s.roots.iter().map(|s| id(s)).collect(),
        nodes: s
            .nodes
            .values()
            .map(|n| WireNode {
                id: id(&n.id),
                payload: tokens.intern(
                    match &s.inventory[&PathBuf::from(format!("nodes/{}/node.md", n.id))] {
                        Entry::File(b) => b,
                        _ => unreachable!(),
                    },
                ),
                children: s.children[&n.id].iter().map(|s| id(s)).collect(),
                refs: n
                    .see_also
                    .iter()
                    .map(|r| (id(&r.target), tokens.intern(r.reason.as_bytes())))
                    .collect(),
            })
            .collect(),
    }
}

fn graph_word_count(g: &Graph) -> Result<usize> {
    let mut count = 3usize
        .checked_add(g.roots.len())
        .ok_or("graph size overflow")?;
    for n in &g.nodes {
        count = count
            .checked_add(4)
            .and_then(|c| c.checked_add(n.children.len()))
            .and_then(|c| n.refs.len().checked_mul(2).and_then(|r| c.checked_add(r)))
            .ok_or("graph size overflow")?;
    }
    Ok(count)
}

fn policy_budget(nodes: usize, items: usize) -> Result<Duration> {
    // The frozen evaluator repeatedly scans nodes for hierarchy/reference checks.
    // Keep the small-request deadline; allow bounded quadratic work, not an
    // untrusted wire-supplied timeout. Saturation is a time cap, not acceptance.
    let micros = nodes
        .checked_mul(items)
        .and_then(|work| work.checked_mul(30))
        .ok_or("policy deadline size overflow")?;
    if nodes <= 256 {
        return Ok(PROCESS_DEADLINE);
    }
    let budget = PROCESS_DEADLINE
        .checked_add(Duration::from_micros(u64::try_from(micros)?))
        .ok_or("policy deadline duration overflow")?;
    Ok(budget.min(MAX_POLICY_DEADLINE))
}

fn policy_deadline(graph: &Graph) -> Result<Duration> {
    let mut items = graph
        .nodes
        .len()
        .checked_add(graph.roots.len())
        .ok_or("policy deadline size overflow")?;
    for node in &graph.nodes {
        items = items
            .checked_add(node.children.len())
            .and_then(|n| n.checked_add(node.refs.len()))
            .ok_or("policy deadline size overflow")?;
    }
    policy_budget(graph.nodes.len(), items)
}

fn graph_words(g: &Graph) -> Result<Vec<usize>> {
    let mut v = reserved(graph_word_count(g)?)?;
    v.extend([g.assets, g.roots.len()]);
    v.extend(&g.roots);
    v.push(g.nodes.len());
    for n in &g.nodes {
        v.extend([n.id, n.payload, n.children.len()]);
        v.extend(&n.children);
        v.push(n.refs.len());
        for (t, r) in &n.refs {
            v.extend([t, r]);
        }
    }
    Ok(v)
}

fn graph_integer_envelope(request: &Graph) -> Result<usize> {
    let largest = request.nodes.iter().fold(request.assets, |max, n| {
        n.refs.iter().fold(
            n.children
                .iter()
                .copied()
                .fold(max.max(n.id).max(n.payload), usize::max),
            |max, &(t, r)| max.max(t).max(r),
        )
    });
    largest
        .max(request.roots.iter().copied().max().unwrap_or(0))
        .max(graph_word_count(request)?)
        .checked_add(1)
        .ok_or_else(|| "graph size overflow".into())
}

fn decimal_budget(words: usize, largest: usize) -> Result<usize> {
    words
        .checked_mul(largest.checked_ilog10().unwrap_or(0) as usize + 2)
        .and_then(|bytes| bytes.checked_sub(1))
        .ok_or_else(|| "graph size overflow".into())
}

fn parse_graph(text: &str, request: &Graph) -> Result<Graph> {
    // Move/reorder/rename preserve total edges, roots, nodes and reference counts.
    // Check the complete word budget BEFORE allocating from untrusted counts.
    let count = graph_word_count(request)?;
    require(
        text.len() <= decimal_budget(count, graph_integer_envelope(request)?)?,
        "process graph exceeds output limit",
    )?;
    let actual_count = text.split(' ').count();
    require(actual_count <= count, "trailing process output")?;
    require(actual_count >= count, "truncated process graph")?;
    // All output numbers come from the request, collection counts, or one new
    // rename symbol. This is a request-derived envelope, not a store ceiling.
    let envelope = graph_integer_envelope(request)?;
    let mut words = reserved(count)?;
    for word in text.split(' ') {
        let value = word.parse::<usize>()?;
        require(value <= envelope, "out-of-envelope process integer")?;
        words.push(value);
    }
    let mut it = words.into_iter();
    let next = |it: &mut std::vec::IntoIter<usize>| -> Result<usize> {
        it.next().ok_or_else(|| "truncated process graph".into())
    };
    let assets = next(&mut it)?;
    let nr = next(&mut it)?;
    require(nr == request.roots.len(), "process changed root count")?;
    let mut roots = reserved(nr)?;
    for _ in 0..nr {
        roots.push(next(&mut it)?);
    }
    let nn = next(&mut it)?;
    require(nn == request.nodes.len(), "process changed node count")?;
    let mut nodes = reserved(nn)?;
    let mut edges = request.nodes.iter().try_fold(0usize, |sum, n| {
        sum.checked_add(n.children.len())
            .ok_or("graph size overflow")
    })?;
    for old in &request.nodes {
        let id = next(&mut it)?;
        let payload = next(&mut it)?;
        let nc = next(&mut it)?;
        require(
            nc <= edges && nc <= it.len(),
            "process changed child budget",
        )?;
        edges -= nc;
        let mut children = reserved(nc)?;
        for _ in 0..nc {
            children.push(next(&mut it)?);
        }
        let nrefs = next(&mut it)?;
        require(nrefs == old.refs.len(), "process changed reference count")?;
        require(
            nrefs.checked_mul(2).is_some_and(|n| n <= it.len()),
            "truncated process references",
        )?;
        let mut refs = reserved(nrefs)?;
        for _ in 0..nrefs {
            refs.push((next(&mut it)?, next(&mut it)?));
        }
        nodes.push(WireNode {
            id,
            payload,
            children,
            refs,
        });
    }
    require(
        edges == 0 && it.next().is_none(),
        "trailing or missing process edges",
    )?;
    Ok(Graph {
        assets,
        roots,
        nodes,
    })
}

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn candidate_files(project: &Path) -> Result<BTreeSet<String>> {
    fn walk(project: &Path, dir: &Path, files: &mut BTreeSet<String>) -> Result<()> {
        for path in entries(dir)? {
            let meta = fs::symlink_metadata(&path)?;
            if meta.is_dir() {
                walk(project, &path, files)?;
            } else {
                require(meta.is_file(), "unsafe candidate source entry")?;
                files.insert(
                    path.strip_prefix(project)?
                        .to_str()
                        .ok_or("non-UTF-8 candidate path")?
                        .to_owned(),
                );
            }
        }
        Ok(())
    }
    let mut files: BTreeSet<String> = ["Cargo.toml", "Cargo.lock", "README.md", ".gitignore"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    if project.join("rust-toolchain.toml").exists() {
        files.insert("rust-toolchain.toml".to_owned());
    }
    for dir in ["src", "laws", "scripts", "tests", "fixtures", "docs"] {
        let path = project.join(dir);
        if path.exists() {
            real_dir(&path)?;
            walk(project, &path, &mut files)?;
        }
    }
    Ok(files)
}

pub struct Policy {
    executable: PathBuf,
    project: PathBuf,
}
impl Policy {
    pub fn checked(project: &Path) -> Result<Self> {
        let policy = Self {
            executable: project.join("build/graph-policy"),
            project: project.to_path_buf(),
        };
        policy.verify_stamp()?;
        Ok(policy)
    }

    fn verify_stamp(&self) -> Result<()> {
        let project = &self.project;
        for manifest in ["build/candidate.sha256", "build/graph-policy.sha256"] {
            let contents = fs::read_to_string(project.join(manifest))?;
            require(!contents.trim().is_empty(), "empty build stamp")?;
            if manifest == "build/graph-policy.sha256" {
                require(
                    contents.lines().count() == 1
                        && contents
                            .lines()
                            .next()
                            .is_some_and(|l| l.ends_with("  build/graph-policy")),
                    "incorrect executable stamp",
                )?;
            }
            let mut stamped = BTreeSet::new();
            for line in contents.lines() {
                let (expected, file) = line.split_once("  ").ok_or("malformed build stamp")?;
                require(
                    !Path::new(file).is_absolute() && !file.split('/').any(|c| c == ".."),
                    "unsafe build stamp path",
                )?;
                require(
                    stamped.insert(file.to_owned()),
                    "duplicate build stamp entry",
                )?;
                require(
                    sha256(&fs::read(project.join(file))?) == expected,
                    "checked candidate changed; rebuild and recheck",
                )?;
            }
            if manifest == "build/candidate.sha256" {
                require(
                    stamped == candidate_files(project)?,
                    "candidate file set changed; rebuild and recheck",
                )?;
            }
        }
        Ok(())
    }

    /// Same-invocation validator identity, including the native graph executable
    /// stamp and canonical policy home, not only a replaceable source manifest.
    /// This only pins identity; native callers still verify the checked stamps.
    pub(crate) fn validation_identity(&self) -> Result<String> {
        let mut bytes = Vec::new();
        for value in [
            fs::canonicalize(&self.project)?
                .as_os_str()
                .as_encoded_bytes()
                .to_vec(),
            crate::read_regular(&self.project.join("build/candidate.sha256"))?,
            crate::read_regular(&self.project.join("build/graph-policy.sha256"))?,
        ] {
            bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
            bytes.extend(value);
        }
        Ok(sha256(&bytes))
    }

    fn call(&self, kind: usize, x: usize, y: usize, graph: &Graph) -> Result<String> {
        self.verify_stamp()?;
        let count = graph_word_count(graph)?
            .checked_add(4)
            .ok_or("graph size overflow")?;
        let mut words = reserved(count)?;
        words.extend([1, kind, x, y]);
        words.extend(graph_words(graph)?);
        let wire = wire_text(&words)?;
        let stdout_bytes = if kind == 0 {
            "STG1 REJECT\n".len()
        } else {
            decimal_budget(
                graph_word_count(graph)?,
                graph_integer_envelope(graph)?.max(y),
            )?
            .checked_add("STG1 OK \n".len())
            .ok_or("graph size overflow")?
        };
        invoke_bounded_with_deadline(
            &self.executable,
            wire.as_bytes(),
            stdout_bytes,
            policy_deadline(graph)?,
        )
    }

    pub(crate) fn graph_valid(&self, s: &Store) -> Result<bool> {
        let ids = s.nodes.keys().cloned().collect::<Vec<_>>();
        let graph = encode(s, &ids, &mut Tokens::default());
        let verdict = self.call(0, 0, 0, &graph).map_err(|error| {
            if error.to_string() == "policy process output exceeds limit" {
                "malformed checked graph verdict".into()
            } else {
                error
            }
        })?;
        match verdict.as_str() {
            "STG1 VALID\n" => Ok(true),
            "STG1 REJECT\n" => Ok(false),
            _ => Err("malformed checked graph verdict".into()),
        }
    }

    pub fn validate(&self, s: &Store) -> Result<()> {
        require(
            self.graph_valid(s)?,
            "checked graph rejected or malformed verdict",
        )
    }
}

fn wire_text(words: &[usize]) -> Result<String> {
    let bytes = words
        .iter()
        .try_fold(0usize, |sum, word| {
            sum.checked_add(word.checked_ilog10().unwrap_or(0) as usize + 2)
                .ok_or("graph size overflow")
        })?
        .saturating_sub(1);
    require(
        bytes <= BACKEND_INPUT_BYTES,
        "wire exceeds frozen backend input limit",
    )?;
    let mut wire = String::new();
    wire.try_reserve_exact(bytes)?;
    for (i, word) in words.iter().enumerate() {
        if i != 0 {
            wire.push(' ');
        }
        use std::fmt::Write;
        write!(wire, "{word}")?;
    }
    Ok(wire)
}

fn process_output(mut pipe: impl Read, limit: usize) -> std::io::Result<Vec<u8>> {
    let end = limit
        .checked_add(1)
        .ok_or_else(|| std::io::Error::other("output size overflow"))?;
    let mut bytes = Vec::new();
    let mut chunk = [0; 8192];
    while bytes.len() < end {
        let remaining = (end - bytes.len()).min(chunk.len());
        let n = match pipe.read(&mut chunk[..remaining]) {
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if n == 0 {
            return Ok(bytes);
        }
        bytes.try_reserve(n).map_err(std::io::Error::other)?;
        bytes.extend_from_slice(&chunk[..n]);
        if bytes.len() > limit {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "policy process output exceeds limit",
            ));
        }
    }
    unreachable!()
}

// Public for prescribed process-failure tests, not an alternate CLI acceptance path.
pub fn invoke(executable: &Path, input: &[u8]) -> Result<String> {
    invoke_with_deadline(executable, input, PROCESS_DEADLINE)
}

fn invoke_with_deadline(executable: &Path, input: &[u8], budget: Duration) -> Result<String> {
    let stdout_bytes = decimal_budget(input.len().max(1), usize::MAX)?
        .checked_add("STG1 OK \n".len())
        .ok_or("output size overflow")?;
    invoke_bounded_with_deadline(executable, input, stdout_bytes, budget)
}

#[cfg(test)]
fn invoke_bounded(executable: &Path, input: &[u8], stdout_bytes: usize) -> Result<String> {
    invoke_bounded_with_deadline(executable, input, stdout_bytes, PROCESS_DEADLINE)
}

fn invoke_bounded_with_deadline(
    executable: &Path,
    input: &[u8],
    stdout_bytes: usize,
    budget: Duration,
) -> Result<String> {
    require(
        input.len() <= BACKEND_INPUT_BYTES,
        "wire exceeds frozen backend input limit",
    )?;
    stdout_bytes.checked_add(1).ok_or("output size overflow")?;
    require(
        !budget.is_zero() && budget <= MAX_POLICY_DEADLINE,
        "policy deadline exceeds resource budget",
    )?;
    let mut file = tempfile::NamedTempFile::new()?;
    file.write_all(input)?;
    file.flush()?;
    let mut child = Command::new(executable)
        .env("SIFTTEXT_WIRE", file.path())
        .args(["--threads", "1", "--gpu", "off"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take().ok_or("missing stdout")?;
    let stderr = child.stderr.take().ok_or("missing stderr")?;
    let (tx, rx) = mpsc::channel();
    for (tag, pipe, limit) in [
        (0, Box::new(stdout) as Box<dyn Read + Send>, stdout_bytes),
        (1, Box::new(stderr) as Box<dyn Read + Send>, 0),
    ] {
        let tx = tx.clone();
        if let Err(error) = thread::Builder::new().spawn(move || {
            let _ = tx.send((tag, process_output(pipe, limit)));
        }) {
            let _ = child.kill();
            child.wait()?;
            return Err(error.into());
        }
    }
    drop(tx);
    let result = (|| -> Result<String> {
        let deadline = Instant::now() + budget;
        let mut status = None;
        let mut output = [None, None];
        loop {
            if status.is_none() {
                status = child.try_wait()?;
            }
            while let Ok((tag, bytes)) = rx.try_recv() {
                output[tag] = Some(match bytes {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        // Closing rejected stdout can cause SIGPIPE; keep its initiating refusal.
                        if tag == 0 && error.kind() == std::io::ErrorKind::InvalidData {
                            return Err("policy process output exceeds limit".into());
                        }
                        thread::sleep(Duration::from_millis(1));
                        if let Some(status) = child.try_wait()? {
                            require(status.success(), "policy process exited nonzero")?;
                        }
                        if error.kind() == std::io::ErrorKind::InvalidData {
                            return Err(if tag == 1 {
                                "unexpected policy stderr"
                            } else {
                                "policy process output exceeds limit"
                            }
                            .into());
                        }
                        return Err(error.into());
                    }
                });
            }
            if let Some(status) = status
                && output[0].is_some()
                && output[1].is_some()
            {
                require(status.success(), "policy process exited nonzero")?;
                return Ok(std::str::from_utf8(&output[0].take().unwrap())?.to_owned());
            }
            if Instant::now() >= deadline {
                return Err("policy process timed out".into());
            }
            thread::sleep(Duration::from_millis(1));
        }
    })();
    if result.is_err() {
        let _ = child.kill();
        child.wait()?;
    }
    result
}

#[derive(Clone, Debug)]
pub enum Operation {
    Move { node: String, parent: String },
    Reorder { node: String, after: String },
    Rename { node: String, name: String },
}

fn order_bytes(ids: &[String]) -> Vec<u8> {
    if ids.is_empty() {
        Vec::new()
    } else {
        (ids.join("\n") + "\n").into_bytes()
    }
}

fn plan(policy: &Policy, before: &Store, operation: &Operation) -> Result<Store> {
    let ids = before.nodes.keys().cloned().collect::<Vec<_>>();
    let index = |s: &str| -> Result<usize> {
        Ok(ids
            .iter()
            .position(|i| i == s)
            .ok_or("unknown operation UUID")?
            + 1)
    };
    let mut tokens = Tokens::default();
    let graph = encode(before, &ids, &mut tokens);
    let mut planned = before.clone();
    let (kind, x, y) = match operation {
        Operation::Move { node, parent } => (1, index(node)?, index(parent)?),
        Operation::Reorder { node, after } => (2, index(node)?, index(after)?),
        Operation::Rename { node, name } => {
            require(!name.trim().is_empty(), "blank rename")?;
            let n = planned.nodes.get_mut(node).ok_or("unknown rename UUID")?;
            n.name = name.clone();
            let path = PathBuf::from(format!("nodes/{node}/node.md"));
            let Entry::File(original) = &before.inventory[&path] else {
                unreachable!()
            };
            let mut text = std::str::from_utf8(original)?.to_owned();
            let range = markdown(&text)?.name_range;
            text.replace_range(range, &serde_json::to_string(name)?);
            let bytes = text.into_bytes();
            require(bytes.len() <= LIMIT, "oversized renamed Markdown file")?;
            let payload = tokens.intern(&bytes);
            planned.inventory.insert(path, Entry::File(bytes));
            (3, index(node)?, payload)
        }
    };
    let output = policy.call(kind, x, y, &graph)?;
    let text = output
        .strip_prefix("STG1 OK ")
        .and_then(|s| s.strip_suffix('\n'))
        .ok_or("checked operation rejected or malformed verdict")?;
    let got = parse_graph(text, &graph)?;
    // Validate untrusted decoding and preservation, not a second implementation of move policy.
    require(
        got.assets == graph.assets
            && got.roots == graph.roots
            && got.nodes.len() == graph.nodes.len(),
        "process changed assets, roots or node inventory",
    )?;
    let rename_payload = if kind == 3 { Some((x, y)) } else { None };
    for (old, new) in graph.nodes.iter().zip(&got.nodes) {
        require(
            new.id == old.id && new.refs == old.refs,
            "process changed identities or references",
        )?;
        let expected_payload = rename_payload
            .filter(|(id, _)| *id == old.id)
            .map_or(old.payload, |(_, p)| p);
        require(
            new.payload == expected_payload,
            "process changed protected payload",
        )?;
        let mut unique = BTreeSet::new();
        let mut children = reserved(new.children.len())?;
        for &i in &new.children {
            require(
                i > 0 && i <= ids.len() && unique.insert(i),
                "invalid process child UUID",
            )?;
            let mut id = String::new();
            id.try_reserve_exact(ids[i - 1].len())?;
            id.push_str(&ids[i - 1]);
            children.push(id);
        }
        planned.children.insert(ids[new.id - 1].clone(), children);
    }
    // The filesystem effect is intentionally narrow. Unexpected policy effect lists fail closed.
    let changed: Vec<_> = ids
        .iter()
        .filter(|id| before.children[*id] != planned.children[*id])
        .collect();
    match operation {
        Operation::Move { node, parent } => {
            let old_parent = before
                .children
                .iter()
                .find(|(_, cs)| cs.contains(node))
                .map(|(id, _)| id)
                .ok_or("root has no movable entry")?;
            require(
                changed.iter().all(|id| *id == old_parent || *id == parent),
                "process changed unrelated children",
            )?;
            if old_parent != parent {
                let old_path = PathBuf::from(format!("nodes/{old_parent}/children/{node}"));
                let new_path = PathBuf::from(format!("nodes/{parent}/children/{node}"));
                let entry = planned
                    .inventory
                    .remove(&old_path)
                    .ok_or("missing old child entry")?;
                require(
                    !planned.inventory.contains_key(&new_path),
                    "destination child already exists",
                )?;
                planned.inventory.insert(new_path, entry);
            }
        }
        Operation::Reorder { node, .. } => {
            let parent = before
                .children
                .iter()
                .find(|(_, cs)| cs.contains(node))
                .map(|(id, _)| id)
                .ok_or("root cannot reorder")?;
            require(
                changed.iter().all(|id| *id == parent),
                "process reordered unrelated parent",
            )?;
        }
        Operation::Rename { .. } => {
            require(changed.is_empty(), "process rename changed hierarchy")?
        }
    }
    for id in changed {
        planned.inventory.insert(
            PathBuf::from(format!("nodes/{id}/children.order")),
            Entry::File(order_bytes(&planned.children[id])),
        );
    }
    // Roundtrip the pure output through the same checked validator before any native writes.
    policy.validate(&planned)?;
    Ok(planned)
}

pub fn validate_draft(
    policy: &Policy,
    before: &Store,
    after: &Store,
    operation: &Operation,
) -> Result<()> {
    let planned = plan(policy, before, operation)?;
    policy.validate(after)?;
    require(
        planned.nodes == after.nodes
            && planned.children == after.children
            && planned.roots == after.roots
            && planned.inventory == after.inventory,
        "native draft differs from the checked operation (bytes or graph)",
    )
}

pub fn apply(policy: &Policy, before: &Store, operation: &Operation) -> Result<Store> {
    let planned = plan(policy, before, operation)?;
    require(
        Store::read(&before.path)?.inventory == before.inventory,
        "candidate changed after checking",
    )?;
    if let Operation::Move { node, parent } = operation {
        let old_parent = before
            .children
            .iter()
            .find(|(_, cs)| cs.contains(node))
            .map(|(id, _)| id)
            .ok_or("root has no movable entry")?;
        if old_parent != parent {
            let source = before
                .path
                .join(format!("nodes/{old_parent}/children/{node}"));
            let destination = before.path.join(format!("nodes/{parent}/children/{node}"));
            require(
                !destination.try_exists()? && fs::symlink_metadata(&destination).is_err(),
                "move would overwrite an entry",
            )?;
            require(
                Command::new("mv")
                    .arg(&source)
                    .arg(&destination)
                    .status()?
                    .success(),
                "native mv failed; draft remains inspectable",
            )?;
        }
    }
    for (rel, entry) in &planned.inventory {
        if before.inventory.get(rel) != Some(entry)
            && let Entry::File(bytes) = entry
        {
            fs::write(before.path.join(rel), bytes)?;
        }
    }
    let after = Store::read(&before.path)?;
    validate_draft(policy, before, &after, operation)?;
    Ok(after)
}
