//! Compact immutable authored Git inputs. Object bytes are never hydrated as a Store.
use crate::{
    Graph, Policy, Result, WireNode,
    candidate_validation::{intrinsic_fields, structural_guard},
    coherent_capture::{authored_path, node_markdown},
    require, source_error,
    source_version::{self, Part},
    status::State,
    uuid,
};
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Stdio},
    sync::{Arc, Weak},
};

#[derive(Debug)]
pub(crate) struct NodeFacts {
    pub(crate) id: String,
    pub(crate) position: Option<u64>,
    pub(crate) state: State,
    pub(crate) body_nonblank: bool,
    pub(crate) raw_hash: [u8; 32],
    pub(crate) text_hash: [u8; 32],
    pub(crate) body_hash: [u8; 32],
    pub(crate) reference_targets: Vec<String>,
    pub(crate) blob: String,
    pub(crate) len: u64,
    pub(crate) field_errors: u8,
    pub(crate) migration_size: bool,
}
pub(crate) struct GitView {
    pub(crate) version: String,
    pub(crate) tree: String,
    pub(crate) nodes: BTreeMap<String, Arc<NodeFacts>>,
    pub(crate) children: BTreeMap<String, Vec<String>>,
    pub(crate) roots: Vec<String>,
}
impl GitView {
    /// Complete conservative placement frontier in candidate ordinals. Compare
    /// UUIDs first: dense ordinal labels are local to each immutable view.
    pub(crate) fn placement_frontier(&self, before: &Self) -> Vec<usize> {
        let mut changed = std::collections::BTreeSet::new();
        changed.extend(self.nodes.keys().filter(|id| !before.nodes.contains_key(*id)));
        let sources: std::collections::BTreeSet<_> = before.children.keys()
            .chain(self.children.keys()).collect();
        for source in sources {
            if before.children.get(source) != self.children.get(source) {
                for target in before.children.get(source).into_iter().flatten()
                    .chain(self.children.get(source).into_iter().flatten()) {
                    if self.nodes.contains_key(target) { changed.insert(target); }
                }
            }
        }
        if before.roots != self.roots {
            changed.extend(before.roots.iter().chain(self.roots.iter())
                .filter(|id| self.nodes.contains_key(*id)));
        }
        self.nodes.keys().enumerate().filter_map(|(ordinal, id)|
            changed.contains(id).then_some(ordinal + 1)).collect()
    }

    /// Shape.bend proves this erasure preserves the full Spec.valid predicate.
    pub(crate) fn graph(&self) -> Graph {
        let symbols: HashMap<_, _> = self
            .nodes
            .keys()
            .enumerate()
            .map(|(i, id)| (id.as_str(), i + 1))
            .collect();
        let ordinal = |id: &str| symbols.get(id).copied().unwrap_or(0);
        Graph {
            assets: 0,
            roots: self.roots.iter().map(|id| ordinal(id)).collect(),
            nodes: self
                .nodes
                .iter()
                .map(|(id, node)| WireNode {
                    id: ordinal(id),
                    payload: 0,
                    children: self.children[id].iter().map(|id| ordinal(id)).collect(),
                    refs: node
                        .reference_targets
                        .iter()
                        .map(|id| (ordinal(id), 1))
                        .collect(),
                })
                .collect(),
        }
    }
}

pub(crate) struct Batch {
    child: Child,
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
    alive: bool,
    empty_blob: Option<String>,
}
impl Batch {
    pub(crate) fn new(repo: &Path) -> Result<Self> {
        let mut child = crate::local::reader_command(repo, &["cat-file", "--batch"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let input = child.stdin.take();
        let output = BufReader::new(child.stdout.take().ok_or("missing Git object output")?);
        Ok(Self {
            child,
            input,
            output,
            alive: true,
            empty_blob: None,
        })
    }
    fn stop(&mut self) {
        self.input.take();
        if self.alive {
            let _ = self.child.kill();
            let _ = self.child.wait();
            self.alive = false;
        }
    }
    pub(crate) fn bytes(&mut self, oid: &str, len: u64) -> Result<Vec<u8>> {
        let result = (|| {
            require(
                self.alive && valid_oid(oid) && len <= crate::migration_size::INPUT_LIMIT as u64,
                "invalid Git object request",
            )?;
            // Leaves and directory markers share the same immutable empty blob.
            // Verify it once, not once per node in every view of this operation.
            if len == 0 && self.empty_blob.as_deref() == Some(oid) {
                return Ok(Vec::new());
            }
            let input = self.input.as_mut().ok_or("closed Git object input")?;
            writeln!(input, "{oid}")?;
            input.flush()?;
            let mut header = Vec::new();
            self.output
                .by_ref()
                .take(256)
                .read_until(b'\n', &mut header)?;
            require(
                header.last() == Some(&b'\n') && header.len() < 256,
                "oversized Git object header",
            )?;
            require(
                header == format!("{oid} blob {len}\n").as_bytes(),
                "missing or changed Git blob",
            )?;
            let mut bytes = vec![0; len as usize];
            self.output.read_exact(&mut bytes)?;
            let mut end = [0];
            self.output.read_exact(&mut end)?;
            require(end == [b'\n'], "malformed Git object framing")?;
            if len == 0 { self.empty_blob = Some(oid.to_owned()); }
            Ok(bytes)
        })();
        if result.is_err() {
            self.stop();
        }
        result
    }
}
impl Drop for Batch {
    fn drop(&mut self) {
        self.stop();
    }
}

fn valid_oid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64)
        && oid
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

struct TreeEntry {
    kind: u8,
    oid: String,
    len: u64,
}

// Only metadata is staged; SQLite's bounded page cache replaces the path inventory.
pub(crate) struct TreeLedger {
    pub(crate) connection: Connection,
    _file: tempfile::NamedTempFile,
    _operation: crate::scratch::Directory,
}
impl TreeLedger {
    fn new() -> Result<Self> {
        let operation = crate::scratch::Directory::new()?;
        let file = tempfile::NamedTempFile::new_in(operation.path())?;
        let connection = Connection::open(file.path())?;
        connection.execute_batch("PRAGMA cache_size=-1024; PRAGMA mmap_size=0;
            PRAGMA temp_store=FILE; PRAGMA journal_mode=OFF; PRAGMA synchronous=OFF;
            CREATE TABLE entries (
                path TEXT PRIMARY KEY, ordering BLOB NOT NULL UNIQUE,
                parent TEXT NOT NULL, name TEXT NOT NULL, kind INTEGER NOT NULL,
                mode TEXT NOT NULL, oid TEXT NOT NULL, len INTEGER NOT NULL
            ) WITHOUT ROWID;
            CREATE INDEX entry_members ON entries(parent,name);")?;
        Ok(Self { connection, _file: file, _operation: operation })
    }
    fn has(&self, path: &str, kind: u8) -> Result<bool> {
        Ok(self.connection.prepare_cached(
            "SELECT EXISTS(SELECT 1 FROM entries WHERE path=?1 AND kind=?2)")?
            .query_row(rusqlite::params![path, kind], |row| row.get(0))?)
    }
}

fn insert_entry(insert: &mut rusqlite::Statement<'_>, path: &str, kind: u8,
    mode: &str, oid: &str, len: u64) -> Result<()> {
    let (parent, name) = path.rsplit_once('/').unwrap_or(("", path));
    // Same component ordering as index::current: children/<UUID> precedes
    // node metadata. The source boundary already excludes NUL components.
    let ordering = path.replace('/', "\0").into_bytes();
    let len = i64::try_from(len)?;
    insert.execute(rusqlite::params![path, ordering, parent, name, kind, mode, oid, len])?;
    Ok(())
}

pub(crate) struct GitReader {
    pub(crate) repo: PathBuf,
    pub(crate) policy_identity: String,
    pub(crate) imported_baseline: Option<(String, String)>,
    pub(crate) submitted_nodes: Option<std::collections::BTreeSet<String>>,
    batch: Option<Batch>,
    empty_blob: Option<String>,
    nodes: HashMap<String, Weak<NodeFacts>>,
    views: HashMap<String, Arc<GitView>>,
    tokens: BTreeMap<[u8; 32], usize>,
}
impl GitReader {
    pub(crate) fn new(policy: &Policy, repo: &Path) -> Result<Self> {
        policy.verify_stamp()?;
        let repo = std::fs::canonicalize(repo)?;
        let identity = policy.validation_identity()?;
        Ok(Self {
            batch: None,
            imported_baseline: None,
            submitted_nodes: None,
            empty_blob: None,
            repo,
            policy_identity: identity,
            nodes: HashMap::new(),
            views: HashMap::new(),
            tokens: BTreeMap::new(),
        })
    }

    pub(crate) fn release_views(&mut self) {
        self.views.clear();
    }

    pub(crate) fn suspend_objects(&mut self) {
        self.batch.take(); // Batch::drop closes stdio, kills and reaps the helper.
    }

    fn object_bytes(&mut self, oid: &str, len: u64) -> Result<Vec<u8>> {
        require(valid_oid(oid) && len <= crate::migration_size::INPUT_LIMIT as u64, "invalid Git object request")?;
        if len == 0 && self.empty_blob.as_deref() == Some(oid) { return Ok(Vec::new()); }
        if self.batch.is_none() { self.batch = Some(Batch::new(&self.repo)?); }
        let bytes = self.batch.as_mut().ok_or("missing Git object reader")?.bytes(oid, len)?;
        if len == 0 { self.empty_blob = Some(oid.to_owned()); }
        Ok(bytes)
    }

    pub(crate) fn inventory(repo: &Path, tree: &str) -> Result<TreeLedger> {
        let mut child =
            crate::local::reader_command(repo, &["ls-tree", "-r", "-t", "-l", "-z", tree])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()?;
        let result = (|| {
            let mut output = BufReader::new(child.stdout.take().ok_or("missing Git tree output")?);
            let mut ledger = TreeLedger::new()?;
            let tx = ledger.connection.transaction()?;
            let mut insert = tx.prepare("INSERT INTO entries VALUES (?1,?2,?3,?4,?5,?6,?7,?8)")?;
            let mut record = Vec::new();
            loop {
                record.clear();
                let n = output
                    .by_ref()
                    .take(crate::LIMIT as u64 + 1)
                    .read_until(0, &mut record)?;
                if n == 0 {
                    break;
                }
                require(
                    record.len() <= crate::LIMIT && record.pop() == Some(0),
                    "oversized Git tree record",
                )?;
                let text = std::str::from_utf8(&record)?;
                let (header, path) = text.split_once('\t').ok_or("malformed Git tree record")?;
                require(
                    !path.is_empty()
                        && !path.contains(['\r', '\n'])
                        && path
                            .split('/')
                            .all(|part| !part.is_empty() && part != "." && part != ".."),
                    "unsafe Git tree path",
                )?;
                let words: Vec<_> = header.split_ascii_whitespace().collect();
                require(
                    words.len() == 4 && valid_oid(words[2]) && words[2].len() == tree.len(),
                    "malformed Git tree metadata",
                )?;
                let kind = match (words[0], words[1]) {
                    ("040000", "tree") => 0,
                    ("100644", "blob") => 1,
                    ("120000", "blob") => 2,
                    ("100755", "blob") if !authored_path(Path::new(path)) => 1,
                    _ => return Err("unsupported Git entry mode or Gitlink".into()),
                };
                let len = if kind == 0 {
                    require(words[3] == "-", "malformed Git directory size")?;
                    0
                } else {
                    require(
                        !words[3].is_empty() && words[3].bytes().all(|b| b.is_ascii_digit()),
                        "missing Git blob size",
                    )?;
                    words[3].parse::<u64>()?
                };
                let path = PathBuf::from(path);
                let parts: Vec<_> = path.components().collect();
                if parts.len() == 2
                    && parts[0].as_os_str() == "nodes"
                    && parts[1].as_os_str() != ".keep"
                {
                    require(
                        kind == 0 && uuid(parts[1].as_os_str().to_str().unwrap_or_default()),
                        "invalid Git node directory",
                    )?;
                }
                if !authored_path(&path) && path != Path::new("nodes") {
                    continue;
                }
                require(
                    kind == 0 || len <= crate::migration_size::input_limit(&path) as u64,
                    "oversized authored Git blob",
                )?;
                insert_entry(&mut insert, path.to_str().ok_or("non-UTF-8 Git path")?,
                    kind, words[0], words[2], len)?;
            }
            drop(insert);
            tx.commit()?;
            Ok(ledger)
        })();
        if result.is_err() {
            let _ = child.kill();
        }
        let status = child.wait()?;
        require(status.success(), "Git tree enumeration failed")?;
        result
    }

    pub(crate) fn view(&mut self, revision: &str) -> Result<Arc<GitView>> {
        let imported = self.imported_baseline.as_ref()
            .filter(|(original, _)| original == revision).map(|(_, imported)| imported.clone());
        let revision = imported.as_deref().unwrap_or(revision);
        require(
            !revision.is_empty()
                && !revision.starts_with('-')
                && !revision.contains(['\n', '\r', '\0']),
            "invalid Git revision",
        )?;
        let tree = crate::local::reader_git(
            &self.repo,
            &["rev-parse", "--verify", &format!("{revision}^{{tree}}")],
        )?;
        require(valid_oid(&tree), "invalid Git tree identity")?;
        // One operation can inspect a received contribution, combine it and
        // validate it. Identical immutable trees share the prepared view.
        if let Some(view) = self.views.get(&tree) {
            return Ok(Arc::clone(view));
        }
        let started = std::time::Instant::now();
        let ledger = Self::inventory(&self.repo, &tree)?;
        // Git omits empty directories. Root containers retain their existing
        // contract; every node has a logical child container, even at a leaf.
        let populated: bool = ledger.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM entries WHERE parent='nodes' AND name<>'.keep')",
            [], |row| row.get(0))?;
        if populated {
            require(ledger.has("nodes", 0)? && ledger.has("trees", 0)?,
                "missing Git root container")?;
        }
        {
            let mut insert = ledger.connection.prepare(
                "INSERT OR IGNORE INTO entries VALUES (?1,?2,?3,?4,?5,?6,?7,?8)")?;
            for path in ["nodes", "trees"] {
                insert_entry(&mut insert, path, 0, "040000", "", 0)?;
            }
        }
        ledger.connection.execute("INSERT OR IGNORE INTO entries
            SELECT path||'/children',CAST(replace(path||'/children','/',char(0)) AS BLOB),
                path,'children',0,'040000','',0 FROM entries
            WHERE parent='nodes' AND kind=0", [])?;
        let mut nodes = BTreeMap::new();
        let (mut entry_count, mut memory_hits) = (0usize, 0usize);
        let mut children = BTreeMap::new();
        let mut roots: Vec<String> = Vec::new();
        let mut version = source_version::start();
        let mut entries = ledger.connection.prepare(
            "SELECT path,kind,oid,len FROM entries ORDER BY ordering")?;
        let mut rows = entries.query([])?;
        while let Some(row) = rows.next()? {
            let path: String = row.get(0)?;
            let path = Path::new(&path);
            let entry = TreeEntry { kind: row.get(1)?, oid: row.get(2)?,
                len: u64::try_from(row.get::<_, i64>(3)?)? };
            entry_count += 1;
            let parts: Vec<_> = path
                .iter()
                .map(|part| part.to_str().ok_or("non-UTF-8 Git path"))
                .collect::<std::result::Result<_, _>>()?;
            let structural_dir = parts.len() == 1
                || (parts[0] == "nodes"
                    && (parts.len() == 2 || (parts.len() == 3 && parts[2] == "children")));
            let marker = path == Path::new("nodes/.keep")
                || path == Path::new("trees/.keep")
                || (parts.len() == 4
                    && parts[0] == "nodes"
                    && parts[2] == "children"
                    && parts[3] == ".keep");
            if marker {
                require(
                    entry.kind == 1 && entry.len == 0,
                    "invalid Git directory marker",
                )?;
            } else if structural_dir {
                require(entry.kind == 0, "unsafe Git structural directory")?;
            }
            match entry.kind {
                0 => {
                    if authored_path(path) {
                        source_version::add(&mut version, path, Part::Directory)?;
                    }
                }
                1 => {
                    let node_file =
                        parts.len() == 3 && parts[0] == "nodes" && parts[2] == "node.md";
                    let raw_hash = if node_file {
                        let id = parts[1];
                        let (facts, reused) = self.node(&entry, id)?;
                        memory_hits += usize::from(reused);
                        let hash = facts.raw_hash;
                        nodes.insert(id.to_owned(), facts);
                        hash
                    } else {
                        let bytes = source_error::context(
                            self.object_bytes(&entry.oid, entry.len),
                            path,
                            None,
                        )?;
                        require(marker, "unexpected Git authored file")?;
                        Sha256::digest(&bytes).into()
                    };
                    source_version::add(
                        &mut version,
                        path,
                        Part::File {
                            len: entry.len,
                            hash: &raw_hash,
                        },
                    )?;
                }
                2 => {
                    let root = parts.len() == 2 && parts[0] == "trees";
                    let parent =
                        if parts.len() == 4 && parts[0] == "nodes" && parts[2] == "children" {
                            Some(parts[1])
                        } else {
                            None
                        };
                    require(root || parent.is_some(), "unsafe Git relationship path")?;
                    let id = parts.last().ok_or("missing Git relationship UUID")?;
                    structural_guard(
                        uuid(id),
                        parent,
                        Some(id),
                        "src/lib.rs:Store.read_view/child_uuid",
                        "invalid Git relationship UUID",
                    )?;
                    let expected = if root {
                        format!("../nodes/{id}")
                    } else {
                        format!("../../{id}")
                    };
                    structural_guard(
                        entry.len == expected.len() as u64,
                        parent,
                        Some(id),
                        "src/lib.rs:safe_link",
                        "unsafe child/root link target",
                    )?;
                    let bytes =
                        source_error::context(self.object_bytes(&entry.oid, entry.len), path, None)?;
                    structural_guard(
                        bytes == expected.as_bytes(),
                        parent,
                        Some(id),
                        "src/lib.rs:safe_link",
                        "unsafe child/root link target",
                    )?;
                    if root {
                        roots.push((*id).into());
                    }
                    let hash = Sha256::digest(&bytes).into();
                    source_version::add(
                        &mut version,
                        path,
                        Part::Link {
                            len: entry.len,
                            hash: &hash,
                        },
                    )?;
                }
                _ => unreachable!(),
            }
            if (parts.len() == 2 && parts[0] == "trees" && !marker)
                || (parts.len() == 4 && parts[0] == "nodes" && parts[2] == "children" && !marker)
            {
                require(entry.kind == 2, "Git child/root must be a symlink")?;
            }
        }
        drop(rows);
        drop(entries);
        for id in nodes.keys() {
            require(ledger.has(&format!("nodes/{id}"), 0)?, "missing Git node directory")?;
        }
        let mut members = ledger.connection.prepare(
            "SELECT name FROM entries WHERE parent=?1 AND kind=2 ORDER BY name")?;
        for id in nodes.keys() {
            require(nodes[id].position.is_some(), "missing Git display position")?;
            let names = members.query_map([format!("nodes/{id}/children")], |row| row.get::<_, String>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            // Establish membership before asking for a child's position. A
            // dangling link has no node, not merely a missing display rank.
            for child in &names {
                structural_guard(nodes.contains_key(child), Some(id), Some(child),
                    "src/lib.rs:Store.read_view/child_membership", "unknown child UUID")?;
            }
            let order = crate::ordering::sort(names, |child| Ok(nodes.get(child).and_then(|node| node.position)))?;
            children.insert(id.clone(), order);
        }
        drop(members);
        require(children.len() == nodes.len(), "unknown Git relationship parent")?;
        let mut owners = ledger.connection.prepare(
            "SELECT name FROM entries WHERE parent='nodes' AND name<>'.keep'")?;
        let mut rows = owners.query([])?;
        while let Some(row) = rows.next()? {
            let id: String = row.get(0)?;
            require(nodes.contains_key(&id), "missing Git node.md")?;
        }
        drop(rows);
        drop(owners);
        for id in &roots {
            structural_guard(
                nodes.contains_key(id),
                None,
                Some(id),
                "src/lib.rs:Store.read_view/root_membership",
                "unknown root UUID",
            )?;
        }
        drop(ledger);
        crate::release_unused_memory();
        if std::env::var_os("SIFTTEXT_INDEX_PROFILE").is_some() {
            eprintln!("git-view-work entries={entry_count} nodes={} memory_hits={memory_hits} elapsed_ms={}",
                nodes.len(), started.elapsed().as_millis());
        }
        let view = Arc::new(GitView {
            version: format!("{:x}", version.finalize()),
            tree: tree.clone(),
            nodes,
            children,
            roots,
        });
        self.views.insert(tree, Arc::clone(&view));
        Ok(view)
    }

    fn node(&mut self, entry: &TreeEntry, id: &str) -> Result<(Arc<NodeFacts>, bool)> {
        let cached = self.nodes.get(&entry.oid).and_then(Weak::upgrade);
        let reused = cached.is_some();
        let facts = match cached {
            Some(facts) => facts,
            None => {
                let path = PathBuf::from(format!("nodes/{id}/node.md"));
                let bytes =
                    source_error::context(self.object_bytes(&entry.oid, entry.len), &path, None)?;
                let parsed = node_markdown(&bytes, id)?;
                let state = source_error::context(
                    State::from_fields(|key| parsed.fields.get(key).copied()),
                    &path,
                    Some("status"),
                )?;
                Arc::new(NodeFacts {
                    id: id.into(),
                    position: parsed.fields.get("position").map(|raw| serde_json::from_str(raw)).transpose()?,
                    state,
                    body_nonblank: !parsed.authored.content.trim().is_empty(),
                    raw_hash: Sha256::digest(&bytes).into(),
                    text_hash: Sha256::digest(crate::ordering::without_position(std::str::from_utf8(&bytes)?).as_bytes()).into(),
                    body_hash: Sha256::digest(parsed.authored.content.as_bytes()).into(),
                    reference_targets: parsed
                        .authored
                        .see_also
                        .into_iter()
                        .map(|reference| reference.target)
                        .collect(),
                    field_errors: intrinsic_fields(&parsed.fields),
                    migration_size: parsed.fields.contains_key("migration_size"),
                    blob: entry.oid.clone(),
                    len: entry.len,
                })
            }
        };
        structural_guard(
            facts.id == id,
            Some(id),
            Some(&facts.id),
            "src/lib.rs:Store.read_view/node_uuid_identity",
            "duplicate/mismatched node UUID",
        )?;
        self.nodes.insert(entry.oid.clone(), Arc::downgrade(&facts));
        Ok((facts, reused))
    }

    pub(crate) fn bytes(&mut self, view: &GitView, id: &str) -> Result<Vec<u8>> {
        let facts = view.nodes.get(id).ok_or("unknown Git node UUID")?;
        let path = PathBuf::from(format!("nodes/{id}/node.md"));
        let bytes = source_error::context(self.object_bytes(&facts.blob, facts.len), &path, None)?;
        require(
            <[u8; 32]>::from(Sha256::digest(&bytes)) == facts.raw_hash,
            "changed Git node facts",
        )?;
        Ok(bytes)
    }

    /// Call only after the graph helper exits: cold BPE and Bend must not overlap.
    pub(crate) fn tokens(&mut self, view: &GitView, id: &str) -> Result<usize> {
        let facts = view.nodes.get(id).ok_or("unknown Git node UUID")?;
        if let Some(&count) = self.tokens.get(&facts.body_hash) {
            return Ok(count);
        }
        let bytes = self.bytes(view, id)?;
        let parsed = node_markdown(&bytes, id)?;
        let count = crate::token_count::ordinary(&parsed.authored.content);
        self.tokens.insert(facts.body_hash, count);
        Ok(count)
    }
}
