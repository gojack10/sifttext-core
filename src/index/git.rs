//! Git owns discovery. SQLite retains immutable object inputs; only dirty paths need native reads.
use crate::{Result, require};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs::{self, OpenOptions}, io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt}, path::{Path, PathBuf}};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Native {
    dev: u64, ino: u64, size: u64, mtime: (i64, i64), ctime: (i64, i64),
}
impl Native {
    fn new(m: &fs::Metadata) -> Self {
        Self { dev: m.dev(), ino: m.ino(), size: m.len(),
            mtime: (m.mtime(), m.mtime_nsec()), ctime: (m.ctime(), m.ctime_nsec()) }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) struct Source {
    pub kind: u8,
    pub size: u64,
    pub target: Option<PathBuf>,
    pub blob: Option<String>,
    native: Option<Native>,
}
impl Source {
    fn directory() -> Self {
        Self { kind: 0, size: 0, target: None, blob: None, native: None }
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        // Keep the kind prefix used by the existing SQL relationship projection.
        let mut bytes = vec![1, self.kind];
        bytes.extend(serde_json::to_vec(self)?);
        Ok(bytes)
    }
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let source: Self = serde_json::from_slice(bytes.get(2..).ok_or("invalid source signature")?)?;
        require(bytes.get(..2) == Some([1, source.kind].as_slice()), "invalid source kind")?;
        Ok(source)
    }
    fn from_git(path: &Path, kind: u8, blob: String, size: u64) -> Result<Self> {
        require(valid_oid(&blob) && size <= crate::LIMIT as u64, "unsafe Git source object")?;
        let source = Self { kind, size, target: link_target(path, kind)?, blob: Some(blob), native: None };
        validate(path, &source)?;
        Ok(source)
    }
}
fn valid_oid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64) && oid.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn link_target(path: &Path, kind: u8) -> Result<Option<PathBuf>> {
    if kind != 2 { return Ok(None) }
    let text = path.to_str().ok_or("non-UTF-8 source path")?;
    let id = path.file_name().and_then(|p| p.to_str()).ok_or("missing relationship UUID")?;
    Ok(Some(PathBuf::from(if text.starts_with("trees/") {
        format!("../nodes/{id}")
    } else { format!("../../{id}") })))
}
fn validate(path: &Path, source: &Source) -> Result<()> {
    let text = path.to_str().ok_or("non-UTF-8 source path")?;
    let parts: Vec<_> = text.split('/').collect();
    let valid = match parts.as_slice() {
        ["nodes" | "trees"] => source.kind == 0,
        ["nodes" | "trees", ".keep"] => source.kind == 1 && source.size == 0,
        ["nodes", id] if crate::uuid(id) => source.kind == 0,
        ["nodes", id, "children"] if crate::uuid(id) => source.kind == 0,
        ["nodes", id, "node.md" | "children.order"] if crate::uuid(id) => source.kind == 1,
        ["nodes", id, "children", ".keep"] if crate::uuid(id) => source.kind == 1 && source.size == 0,
        ["nodes", id, "children", child] if crate::uuid(id) && crate::uuid(child) => source.kind == 2,
        ["trees", id] if crate::uuid(id) => source.kind == 2,
        _ => false,
    };
    require(valid && source.target == link_target(path, source.kind)?,
        &format!("unsafe authored path: {}", path.display()))
}
fn fingerprint(root: &Path, relative: &Path) -> Result<Option<Source>> {
    crate::real_dir(root)?;
    let mut path = root.to_owned();
    let parts: Vec<_> = relative.components().collect();
    for (i, part) in parts.iter().enumerate() {
        require(matches!(part, std::path::Component::Normal(_)), "unsafe authored path")?;
        path.push(part);
        let m = match fs::symlink_metadata(&path) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        if i + 1 < parts.len() {
            require(m.is_dir(), "unsafe authored parent")?;
            continue;
        }
        let kind = if m.is_file() { 1 } else if m.file_type().is_symlink() { 2 } else { return Err("unsafe authored entry".into()) };
        require(m.len() <= crate::LIMIT as u64 && (kind != 1 || m.nlink() == 1), "unsafe or oversized authored file")?;
        let source = Source { kind, size: m.len(), blob: None, native: Some(Native::new(&m)),
            target: (kind == 2).then(|| fs::read_link(&path)).transpose()? };
        validate(relative, &source)?;
        return Ok(Some(source));
    }
    Err("empty authored path".into())
}
fn output(root: &Path, args: &[&str]) -> Result<Vec<u8>> {
    // Discovery owns an ordinary Git index refresh. Permit its native stat and
    // untracked-cache updates; immutable object readers need no optional writes.
    let mut command = crate::local::reader_command(root, args);
    command.env_remove("GIT_OPTIONAL_LOCKS");
    let output = crate::checked_publication::run_command(command)?;
    require(output.status.success(), &format!("Git change discovery failed: {}", String::from_utf8_lossy(&output.stderr)))?;
    Ok(output.stdout)
}

pub(super) struct Snapshot {
    pub identity: String,
    pub repo: PathBuf,
    tree: String,
    changes: BTreeMap<PathBuf, Option<Source>>,
}
impl Snapshot {
    pub fn read(root: &Path) -> Result<Self> {
        let discovery = crate::local::reader_git(root, &["rev-parse", "--show-toplevel", "HEAD^{tree}"])?;
        let (top, tree) = discovery.split_once('\n').ok_or("missing Git source root")?;
        require(fs::canonicalize(top)? == root, "knowledge view must be its Git root")?;
        require(valid_oid(tree), "invalid Git tree identity")?;
        let tree = tree.to_owned();
        let status = output(root, &["-c", "core.untrackedCache=true", "status", "--porcelain=v1", "-z", "--untracked-files=all", "--no-renames", "--", "nodes", "trees"])?;
        let mut changes = BTreeMap::new();
        let mut hash = Sha256::new();
        hash.update(tree.as_bytes());
        for record in status.split(|b| *b == 0).filter(|r| !r.is_empty()) {
            require(record.len() > 3 && record[2] == b' ', "malformed Git status")?;
            require(!record[..2].iter().any(|b| matches!(b, b'U' | b'R' | b'C'))
                && &record[..2] != b"AA" && &record[..2] != b"DD", "unmerged Git working inputs")?;
            let path = PathBuf::from(std::str::from_utf8(&record[3..])?);
            if !crate::coherent_capture::authored_path(&path) && path != Path::new("nodes") { continue }
            let source = match fingerprint(root, &path)? {
                Some(source) => Some(source),
                None if !record[..2].contains(&b'D') => skipped_source(root, &path)?,
                None => None,
            };
            hash.update((record.len() as u64).to_le_bytes());
            hash.update(record);
            if let Some(source) = &source { hash.update(source.encode()?); }
            changes.insert(path, source);
        }
        // A concurrent checkout is a retry, not a different baseline for this capture.
        require(crate::local::reader_git(root, &["rev-parse", "--verify", "HEAD^{tree}"])? == tree,
            "changed source identity")?;
        Ok(Self { identity: format!("{:x}", hash.finalize()), repo: root.to_owned(), tree, changes })
    }
    pub fn accepted_version<'a>(&self, accepted: Option<(&str, &'a str)>) -> Option<&'a str> {
        accepted.filter(|(tree, _)| *tree == self.tree && self.changes.is_empty())
            .map(|(_, version)| version)
    }
    pub fn unchanged(&self) -> Result<bool> {
        let after = Self::read(&self.repo)?;
        Ok(after.identity == self.identity)
    }
    pub fn stage(&self, connection: &mut Connection, root: &Path) -> Result<()> {
        connection.execute_batch("CREATE TABLE IF NOT EXISTS git_sources (
            path TEXT PRIMARY KEY, signature BLOB NOT NULL, content_hash TEXT, oid TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS git_sources_oid ON git_sources(oid);")?;
        let previous: Option<String> = connection.query_row("SELECT value FROM metadata WHERE key='git_tree'", [], |r| r.get(0)).optional()?;
        if previous.as_deref() != Some(&self.tree) {
            let tx = connection.transaction()?;
            let mut objects = Objects::new(root);
            if let Some(previous) = previous.filter(|p| valid_oid(p)) {
                let delta = output(root, &["diff-tree", "-r", "--no-renames", "--name-only", "-z", &previous, &self.tree, "--", "nodes", "trees"])?;
                let paths = delta.split(|b| *b == 0).filter(|p| !p.is_empty())
                    .map(std::str::from_utf8).collect::<std::result::Result<Vec<_>, _>>()?;
                for path in &paths {
                    tx.execute("DELETE FROM git_sources WHERE path=?1", [path])?;
                }
                for paths in paths.chunks(128) {
                    let mut args = vec!["--literal-pathspecs", "ls-tree", "-l", "-z", &self.tree, "--"];
                    args.extend(paths.iter().copied());
                    let listing = output(root, &args)?;
                    for record in listing.split(|b| *b == 0).filter(|r| !r.is_empty()) {
                        let (header, path) = std::str::from_utf8(record)?.split_once('\t').ok_or("malformed Git tree entry")?;
                        if !crate::coherent_capture::authored_path(Path::new(path)) { continue }
                        let words: Vec<_> = header.split_ascii_whitespace().collect();
                        require(words.len() == 4 && words[1] == "blob", "unsupported Git source entry")?;
                        let kind = match words[0] { "100644" => 1, "120000" => 2, _ => return Err("unsupported Git source mode".into()) };
                        let source = Source::from_git(Path::new(path), kind, words[2].to_owned(), words[3].parse()?)?;
                        insert_git(&tx, &mut objects, path, &source)?;
                    }
                }
            } else {
                tx.execute("DELETE FROM git_sources", [])?;
                let ledger = crate::git_snapshot::GitReader::inventory(root, &self.tree)?;
                let mut query = ledger.connection.prepare("SELECT path,kind,oid,len FROM entries WHERE kind<>0 ORDER BY ordering")?;
                let mut rows = query.query([])?;
                while let Some(row) = rows.next()? {
                    let path: String = row.get(0)?;
                    let source = Source::from_git(Path::new(&path), row.get(1)?, row.get(2)?, u64::try_from(row.get::<_, i64>(3)?)?)?;
                    insert_git(&tx, &mut objects, &path, &source)?;
                }
            }
            tx.execute("INSERT INTO metadata VALUES('git_tree',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value", [&self.tree])?;
            tx.commit()?;
        }
        let tx = connection.transaction()?;
        tx.execute("INSERT INTO expected_sources(path,signature,content_hash,ordering) SELECT path,signature,content_hash,replace(path,'/',char(0)) FROM git_sources", [])?;
        for (path, source) in &self.changes {
            let path = path.to_str().ok_or("non-UTF-8 authored path")?;
            tx.execute("DELETE FROM expected_sources WHERE path=?1", [path])?;
            if let Some(source) = source {
                tx.execute("INSERT INTO expected_sources(path,signature,ordering) VALUES (?1,?2,?3)", params![path, source.encode()?, path.replace('/', "\0")])?;
            }
        }
        // Git omits empty directories. The complete logical inventory has the
        // two root containers and the structural parents of every authored leaf.
        let directory = Source::directory().encode()?;
        tx.execute("INSERT OR IGNORE INTO expected_sources(path,signature,ordering) VALUES ('nodes',?1,'nodes'),('trees',?1,'trees')", [&directory])?;
        tx.execute("INSERT OR IGNORE INTO expected_sources(path,signature,ordering)
            SELECT substr(path,1,42),?1,replace(substr(path,1,42),'/',char(0)) FROM expected_sources
            WHERE path GLOB 'nodes/*/*'", [&directory])?;
        tx.execute("INSERT OR IGNORE INTO expected_sources(path,signature,ordering)
            SELECT substr(path,1,42)||'/children',?1,replace(substr(path,1,42)||'/children','/',char(0)) FROM expected_sources
            WHERE path GLOB 'nodes/*' AND length(path)=42", [&directory])?;
        tx.commit()?;
        Ok(())
    }
}
fn insert_git(connection: &Connection, objects: &mut Objects<'_>, path: &str, source: &Source) -> Result<()> {
    // Relationship targets are checked against object bytes, not inferred from names.
    let hash = if source.kind == 2 {
        let bytes = objects.bytes(source.blob.as_deref().ok_or("missing Git object")?, source.size)?;
        require(Some(bytes.as_slice()) == source.target.as_ref().map(|p| p.as_os_str().as_encoded_bytes()), "unsafe Git relationship target")?;
        Some(format!("{:x}", Sha256::digest(&bytes)))
    } else {
        connection.query_row("SELECT content_hash FROM git_sources WHERE oid=?1 AND content_hash IS NOT NULL LIMIT 1", [&source.blob], |r| r.get::<_, String>(0)).optional()?
    };
    connection.execute("INSERT INTO git_sources VALUES (?1,?2,?3,?4)", params![path, source.encode()?, hash, source.blob])?;
    Ok(())
}

// An omitted skip-worktree path inherits its index object, including a staged
// change. A normal missing tracked path remains an explicit working deletion.
fn skipped_source(root: &Path, path: &Path) -> Result<Option<Source>> {
    let path_text = path.to_str().ok_or("non-UTF-8 authored path")?;
    let entries = output(root, &["--literal-pathspecs", "ls-files", "-v", "--stage", "-z", "--", path_text])?;
    let Some(record) = entries.split(|b| *b == 0).find(|r| !r.is_empty()) else { return Ok(None) };
    if record.first() != Some(&b'S') { return Ok(None) }
    let text = std::str::from_utf8(record)?;
    let (header, actual) = text.split_once('\t').ok_or("malformed sparse index source")?;
    let fields: Vec<_> = header.split_ascii_whitespace().collect();
    require(fields.len() == 4 && fields[3] == "0" && actual == path_text && valid_oid(fields[2]), "invalid sparse index source")?;
    let kind = match fields[1] { "100644" => 1, "120000" => 2, _ => return Err("unsupported sparse source mode".into()) };
    let size = crate::local::reader_git(root, &["cat-file", "-s", fields[2]])?.parse()?;
    let source = Source::from_git(path, kind, fields[2].to_owned(), size)?;
    if kind == 2 {
        let bytes = Objects::new(root).bytes(fields[2], size)?;
        require(Some(bytes.as_slice()) == source.target.as_ref().map(|p| p.as_os_str().as_encoded_bytes()), "unsafe sparse relationship target")?;
    }
    Ok(Some(source))
}

pub(super) struct Objects<'a> { root: &'a Path, batch: Option<crate::git_snapshot::Batch> }
impl<'a> Objects<'a> {
    pub fn new(root: &'a Path) -> Self { Self { root, batch: None } }
    pub fn bytes(&mut self, oid: &str, size: u64) -> Result<Vec<u8>> {
        if self.batch.is_none() { self.batch = Some(crate::git_snapshot::Batch::new(self.root)?); }
        self.batch.as_mut().ok_or("missing Git object reader")?.bytes(oid, size)
    }
    pub fn source_bytes(&mut self, path: &Path, source: &Source) -> Result<Vec<u8>> {
        if let Some(blob) = &source.blob { return self.bytes(blob, source.size) }
        require(fingerprint(self.root, path)?.as_ref() == Some(source), "changed source identity")?;
        let mut file = OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).open(self.root.join(path))?;
        require(file.metadata()?.is_file() && source.native.as_ref() == Some(&Native::new(&file.metadata()?)), "changed source identity")?;
        let mut bytes = Vec::new();
        file.by_ref().take(crate::LIMIT as u64 + 1).read_to_end(&mut bytes)?;
        require(bytes.len() as u64 == source.size && source.native.as_ref() == Some(&Native::new(&file.metadata()?))
            && fingerprint(self.root, path)?.as_ref() == Some(source), "changed authored bytes")?;
        Ok(bytes)
    }
}
