//! Disk-backed native outline cache. The caller owns freshness, serialization and crash markers.
use crate::index::residue;
use super::{cli_document, directory, processing_identity};
use crate::{
    Result, outline_artifacts,
    outline_rows::{Coverage, Forest, Node},
    require, sha256,
};
use rusqlite::{Connection, OptionalExtension, params};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
    sync::OnceLock,
};

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS outline_cache (
    path TEXT PRIMARY KEY, node_id TEXT NOT NULL, full INTEGER NOT NULL,
    sidecar INTEGER NOT NULL, input_fingerprint TEXT NOT NULL, processing TEXT NOT NULL,
    bytes_hash TEXT NOT NULL, tokens INTEGER,
    dev INTEGER NOT NULL, ino INTEGER NOT NULL, size INTEGER NOT NULL,
    mtime INTEGER NOT NULL, mtime_ns INTEGER NOT NULL,
    ctime INTEGER NOT NULL, ctime_ns INTEGER NOT NULL);
CREATE UNIQUE INDEX IF NOT EXISTS outline_cache_owner_view ON outline_cache(node_id,full,sidecar);
CREATE TABLE IF NOT EXISTS outline_documents (path TEXT PRIMARY KEY, bytes BLOB NOT NULL);
CREATE TABLE IF NOT EXISTS outline_state (
    singleton INTEGER PRIMARY KEY CHECK(singleton=1), processing TEXT NOT NULL,
    node_count INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS outline_inputs (id TEXT PRIMARY KEY, fingerprint TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS outline_parents (source TEXT NOT NULL, target TEXT NOT NULL, PRIMARY KEY(source,target));
CREATE INDEX IF NOT EXISTS outline_parents_target ON outline_parents(target);
CREATE TABLE IF NOT EXISTS outline_dependencies (singleton INTEGER PRIMARY KEY CHECK(singleton=1),
    processing TEXT NOT NULL, roots_hash TEXT NOT NULL, invalid INTEGER NOT NULL);
CREATE TEMP TABLE IF NOT EXISTS outline_selected (id TEXT PRIMARY KEY);
CREATE TEMP TABLE IF NOT EXISTS outline_affected (id TEXT PRIMARY KEY);
";
const COLUMNS: &str = "path,node_id,full,sidecar,input_fingerprint,processing,bytes_hash,tokens,dev,ino,size,mtime,mtime_ns,ctime,ctime_ns";

fn processing() -> &'static str {
    static ID: OnceLock<String> = OnceLock::new();
    ID.get_or_init(processing_identity)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Signature {
    dev: u64,
    ino: u64,
    size: u64,
    mtime: i64,
    mtime_ns: i64,
    ctime: i64,
    ctime_ns: i64,
}
impl Signature {
    fn from(m: &fs::Metadata) -> Self {
        Self {
            dev: m.dev(),
            ino: m.ino(),
            size: m.len(),
            mtime: m.mtime(),
            mtime_ns: m.mtime_nsec(),
            ctime: m.ctime(),
            ctime_ns: m.ctime_nsec(),
        }
    }
}
struct Cached {
    path: String,
    node_id: String,
    full: bool,
    sidecar: bool,
    input: String,
    processing: String,
    hash: String,
    tokens: Option<usize>,
    signature: Signature,
}
impl Cached {
    fn read(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
        fn unsigned(row: &rusqlite::Row<'_>, i: usize) -> rusqlite::Result<u64> {
            u64::try_from(row.get::<_, i64>(i)?).map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    i,
                    rusqlite::types::Type::Integer,
                    Box::new(e),
                )
            })
        }
        let tokens = row
            .get::<_, Option<i64>>(7)?
            .map(usize::try_from)
            .transpose()
            .map_err(|e| {
                rusqlite::Error::FromSqlConversionFailure(
                    7,
                    rusqlite::types::Type::Integer,
                    Box::new(e),
                )
            })?;
        Ok(Self {
            path: row.get(0)?,
            node_id: row.get(1)?,
            full: row.get(2)?,
            sidecar: row.get(3)?,
            input: row.get(4)?,
            processing: row.get(5)?,
            hash: row.get(6)?,
            tokens,
            signature: Signature {
                dev: unsigned(row, 8)?,
                ino: unsigned(row, 9)?,
                size: unsigned(row, 10)?,
                mtime: row.get(11)?,
                mtime_ns: row.get(12)?,
                ctime: row.get(13)?,
                ctime_ns: row.get(14)?,
            },
        })
    }
    fn matches(&self, connection: &Connection, root: &Path, input: &str) -> Result<bool> {
        validate_cache_path(self)?;
        Ok(self.processing == processing()
            && self.input == input
            && if virtual_path(&self.path) {
                connection.query_row("SELECT EXISTS(SELECT 1 FROM outline_documents WHERE path=?1)", [&self.path], |r| r.get(0))?
            } else { signature(root, &self.path)? == Some(self.signature) })
    }

    fn reusable(
        &mut self,
        connection: &Connection,
        root: &Path,
        input: &str,
        force: bool,
    ) -> Result<bool> {
        if !force { return self.matches(connection, root, input) }
        if virtual_path(&self.path) {
            return Ok(self.processing == processing() && self.input == input
                && read_cached(connection, root, self).is_ok());
        }
        validate_cache_path(self)?;
        if self.processing != processing() || self.input != input {
            return Ok(false);
        }
        let Some(before) = self.verified_bytes(root)? else { return Ok(false) };
        if self.signature != before {
            self.signature = before;
            remember(connection, root, self)?;
        }
        Ok(true)
    }

    // Ownership survives processor upgrades, but never changed file bytes.
    fn verified_bytes(&self, root: &Path) -> Result<Option<Signature>> {
        validate_cache_path(self)?;
        let parent = root.join(&self.path).parent().ok_or("missing outline parent")?.to_owned();
        let Some(before) = signature(root, &self.path)? else {
            return Ok(None);
        };
        let parent_id = directory(&parent)?;
        if before.size != self.signature.size {
            return Ok(None);
        }
        let mut file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(root.join(&self.path))?;
        require(
            Signature::from(&file.metadata()?) == before,
            "changed outline during verification",
        )?;
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 32 * 1024];
        let mut size = 0u64;
        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            size = size.checked_add(n as u64).ok_or("outline size overflow")?;
            require(size <= before.size, "changed outline during verification")?;
            hash.update(&buffer[..n]);
        }
        require(
            Signature::from(&file.metadata()?) == before
                && signature(root, &self.path)? == Some(before)
                && directory(&parent)? == parent_id,
            "changed outline during verification",
        )?;
        if size != before.size || format!("{:x}", hash.finalize()) != self.hash {
            return Ok(None);
        }
        Ok(Some(before))
    }

    fn retire_file(&self, connection: &Connection, root: &Path) -> Result<()> {
        if virtual_path(&self.path) {
            connection.execute("DELETE FROM outline_documents WHERE path=?1", [&self.path])?;
        } else if signature(root, &self.path)?.is_some() {
            require(self.verified_bytes(root)?.is_some(), "preserved obsolete outline: cached bytes no longer match")?;
            fs::remove_file(root.join(&self.path))?;
        }
        Ok(())
    }
}

fn name(full: bool, sidecar: bool) -> &'static str {
    match (full, sidecar) {
        (false, false) => "outline.lean",
        (true, false) => "outline.full.lean",
        (false, true) => "outline.lean.meta.json",
        (true, true) => "outline.full.lean.meta.json",
    }
}
fn validate_path(path: &str, node_id: &str, full: bool, sidecar: bool) -> Result<()> {
    require(crate::uuid(node_id), "malformed cached outline owner")?;
    let relative = path.strip_prefix("nodes/")
        .or_else(|| path.strip_prefix(".sifttext/outlines/"));
    require(
        relative.and_then(|p| p.rsplit_once('/')) == Some((node_id, name(full, sidecar))),
        "malformed cached outline path",
    )
}
fn validate_cache_path(c: &Cached) -> Result<()> {
    validate_path(&c.path, &c.node_id, c.full, c.sidecar)?;
    require(
        c.sidecar == c.tokens.is_none(),
        "malformed cached outline token count",
    )
}

/// Check every parent without following symlinks. Missing generated paths are repairable;
/// malformed types and linked files are explicit refusals, never silently omitted artifacts.
fn signature(root: &Path, relative: &str) -> Result<Option<Signature>> {
    directory(root)?;
    let relative = Path::new(relative);
    let mut parent = root.to_owned();
    for component in relative
        .parent()
        .ok_or("missing outline parent")?
        .components()
    {
        require(
            matches!(component, std::path::Component::Normal(_)),
            "invalid outline path",
        )?;
        parent.push(component);
        match fs::symlink_metadata(&parent) {
            Ok(m) => require(m.is_dir(), "unsafe outline directory")?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        }
    }
    leaf_signature(&root.join(relative))
}
fn leaf_signature(path: &Path) -> Result<Option<Signature>> {
    match fs::symlink_metadata(path) {
        Ok(m) => {
            require(
                m.is_file() && m.nlink() == 1,
                "unsafe generated outline file",
            )?;
            Ok(Some(Signature::from(&m)))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
fn exists(connection: &Connection, table: &str) -> Result<bool> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [table],
        |r| r.get(0),
    )?)
}
fn cached(connection: &Connection, path: &str) -> Result<Option<Cached>> {
    Ok(connection
        .query_row(
            &format!("SELECT {COLUMNS} FROM outline_cache WHERE path=?1"),
            [path],
            Cached::read,
        )
        .optional()?)
}

/// Completed generation state, not a whole-forest generated-file inventory.
/// Requested artifacts and mutation destinations are still checked when used.
pub(crate) fn indexed_current(connection: &Connection) -> Result<bool> {
    if !exists(connection, "outline_state")? { return Ok(false) }
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM outline_state WHERE singleton=1 AND processing=?1
            AND node_count=(SELECT count(*) FROM nodes))",
        [processing()], |r| r.get(0))?)
}

/// Hydrate only displayed draft row fields; body text is never an outline dependency.
/// Optional fields present with null/non-text values are malformed, not missing metadata.
const COMPACT: &str = "SELECT id,name,status,micro_summary,children,
        CASE WHEN json_type(record)='object'
          AND (json_type(record,'$.warnings') IS NULL OR json_type(record,'$.warnings')='text')
          AND (json_type(record,'$.blocker') IS NULL OR json_type(record,'$.blocker')='text')
          AND (json_type(record,'$.deferred_reason') IS NULL OR json_type(record,'$.deferred_reason')='text')
          AND (json_type(record,'$.discard_reason') IS NULL OR json_type(record,'$.discard_reason')='text')
        THEN json_object('warnings',json_extract(record,'$.warnings'),
            'blocker',json_extract(record,'$.blocker'),'deferred_reason',json_extract(record,'$.deferred_reason'),
            'discard_reason',json_extract(record,'$.discard_reason')) END
        FROM nodes";
fn compact_node(row: &rusqlite::Row<'_>) -> Result<Node> {
    let fields: Option<String> = row.get(5)?;
    let mut fields: Value =
        serde_json::from_str(&fields.ok_or("malformed outline metadata fields")?)?;
    for (key, column) in [("id", 0), ("name", 1), ("status", 2), ("micro_summary", 3)] {
        fields[key] = json!(row.get::<_, String>(column)?);
    }
    fields["children"] = serde_json::from_str(&row.get::<_, String>(4)?)?;
    let node: Node = serde_json::from_value(fields)?;
    require(
        crate::uuid(&node.id) && node.children.iter().all(|id| crate::uuid(id)),
        "malformed outline UUID metadata",
    )?;
    Ok(node)
}
fn compact_nodes(connection: &Connection) -> Result<Vec<Node>> {
    let mut query = connection.prepare(&format!("{COMPACT} WHERE id IN (SELECT id FROM outline_selected) ORDER BY id"))?;
    let mut rows = query.query([])?;
    let mut nodes = Vec::new();
    while let Some(row) = rows.next()? { nodes.push(compact_node(row)?); }
    Ok(nodes)
}

// The draft projector never uses body text. Only the status-active reason and
// nonempty warnings can appear in rows; inactive metadata is not a dependency.
fn row_fingerprint(node: &Node) -> Result<String> {
    let mut row = serde_json::to_value(node)?;
    row.as_object_mut().ok_or("missing compact node")?.remove("content");
    let status = row["status"].as_str().ok_or("missing compact status")?.to_owned();
    for (field, active) in [("blocker", "stuck"), ("deferred_reason", "deferred"), ("discard_reason", "discarded")] {
        if status != active { row.as_object_mut().unwrap().remove(field); }
    }
    if row["warnings"].as_str() == Some("") { row["warnings"] = Value::Null; }
    Ok(sha256(&serde_json::to_vec(&row)?))
}

fn prefix(root: &Path, id: &str) -> Result<String> {
    directory(root)?;
    match fs::symlink_metadata(root.join("nodes")) {
        Ok(m) => require(m.is_dir(), "unsafe nodes directory")?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(format!(".sifttext/outlines/{id}")),
        Err(e) => return Err(e.into()),
    }
    match fs::symlink_metadata(root.join("nodes").join(id)) {
        Ok(m) => {
            require(m.is_dir(), "unsafe outline owner directory")?;
            Ok(format!("nodes/{id}"))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(format!(".sifttext/outlines/{id}")),
        Err(e) => Err(e.into()),
    }
}

fn remember(connection: &Connection, root: &Path, c: &Cached) -> Result<()> {
    require(
        virtual_path(&c.path) || signature(root, &c.path)? == Some(c.signature),
        "changed published outline",
    )?;
    let s = &c.signature;
    connection.execute("INSERT OR REPLACE INTO outline_cache VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)", params![c.path,c.node_id,c.full,c.sidecar,c.input,processing(),c.hash,c.tokens.map(i64::try_from).transpose()?,i64::try_from(s.dev)?,i64::try_from(s.ino)?,i64::try_from(s.size)?,s.mtime,s.mtime_ns,s.ctime,s.ctime_ns])?;
    Ok(())
}
fn virtual_path(path: &str) -> bool { path.starts_with(".sifttext/outlines/") }
fn replace(connection: &Connection, root: &Path, relative: &str, bytes: &[u8]) -> Result<Signature> {
    if virtual_path(relative) {
        connection.execute("INSERT OR REPLACE INTO outline_documents VALUES (?1,?2)", params![relative, bytes])?;
        return Ok(Signature { dev: 0, ino: 0, size: bytes.len() as u64, mtime: 0, mtime_ns: 0, ctime: 0, ctime_ns: 0 });
    }
    let target = root.join(relative);
    let parent = target.parent().ok_or("missing output directory")?;
    let parent_identity = directory(parent)?;
    let before = signature(root, relative)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    require(
        directory(parent)? == parent_identity && signature(root, relative)? == before,
        "changed outline destination",
    )?;
    let file = temp.persist(&target).map_err(|e| e.error)?;
    // Rebuildable outputs: SQLite's durable dirty marker and boot epoch fence this rename.
    let written = Signature::from(&file.metadata()?);
    require(
        signature(root, relative)? == Some(written),
        "changed published outline",
    )?;
    Ok(written)
}

struct Publisher<'a> {
    connection: &'a Connection,
    root: &'a Path,
    donor: Option<(&'a Connection, &'a Path)>,
    force: bool,
    written: u64,
}
impl Publisher<'_> {
    fn reuse_donor(&mut self, id: &str, path: &str, full: bool, sidecar: bool, input: &str) -> Result<Option<Cached>> {
        let Some((connection, root)) = self.donor else { return Ok(None) };
        let read = || -> Result<Cached> {
            let c = connection.query_row(
                &format!("SELECT {COLUMNS} FROM outline_cache WHERE node_id=?1 AND full=?2 AND sidecar=?3"),
                params![id, full, sidecar], Cached::read)?;
            require(c.matches(connection, root, input)?, "incompatible donor outline")?;
            Ok(c)
        };
        let Ok(mut c) = read() else { return Ok(None) };
        if virtual_path(path) || virtual_path(&c.path) {
            let Ok(text) = read_cached(connection, root, &c) else { return Ok(None) };
            if sha256(text.as_bytes()) != c.hash { return Ok(None) }
            c.signature = replace(self.connection, self.root, path, text.as_bytes())?;
            c.path = path.to_owned();
            remember(self.connection, self.root, &c)?;
            self.written += 1;
            return Ok(Some(c));
        }
        // Donor failures are misses, but unsafe target paths must still refuse.
        let before = signature(self.root, path)?;
        let target = self.root.join(path);
        let parent = target.parent().ok_or("missing output directory")?;
        let parent_identity = directory(parent)?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        let mut verify = || -> Result<()> {
            let mut source = fs::OpenOptions::new().read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).open(root.join(&c.path))?;
            let metadata = source.metadata()?;
            require(metadata.is_file() && metadata.nlink() == 1 && Signature::from(&metadata) == c.signature, "changed donor outline")?;
            let mut hash = Sha256::new();
            let mut size = 0u64;
            let mut buffer = [0u8; 32 * 1024];
            loop {
                let n = source.read(&mut buffer)?;
                if n == 0 { break }
                size = size.checked_add(n as u64).ok_or("outline size overflow")?;
                require(size <= c.signature.size, "changed donor outline")?;
                hash.update(&buffer[..n]);
                temp.write_all(&buffer[..n])?;
            }
            require(size == c.signature.size && format!("{:x}", hash.finalize()) == c.hash
                && signature(root, &c.path)? == Some(c.signature), "changed donor outline bytes")?;
            Ok(())
        };
        if verify().is_err() { return Ok(None) }
        require(directory(parent)? == parent_identity && signature(self.root, path)? == before, "changed outline destination")?;
        let file = temp.persist(&target).map_err(|e| e.error)?;
        c.path = path.to_owned();
        c.signature = Signature::from(&file.metadata()?);
        remember(self.connection, self.root, &c)?;
        self.written += 1;
        Ok(Some(c))
    }
    fn document(
        &mut self,
        id: &str,
        prefix: &str,
        full: bool,
        input: &str,
        render: impl FnOnce() -> Result<String>,
    ) -> Result<(String, usize)> {
        let path = format!("{prefix}/{}", name(full, false));
        let native = prefix.starts_with("nodes/");
        // Full is always processed before shallow for every visited owner.
        if full {
            // Retire cached wrong-prefix artifacts before the unique owner/view
            // index can replace their records. Never enumerate untracked files.
            let mut query = self.connection.prepare(&format!("SELECT {COLUMNS} FROM outline_cache WHERE node_id=?1"))?;
            let mut obsolete = Vec::new();
            for row in query.query_map([id], Cached::read)? {
                let c = row?;
                validate_cache_path(&c)?;
                if c.path.starts_with("nodes/") != native { obsolete.push(c); }
            }
            drop(query);
            for old in obsolete {
                old.retire_file(self.connection, self.root)?;
                self.connection.execute("DELETE FROM outline_cache WHERE path=?1", [&old.path])?;
            }
        }
        if let Some(mut c) = cached(self.connection, &path)?
            && c.reusable(self.connection, self.root, input, self.force)?
        {
            return Ok((c.hash, c.tokens.ok_or("missing cached exact outline cost")?));
        }
        if let Some(c) = self.reuse_donor(id, &path, full, false, input)? {
            return Ok((c.hash, c.tokens.ok_or("missing cached exact outline cost")?));
        }
        let text = render()?;
        let (hash, tokens) = outline_artifacts::cost(&text);
        let signature = replace(self.connection, self.root, &path, text.as_bytes())?;
        let c = Cached {
            path,
            node_id: id.to_owned(),
            full,
            sidecar: false,
            input: input.to_owned(),
            processing: processing().to_owned(),
            hash: hash.clone(),
            tokens: Some(tokens),
            signature,
        };
        remember(self.connection, self.root, &c)?;
        self.written += 1;
        Ok((hash, tokens))
    }
    fn sidecar(
        &mut self,
        id: &str,
        prefix: &str,
        full: bool,
        input: &str,
        meta: &Value,
    ) -> Result<()> {
        let path = format!("{prefix}/{}", name(full, true));
        if let Some(mut c) = cached(self.connection, &path)?
            && c.reusable(self.connection, self.root, input, self.force)?
        {
            return Ok(());
        }
        if self.reuse_donor(id, &path, full, true, input)?.is_some() {
            return Ok(());
        }
        let bytes = serde_json::to_string(meta)? + "\n";
        let signature = replace(self.connection, self.root, &path, bytes.as_bytes())?;
        let c = Cached {
            path,
            node_id: id.to_owned(),
            full,
            sidecar: true,
            input: input.to_owned(),
            processing: processing().to_owned(),
            hash: sha256(bytes.as_bytes()),
            tokens: None,
            signature,
        };
        remember(self.connection, self.root, &c)?;
        self.written += 1;
        Ok(())
    }
    fn clean(self) -> Result<u64> {
        loop {
            // Cleanup memory is bounded even when every owner was deleted.
            let owner: Option<String> = self.connection.query_row(
                "SELECT node_id FROM outline_cache WHERE node_id NOT IN (SELECT id FROM nodes) ORDER BY node_id LIMIT 1",
                [], |r| r.get(0)).optional()?;
            let Some(id) = owner else { break };
            require(crate::uuid(&id), "malformed cached outline owner")?;
            let mut query = self.connection.prepare(&format!("SELECT {COLUMNS} FROM outline_cache WHERE node_id=?1"))?;
            let obsolete = query.query_map([&id], Cached::read)?.collect::<rusqlite::Result<Vec<_>>>()?;
            drop(query);
            for c in &obsolete { validate_cache_path(c)?; }
            match fs::symlink_metadata(self.root.join("nodes").join(&id)) {
                Ok(_) => residue::retire(self.connection, self.root, &id)?,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
                Err(e) => return Err(e.into()),
            }
            for c in obsolete.iter().filter(|c| c.path.starts_with(".sifttext/")) {
                c.retire_file(self.connection, self.root)?;
            }
            self.connection.execute("DELETE FROM outline_cache WHERE node_id=?1", [&id])?;
        }
        // Do not remove authored directories or unknown generated resources.
        Ok(self.written)
    }
}

// Finite global structural checks are diagnostic only, not a replacement for
// publication's Bend law gate. UNION makes rootless/unreachable cycles terminate.
fn structural_diagnostic(connection: &Connection) -> Result<Option<String>> {
    Ok(connection.query_row("SELECT id||': '||detail FROM (
        SELECT source AS id,detail FROM relationship_mismatches
        UNION ALL SELECT c.source,'child link names missing node' FROM child_links c LEFT JOIN nodes n ON n.id=c.target WHERE n.id IS NULL
        UNION ALL SELECT target,'child has more than one parent' FROM child_links GROUP BY target HAVING count(*)>1
        UNION ALL SELECT r.id,'root shortcut names missing node' FROM tree_roots r LEFT JOIN nodes n ON n.id=r.id WHERE n.id IS NULL
        UNION ALL SELECT r.id,'root shortcut points to a child' FROM tree_roots r WHERE EXISTS(SELECT 1 FROM child_links c WHERE c.target=r.id)
        UNION ALL SELECT n.id,'parentless node has no root shortcut' FROM nodes n LEFT JOIN tree_roots r ON r.id=n.id WHERE r.id IS NULL AND NOT EXISTS(SELECT 1 FROM child_links c WHERE c.target=n.id)
        UNION ALL SELECT n.id,'cycle in supplied forest' FROM nodes n WHERE n.id NOT IN (
            WITH RECURSIVE reachable(id) AS (SELECT n.id FROM nodes n WHERE NOT EXISTS(SELECT 1 FROM child_links c WHERE c.target=n.id)
                UNION SELECT c.target FROM child_links c JOIN reachable r ON c.source=r.id) SELECT id FROM reachable)
        ) ORDER BY id,detail LIMIT 1", [], |r| r.get(0)).optional()?)
}

fn roots_hash(connection: &Connection) -> Result<String> {
    let mut hash = Sha256::new();
    let mut query = connection.prepare("SELECT id FROM tree_roots ORDER BY id")?;
    for id in query.query_map([], |r| r.get::<_, String>(0))? { hash.update(id?.as_bytes()); }
    Ok(format!("{:x}", hash.finalize()))
}
fn remember_input(connection: &Connection, node: &Node, fingerprint: &str) -> Result<()> {
    connection.execute("INSERT INTO outline_inputs VALUES (?1,?2) ON CONFLICT(id) DO UPDATE SET fingerprint=excluded.fingerprint WHERE fingerprint<>excluded.fingerprint", params![node.id, fingerprint])?;
    Ok(())
}
fn full_dependency(connection: &Connection, id: &str) -> Result<(String, usize)> {
    let (input, tokens): (String, i64) = connection.query_row("SELECT input_fingerprint,tokens FROM outline_cache WHERE node_id=?1 AND full=1 AND sidecar=0 AND processing=?2", params![id, processing()], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok((input, usize::try_from(tokens)?))
}

/// Incremental publication within the caller's whole-view transaction. Artifact
/// currency is checked before entry; force always revisits every required output.
pub(crate) fn refresh_indexed_from(
    connection: &Connection,
    root: &Path,
    version: &str,
    changed: &BTreeSet<String>,
    force: bool,
    donor: Option<(&Connection, &Path)>,
) -> Result<u64> {
    require(!version.trim().is_empty(), "missing authored outline version")?;
    connection.execute_batch(SCHEMA)?;
    connection.execute_batch("DELETE FROM outline_selected; DELETE FROM outline_affected;")?;
    let state: Option<(String, String, bool)> = connection.query_row(
        "SELECT processing,roots_hash,invalid FROM outline_dependencies WHERE singleton=1", [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?))).optional()?;
    let roots = roots_hash(connection)?;
    let previous_valid = state.as_ref().is_some_and(|(p, _, invalid)| p == processing() && !invalid);
    let all = force || !previous_valid;
    if !all {
        for id in changed { connection.execute("INSERT OR IGNORE INTO outline_selected VALUES (?1)", [id])?; }
    }
    let mut relevant = BTreeSet::new();
    if !all {
        let mut query = connection.prepare(&format!("{COMPACT} WHERE id IN (SELECT id FROM outline_selected) ORDER BY id"))?;
        let mut rows = query.query([])?;
        while let Some(row) = rows.next()? {
            let node = compact_node(row)?;
            let fingerprint = row_fingerprint(&node)?;
            let before: Option<String> = connection.query_row("SELECT fingerprint FROM outline_inputs WHERE id=?1", [&node.id], |r| r.get(0)).optional()?;
            if before.as_deref() != Some(&fingerprint) { relevant.insert(node.id.clone()); }
            remember_input(connection, &node, &fingerprint)?;
        }
    }
    // Deleted owners and old/new parent edges are dependencies even without a
    // new row. This comparison is SQL-only, not filesystem revalidation.
    let edges_changed: bool = connection.query_row("SELECT EXISTS(
        SELECT source,target FROM outline_parents EXCEPT SELECT source,target FROM child_links)
        OR EXISTS(SELECT source,target FROM child_links EXCEPT SELECT source,target FROM outline_parents)
        OR EXISTS(SELECT 1 FROM outline_inputs i LEFT JOIN nodes n ON n.id=i.id WHERE n.id IS NULL)", [], |r| r.get(0))?;
    let roots_changed = state.as_ref().is_none_or(|(_, old, _)| old != &roots);
    let mismatch: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM relationship_mismatches)", [], |r| r.get(0))?;
    if !all && relevant.is_empty() && !edges_changed && !roots_changed && !mismatch {
        trace_outline(0, 0, 0, true, false);
        return Ok(0);
    }
    let diagnostic = structural_diagnostic(connection)?;
    let invalid = diagnostic.is_some();
    let all = all || invalid;
    if all {
        connection.execute("INSERT INTO outline_affected SELECT id FROM nodes", [])?;
    } else {
        for id in relevant.iter().chain(changed.iter()) {
            connection.execute("INSERT OR IGNORE INTO outline_affected VALUES (?1)", [id])?;
        }
        // Follow each actual old and new ancestry separately; UNION is finite
        // even when repairing a formerly invalid graph.
        connection.execute_batch("WITH RECURSIVE ancestors(id) AS (
            SELECT id FROM outline_affected UNION SELECT p.source FROM outline_parents p JOIN ancestors a ON p.target=a.id
            UNION SELECT c.source FROM child_links c JOIN ancestors a ON c.target=a.id)
            INSERT OR IGNORE INTO outline_affected SELECT id FROM ancestors;")?;
    }
    let mut publisher = Publisher { connection, root, donor, force, written: 0 };
    let mut component_count = 0u64;
    let mut forest_nodes = 0u64;
    let mut owners = 0u64;
    if let Some(diagnostic) = diagnostic {
        // One global, streaming compact hash keeps every invalid view coherent,
        // including dangling links and rootless cycles. No prose/node inventory.
        let mut hash = Sha256::new();
        hash.update(processing().as_bytes());
        hash.update(roots.as_bytes());
        hash.update(diagnostic.as_bytes());
        let mut query = connection.prepare(&format!("{COMPACT} ORDER BY id"))?;
        let mut rows = query.query([])?;
        while let Some(row) = rows.next()? {
            let node = compact_node(row)?;
            let fingerprint = row_fingerprint(&node)?;
            hash.update(node.id.as_bytes());
            hash.update(fingerprint.as_bytes());
            remember_input(connection, &node, &fingerprint)?;
        }
        let mut links = connection.prepare("SELECT source,target FROM child_links ORDER BY source,target")?;
        for link in links.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
            let (source, target) = link?;
            hash.update(source.as_bytes()); hash.update(target.as_bytes());
        }
        let compact_identity = format!("{:x}", hash.finalize());
        let mut ids = connection.prepare("SELECT id FROM nodes ORDER BY id")?;
        for id in ids.query_map([], |r| r.get::<_, String>(0))? {
            let id = id?;
            owners += 1;
            let prefix = prefix(root, &id)?;
            let mut full_tokens = 0;
            for full in [true, false] {
                let input = sha256(&serde_json::to_vec(&(&compact_identity, &id, full))?);
                let document = json!({"coverage":"invalid_draft","requested_coverage":if full { "full" } else { "shallow" },"status":"invalid_draft","owner":id,"structural_diagnostic":diagnostic,"source_version":input,"source_version_scope":"outline_view","path_template":crate::outline_rows::PATH_TEMPLATE,"token_encoding":"o200k_base","rows":[]});
                let (hash, tokens) = publisher.document(&id, &prefix, full, &input, || Ok(outline_artifacts::owned_bytes(document)?))?;
                let mut meta = json!({"hash":hash,"tokens":tokens,"encoding":"o200k_base"});
                if full { full_tokens = tokens; } else {
                    meta["full_view_tokens"] = json!(full_tokens);
                    meta["branch_full_tokens"] = json!({});
                }
                publisher.sidecar(&id, &prefix, full, &input, &meta)?;
            }
        }
    } else {
        // Only actual affected root components are hydrated. Forest::draft keeps
        // its existing draft projection/guards; SQL guards above do not accept.
        let mut roots_query = connection.prepare("SELECT id FROM tree_roots WHERE id IN (SELECT id FROM outline_affected) ORDER BY id")?;
        for component in roots_query.query_map([], |r| r.get::<_, String>(0))? {
            let component = component?;
            connection.execute("DELETE FROM outline_selected", [])?;
            connection.execute("WITH RECURSIVE component(id) AS (SELECT ?1 UNION SELECT c.target FROM child_links c JOIN component p ON c.source=p.id) INSERT INTO outline_selected SELECT id FROM component", [&component])?;
            let nodes = compact_nodes(connection)?;
            component_count += 1;
            forest_nodes += nodes.len() as u64;
            // ponytail: memory is bounded by the largest root component, not by
            // the store. A single giant component remains the scaling ceiling.
            let forest = Forest::draft(&nodes)?;
            let by_id: BTreeMap<_, _> = nodes.iter().map(|n| (n.id.as_str(), n)).collect();
            let mut preorder = Vec::with_capacity(nodes.len());
            let mut pending = vec![component.as_str()];
            while let Some(id) = pending.pop() {
                preorder.push(id);
                pending.extend(by_id[id].children.iter().rev().map(String::as_str));
            }
            let affected: BTreeSet<String> = if all { BTreeSet::new() } else {
                connection.prepare("SELECT id FROM outline_affected WHERE id IN (SELECT id FROM outline_selected)")?
                    .query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?
            };
            let mut identities = BTreeMap::new();
            let mut costs = BTreeMap::new();
            for id in preorder.iter().rev().copied().filter(|id| all || affected.contains(*id)) {
                let row = forest.project(id, Coverage::Shallow, version)?;
                let children: Vec<String> = by_id[id].children.iter().map(|child| {
                    identities.get(child.as_str()).cloned().map(Ok)
                        .unwrap_or_else(|| full_dependency(connection, child).map(|(input, _)| input))
                }).collect::<Result<_>>()?;
                // Exactly Forest::full_identities' existing recurrence, but only
                // dirty owners. Unchanged child inputs come from the ledger.
                let input = sha256(&serde_json::to_vec(&(processing(), &row.rows[0], children))?);
                let prefix = prefix(root, id)?;
                let (hash, tokens) = publisher.document(id, &prefix, true, &input, || {
                    Ok(outline_artifacts::response_bytes(&forest.project(id, Coverage::Full, &input)?)?)
                })?;
                publisher.sidecar(id, &prefix, true, &input, &json!({"hash":hash,"tokens":tokens,"encoding":"o200k_base"}))?;
                identities.insert(id, input);
                costs.insert(id, tokens);
                owners += 1;
            }
            for id in preorder.iter().copied().filter(|id| all || affected.contains(*id)) {
                let prefix = prefix(root, id)?;
                let mut expected = serde_json::to_value(forest.project(id, Coverage::Shallow, &identities[id])?)?;
                expected.as_object_mut().ok_or("missing outline object")?.remove("source_version");
                let mut branches = serde_json::Map::new();
                for row in expected["rows"].as_array_mut().ok_or("missing outline rows")? {
                    let child = row["id"].as_str().ok_or("missing row id")?.to_owned();
                    let tokens = match costs.get(child.as_str()) { Some(tokens) => *tokens, None => full_dependency(connection, &child)?.1 };
                    row["full_view_tokens"] = json!(tokens);
                    if child != id { branches.insert(child, json!(tokens)); }
                }
                let input = sha256(&serde_json::to_vec(&(processing(), &expected))?);
                expected["source_version"] = json!(input);
                let (hash, tokens) = publisher.document(id, &prefix, false, &input, || Ok(outline_artifacts::owned_bytes(expected)?))?;
                publisher.sidecar(id, &prefix, false, &input, &json!({"hash":hash,"tokens":tokens,"encoding":"o200k_base","full_view_tokens":costs[id],"branch_full_tokens":branches}))?;
            }
            // Cache row dependencies incrementally alongside the complete view.
            if all {
                for node in &nodes { remember_input(connection, node, &row_fingerprint(node)?)?; }
            }
        }
    }
    let written = publisher.clean()?;
    connection.execute_batch("DELETE FROM outline_inputs WHERE id NOT IN (SELECT id FROM nodes);
        DELETE FROM outline_parents WHERE (source,target) NOT IN (SELECT source,target FROM child_links);
        INSERT OR IGNORE INTO outline_parents SELECT source,target FROM child_links;")?;
    let count: i64 = connection.query_row("SELECT count(*) FROM nodes", [], |r| r.get(0))?;
    connection.execute("INSERT INTO outline_state VALUES(1,?1,?2) ON CONFLICT(singleton) DO UPDATE SET processing=excluded.processing,node_count=excluded.node_count WHERE processing<>excluded.processing OR node_count<>excluded.node_count", params![processing(), count])?;
    connection.execute("INSERT OR REPLACE INTO outline_dependencies VALUES (1,?1,?2,?3)", params![processing(), roots, invalid])?;
    trace_outline(component_count, forest_nodes, owners, false, invalid);
    Ok(written)
}

fn trace_outline(components: u64, forest_nodes: u64, owners: u64, fast_skip: bool, invalid: bool) {
    if std::env::var_os("SIFTTEXT_RETRIEVAL_TRACE").is_some() {
        eprintln!("outline-work {}", json!({"components":components,"forest_nodes":forest_nodes,"owners_examined":owners,"fast_skip":fast_skip,"invalid":invalid}));
    }
}

fn read_document(connection: &Connection, root: &Path, id: &str, full: bool) -> Result<String> {
    let row: Cached = connection.query_row(
        &format!("SELECT {COLUMNS} FROM outline_cache WHERE node_id=?1 AND full=?2 AND sidecar=0"),
        params![id, full],
        Cached::read,
    ).optional()?.ok_or("missing requested outline artifact; run sifttext index STORE --repair")?;
    read_cached(connection, root, &row)
}
fn read_cached(connection: &Connection, root: &Path, row: &Cached) -> Result<String> {
    validate_cache_path(row)?;
    require(
        row.matches(connection, root, &row.input)?,
        "stale requested outline artifact; run sifttext index STORE --repair",
    )?;
    if virtual_path(&row.path) {
        let bytes: Vec<u8> = connection.query_row("SELECT bytes FROM outline_documents WHERE path=?1", [&row.path], |r| r.get(0))?;
        require(bytes.len() as u64 == row.signature.size && sha256(&bytes) == row.hash, "changed requested outline bytes; run sifttext index STORE --repair")?;
        return Ok(String::from_utf8(bytes)?);
    }
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(root.join(&row.path))?;
    require(
        Signature::from(&file.metadata()?) == row.signature,
        "changed requested outline file",
    )?;
    let mut text = String::new();
    file.take(
        row.signature
            .size
            .checked_add(1)
            .ok_or("outline size overflow")?,
    )
    .read_to_string(&mut text)?;
    require(
        signature(root, &row.path)? == Some(row.signature),
        "changed requested outline file",
    )?;
    require(
        sha256(text.as_bytes()) == row.hash,
        "changed requested outline bytes; run sifttext index STORE --repair",
    )?;
    Ok(text)
}

/// Requested CLI projection only; global freshness is established by the caller's with_current.
pub(crate) fn indexed_outline(
    connection: &Connection,
    root: &Path,
    id: &str,
    full: bool,
    version: &str,
) -> Result<String> {
    require(
        !id.is_empty() && id.len() <= 36 && id.bytes().all(|b| b.is_ascii_hexdigit() || b == b'-'),
        "invalid UUID prefix",
    )?;
    let mut query = connection
        .prepare("SELECT id FROM nodes WHERE substr(id,1,length(?1))=?1 ORDER BY id LIMIT 2")?;
    let ids = query
        .query_map([id], |r| r.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    require(ids.len() == 1, "UUID prefix absent or ambiguous")?;
    let id = &ids[0];
    let mut document = cli_document(&read_document(connection, root, id, full)?, version)?;
    if !full && document["status"] != "invalid_draft" {
        for row in document["rows"]
            .as_array_mut()
            .ok_or("missing outline rows")?
        {
            if row["children"].as_u64().ok_or("missing child count")? > 0 {
                let branch = row["id"].as_str().ok_or("missing outline id")?;
                let text = read_document(connection, root, branch, true)?;
                let projected = crate::lean_render::render(&cli_document(&text, version)?)? + "\n";
                row["full_view_tokens"] = json!(outline_artifacts::cost(&projected).1);
            }
        }
    }
    Ok(crate::lean_render::render(&document)? + "\n")
}
