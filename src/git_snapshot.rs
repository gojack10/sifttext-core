//! Compact immutable authored Git inputs. Object bytes are never hydrated as a Store.
use crate::{
    Graph, Policy, Result, WireNode,
    candidate_validation::{intrinsic_fields, structural_guard},
    coherent_capture::{authored_path, node_markdown},
    require, source_error,
    source_version::{self, Part},
    status::{State, Status},
    uuid,
};
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    io::{BufRead, BufReader, Read, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Stdio},
    sync::{Arc, Weak},
};

const FACT_PROCESSOR: &str = "git-node-facts-v3;authored-merkle-v2;native-markdown-v1";
const TOKEN_PROCESSOR: &str = "tiktoken-rs/0.6.0;ordinary;content/v1";

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NodeFacts {
    pub(crate) id: String,
    pub(crate) position: Option<u64>,
    #[serde(with = "state_record")]
    pub(crate) state: State,
    pub(crate) body_nonblank: bool,
    pub(crate) raw_hash: [u8; 32],
    pub(crate) text_hash: [u8; 32],
    pub(crate) body_hash: [u8; 32],
    pub(crate) reference_targets: Vec<String>,
    pub(crate) blob: String,
    pub(crate) len: u64,
    pub(crate) field_errors: u8,
}
mod state_record {
    use super::*;
    pub(super) fn serialize<S: serde::Serializer>(
        state: &State,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        (&state.status, &state.reasons).serialize(serializer)
    }
    pub(super) fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<State, D::Error> {
        let (status, reasons): (Status, BTreeMap<String, String>) =
            Deserialize::deserialize(deserializer)?;
        Ok(State { status, reasons })
    }
}

pub(crate) struct GitView {
    pub(crate) version: String,
    pub(crate) tree: String,
    pub(crate) nodes: BTreeMap<String, Arc<NodeFacts>>,
    pub(crate) children: BTreeMap<String, Vec<String>>,
    pub(crate) roots: Vec<String>,
}
impl GitView {
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
                self.alive && valid_oid(oid) && len <= crate::LIMIT as u64,
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

// A separate private file, never the optional common-Git fact cache. Only
// metadata is staged; SQLite's bounded page cache replaces the path inventory.
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
    // children.order. The source boundary already excludes NUL components.
    let ordering = path.replace('/', "\0").into_bytes();
    let len = i64::try_from(len)?;
    insert.execute(rusqlite::params![path, ordering, parent, name, kind, mode, oid, len])?;
    Ok(())
}

// Optional derived facts. No transaction spans Git, Bend, parsing or tokenization.
struct FactCache {
    connection: Connection,
    processor: String,
    token_processor: String,
}
impl FactCache {
    fn open(repo: &Path) -> Option<Self> {
        (|| -> Result<Self> {
            let common = crate::local::reader_git(repo,
                &["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
            let common = PathBuf::from(common);
            crate::real_dir(&common)?;
            let path = common.join("sifttext-git-facts.sqlite");
            for suffix in ["", "-wal", "-shm", "-journal"] {
                let file = common.join(format!("sifttext-git-facts.sqlite{suffix}"));
                match std::fs::symlink_metadata(&file) {
                    Ok(meta) => {
                        require(meta.is_file(), "unsafe Git fact cache")?;
                        #[cfg(unix)] { use std::os::unix::fs::MetadataExt;
                            require(meta.nlink() == 1, "hard-linked Git fact cache")?; }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                    Err(error) => return Err(error.into()),
                }
            }
            let connection = Connection::open_with_flags(path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
                    | rusqlite::OpenFlags::SQLITE_OPEN_CREATE
                    | rusqlite::OpenFlags::SQLITE_OPEN_NOFOLLOW)?;
            connection.busy_timeout(std::time::Duration::ZERO)?;
            connection.execute_batch("PRAGMA cache_size=-256; PRAGMA synchronous=OFF;
                CREATE TABLE IF NOT EXISTS node_facts_v1 (
                    processor TEXT NOT NULL, oid TEXT NOT NULL, data BLOB NOT NULL, checksum BLOB NOT NULL,
                    PRIMARY KEY(processor,oid)) WITHOUT ROWID;
                CREATE TABLE IF NOT EXISTS body_tokens_v1 (
                    processor TEXT NOT NULL, hash BLOB NOT NULL, count INTEGER NOT NULL, checksum BLOB NOT NULL,
                    PRIMARY KEY(processor,hash)) WITHOUT ROWID;")?;
            // Parsed headers and token counts are facts about their own inputs,
            // not endorsements by a particular graph-law or unrelated renderer.
            let parser = crate::sha256(concat!(include_str!("git_snapshot.rs"),
                include_str!("lib.rs"), include_str!("coherent_capture.rs"),
                include_str!("candidate_validation.rs"), include_str!("status.rs")).as_bytes());
            let tokenizer = crate::sha256(concat!(include_str!("token_count.rs"),
                include_str!("../Cargo.lock")).as_bytes());
            Ok(Self { connection, processor: format!("{FACT_PROCESSOR};{parser}"),
                token_processor: format!("{TOKEN_PROCESSOR};{tokenizer}") })
        })().ok()
    }
    fn node(&self, oid: &str, len: u64) -> Option<Arc<NodeFacts>> {
        (|| -> Result<Arc<NodeFacts>> {
            let record: Option<(Vec<u8>, Vec<u8>)> = self
                .connection
                .query_row(
                    "SELECT data,checksum FROM node_facts_v1 WHERE processor=?1 AND oid=?2
                 AND length(data)<=?3 AND length(checksum)=32",
                    rusqlite::params![self.processor, oid, crate::LIMIT as i64 * 2],
                    |r| Ok((r.get(0)?, r.get(1)?)),
                )
                .optional()?;
            let (data, checksum) = record.ok_or("missing Git node facts")?;
            require(
                fact_checksum(&self.processor, oid.as_bytes(), &data).as_slice() == checksum,
                "damaged Git node facts",
            )?;
            let facts: NodeFacts = serde_json::from_slice(&data)?;
            require(
                facts.blob == oid
                    && facts.len == len
                    && uuid(&facts.id)
                    && facts.field_errors <= 15
                    && facts.reference_targets.iter().all(|id| uuid(id))
                    && facts.state.reasons.len() <= 3
                    && facts.state.reasons.keys().all(|key| {
                        ["blocker", "deferred_reason", "discard_reason"].contains(&key.as_str())
                    }),
                "invalid Git node facts",
            )?;
            Ok(Arc::new(facts))
        })()
        .ok()
    }
    fn record_nodes(&mut self, nodes: &[Arc<NodeFacts>]) {
        if nodes.is_empty() { return; }
        let _ = (|| -> Result<()> {
            let tx = self
                .connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            {
                let mut insert =
                    tx.prepare_cached("INSERT OR REPLACE INTO node_facts_v1 VALUES (?1,?2,?3,?4)")?;
                for node in nodes {
                    let data = serde_json::to_vec(node.as_ref())?;
                    let checksum = fact_checksum(&self.processor, node.blob.as_bytes(), &data);
                    insert.execute(rusqlite::params![
                        self.processor,
                        node.blob,
                        data,
                        checksum.as_slice()
                    ])?;
                }
            }
            tx.commit()?;
            Ok(())
        })();
    }
    fn tokens(&self, hash: &[u8; 32]) -> Option<usize> {
        (|| -> Result<usize> {
            let (count, checksum): (i64, Vec<u8>) = self.connection.query_row(
                "SELECT count,checksum FROM body_tokens_v1 WHERE processor=?1 AND hash=?2 AND length(checksum)=32",
                rusqlite::params![self.token_processor, hash.as_slice()], |r| Ok((r.get(0)?, r.get(1)?)))?;
            require(count >= 0 && count <= crate::LIMIT as i64, "invalid token fact")?;
            require(fact_checksum(&self.token_processor, hash, &count.to_le_bytes()).as_slice() == checksum,
                "damaged token fact")?;
            Ok(count as usize)
        })().ok()
    }
    fn record_tokens(&mut self, tokens: &BTreeMap<[u8; 32], usize>) {
        let _ = (|| -> Result<()> {
            let tx = self
                .connection
                .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
            {
                let mut insert =
                    tx.prepare_cached("INSERT OR IGNORE INTO body_tokens_v1 VALUES (?1,?2,?3,?4)")?;
                for (hash, &count) in tokens {
                    let count = count as i64;
                    let checksum = fact_checksum(&self.token_processor, hash, &count.to_le_bytes());
                    insert.execute(rusqlite::params![
                        self.token_processor,
                        hash.as_slice(),
                        count,
                        checksum.as_slice()
                    ])?;
                }
            }
            tx.commit()?;
            Ok(())
        })();
    }
}
fn fact_checksum(processor: &str, key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(b"sifttext-derived-fact-v1\0");
    hash.update((processor.len() as u64).to_le_bytes());
    hash.update(processor.as_bytes());
    hash.update((key.len() as u64).to_le_bytes());
    hash.update(key);
    hash.update(data);
    hash.finalize().into()
}

pub(crate) struct GitReader {
    pub(crate) repo: PathBuf,
    pub(crate) policy_identity: String,
    batch: Option<Batch>,
    empty_blob: Option<String>,
    cache: Option<FactCache>,
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
            empty_blob: None,
            cache: FactCache::open(&repo),
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
        require(valid_oid(oid) && len <= crate::LIMIT as u64, "invalid Git object request")?;
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
                    kind == 0 || len <= crate::LIMIT as u64,
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
        // Git cannot represent an empty directory. Only an empty initialized
        // home may omit its root containers; node children need their marker.
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
        let mut nodes = BTreeMap::new();
        let mut new_facts = Vec::new();
        let (mut entry_count, mut memory_hits, mut fact_hits, mut misses) = (0usize, 0usize, 0usize, 0usize);
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
                        let (facts, reuse) = self.node(&entry, id, &mut new_facts)?;
                        match reuse { 0 => misses += 1, 1 => memory_hits += 1, _ => fact_hits += 1 }
                        let hash = facts.raw_hash;
                        nodes.insert(id.to_owned(), facts);
                        hash
                    } else {
                        let bytes = source_error::context(
                            self.object_bytes(&entry.oid, entry.len),
                            path,
                            None,
                        )?;
                        if parts.len() == 3 && parts[0] == "nodes" && parts[2] == "children.order" {
                            let text = source_error::context(
                                std::str::from_utf8(&bytes).map_err(Into::into),
                                path,
                                None,
                            )?;
                            let order: Vec<String> = text.lines().map(str::to_owned).collect();
                            structural_guard(
                                order.iter().all(|id| uuid(id)),
                                Some(parts[1]),
                                None,
                                "src/lib.rs:Store.read_view/order_uuid",
                                "malformed children.order UUID",
                            )?;
                            children.insert(parts[1].to_owned(), order);
                        } else {
                            require(marker, "unexpected Git authored file")?;
                        }
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
        let mut missing = Vec::new();
        for id in nodes.keys() {
            for (tail, kind) in [("", 0), ("/children", 0)] {
                let path = format!("nodes/{id}{tail}");
                if !ledger.has(&path, kind)? {
                    missing.push(path);
                }
            }
        }
        if !missing.is_empty() {
            return Err(source_error::PublicationProblem {
                kind: "missing_scaffolding",
                paths: missing,
                reason: "Git node containers or children.order files are missing",
                next_action: "Keep the draft. Each new leaf needs an empty children.order and an empty children/.keep so Git retains its child directory. For a node with children, restore its intended links and full order instead of inventing an order. Commit the repair after the candidate and resubmit with the same home, draft and base. Use sifttext create when creating future nodes.",
            }.into());
        }
        let mut members = ledger.connection.prepare(
            "SELECT name FROM entries WHERE parent=?1 AND kind=2 ORDER BY name")?;
        for id in nodes.keys() {
            if !children.contains_key(id) {
                require(nodes[id].position.is_some(), "missing Git children.order or per-node position")?;
                let names = members.query_map([format!("nodes/{id}/children")], |row| row.get::<_, String>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                let order = crate::ordering::sort(names, |child| Ok(nodes.get(child).and_then(|node| node.position)))?;
                children.insert(id.clone(), order);
            }
            let order = &children[id];
            // Only one owner's borrowed names are collected, never all links.
            let ordered: BTreeSet<&str> = order.iter().map(String::as_str).collect();
            structural_guard(order.len() == ordered.len(), Some(id), None,
                "src/lib.rs:Store.read_view/order_membership",
                "missing, duplicate or non-child order member")?;
            let parent = format!("nodes/{id}/children");
            let mut rows = members.query([&parent])?;
            let mut count = 0;
            while let Some(row) = rows.next()? {
                let child: String = row.get(0)?;
                structural_guard(ordered.contains(child.as_str()), Some(id), Some(&child),
                    "src/lib.rs:Store.read_view/order_membership",
                    "missing, duplicate or non-child order member")?;
                count += 1;
            }
            let mut offender = None;
            if count != order.len() {
                for child in order {
                    if !ledger.has(&format!("{parent}/{child}"), 2)? {
                        offender = Some(child);
                        break;
                    }
                }
            }
            structural_guard(count == order.len(), Some(id), offender.map(String::as_str),
                "src/lib.rs:Store.read_view/order_membership",
                "missing, duplicate or non-child order member")?;
            for child in order {
                structural_guard(nodes.contains_key(child), Some(id), Some(child),
                    "src/lib.rs:Store.read_view/child_membership", "unknown child UUID")?;
            }
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
        if let Some(cache) = &mut self.cache {
            cache.record_nodes(&new_facts);
        }
        let staged_facts = new_facts.len();
        drop(new_facts);
        crate::release_unused_memory();
        if std::env::var_os("SIFTTEXT_INDEX_PROFILE").is_some() {
            eprintln!("git-view-work entries={entry_count} nodes={} memory_hits={memory_hits} fact_hits={fact_hits} misses={misses} staged_facts={staged_facts} elapsed_ms={}",
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

    fn node(&mut self, entry: &TreeEntry, id: &str,
        new_facts: &mut Vec<Arc<NodeFacts>>) -> Result<(Arc<NodeFacts>, u8)> {
        let memory = self.nodes.get(&entry.oid).and_then(Weak::upgrade);
        let reuse = if memory.is_some() { 1 } else { 2 };
        let cached = memory.or_else(|| self.cache.as_ref()
            .and_then(|cache| cache.node(&entry.oid, entry.len)));
        let cache_miss = cached.is_none();
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
        if cache_miss && self.cache.is_some() {
            new_facts.push(Arc::clone(&facts));
        }
        self.nodes.insert(entry.oid.clone(), Arc::downgrade(&facts));
        Ok((facts, if cache_miss { 0 } else { reuse }))
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
        if let Some(count) = self
            .cache
            .as_ref()
            .and_then(|cache| cache.tokens(&facts.body_hash))
        {
            self.tokens.insert(facts.body_hash, count);
            return Ok(count);
        }
        let bytes = self.bytes(view, id)?;
        let parsed = node_markdown(&bytes, id)?;
        let count = crate::token_count::ordinary(&parsed.authored.content);
        self.tokens.insert(facts.body_hash, count);
        Ok(count)
    }
}
impl Drop for GitReader {
    fn drop(&mut self) {
        self.suspend_objects();
        if let Some(cache) = &mut self.cache {
            cache.record_tokens(&self.tokens);
        }
    }
}
