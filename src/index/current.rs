//! Git discovers working changes; immutable objects and unchanged derived rows are reused.
use super::git::{Objects, Snapshot, Source};
use super::{identity, processing_identity, working_path};
use crate::{Policy, Result, require};
use rusqlite::{Connection, OpenFlags, OptionalExtension, Transaction, params, types::ValueRef};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const SCHEMA: &str = "sifttext-index/6";
#[derive(Default)]
struct Work {
    refreshed: bool,
    source_rows_examined: u64,
    source_rows_changed: u64,
    nodes_parsed: u64,
    files_hashed: u64,
    hash_reused: u64,
    nodes_main_reused: u64,
    nodes_donor_reused: u64,
    outline_rows_written: u64,
    refresh_lock_wait_us: u128,
    sql_writer_held_us: u128,
}
impl Drop for Work {
    fn drop(&mut self) {
        crate::token_count::release();
        crate::release_unused_memory();
        if std::env::var_os("SIFTTEXT_RETRIEVAL_TRACE").is_some() {
            eprintln!("index-work {}", json!({
                "path":if self.refreshed {"refresh"} else {"warm"},
                "source_rows_examined":self.source_rows_examined,
                "source_rows_changed":self.source_rows_changed,
                "nodes_parsed":self.nodes_parsed,"nodes_main_reused":self.nodes_main_reused,
                "files_hashed":self.files_hashed,"hash_reused":self.hash_reused,
                "nodes_donor_reused":self.nodes_donor_reused,"outline_rows_written":self.outline_rows_written,
                "refresh_lock_wait_us":self.refresh_lock_wait_us,"sql_writer_held_us":self.sql_writer_held_us
            }));
        }
    }
}

fn source(connection: &Connection, path: &Path) -> Result<Source> {
    let encoded: Vec<u8> = connection.query_row("SELECT signature FROM expected_sources WHERE path=?1", [path.to_str().ok_or("non-UTF-8 path")?], |r| r.get(0))?;
    Source::decode(&encoded)
}
fn bytes(connection: &Connection, objects: &mut Objects<'_>, p: &Path) -> Result<Vec<u8>> {
    objects.source_bytes(p, &source(connection, p)?)
}

fn raw_hash(text: &str) -> Option<[u8; 32]> {
    if text.len() != 64 { return None }
    let mut hash = [0; 32];
    let digit = |c| match c { b'0'..=b'9' => Some(c - b'0'), b'a'..=b'f' => Some(c - b'a' + 10), _ => None };
    for (byte, pair) in hash.iter_mut().zip(text.as_bytes().chunks_exact(2)) {
        *byte = digit(pair[0])? * 16 + digit(pair[1])?;
    }
    Some(hash)
}

// Reuse committed bytes by object identity, and dirty bytes by exact native identity.
fn version(
    connection: &mut Connection,
    snapshot: &Snapshot,
    reuse: bool,
    accepted_version: Option<&str>,
    work: &mut Work,
) -> Result<String> {
    use crate::source_version::{self, Part};
    snapshot.stage(connection, &snapshot.repo)?;
    let mut version = source_version::start();
    let mut objects = Objects::new(&snapshot.repo);
    let tx = connection.transaction()?;
    let mut known = tx.prepare("SELECT signature,content_hash FROM sources WHERE path=?1")?;
    // Acceptance already hashed this exact immutable tree. Staging still
    // discovers the Git delta, but only missing file hashes need native work;
    // do not fold every unchanged source into the same content identity again.
    let mut entries = tx.prepare(if accepted_version.is_some() {
        "SELECT path,signature,content_hash FROM expected_sources WHERE content_hash IS NULL AND substr(signature,1,2)=x'0101' ORDER BY ordering"
    } else {
        "SELECT path,signature,content_hash FROM expected_sources ORDER BY ordering"
    })?;
    let mut rows = entries.query([])?;
    let mut update = tx.prepare("UPDATE expected_sources SET content_hash=?2 WHERE path=?1")?;
    while let Some(row) = rows.next()? {
        work.source_rows_examined += 1;
        let path: String = row.get(0)?;
        let p = Path::new(&path);
        let encoded: Vec<u8> = row.get(1)?;
        let source = Source::decode(&encoded)?;
        let immutable: Option<String> = row.get(2)?;
        let hash = match source.kind {
            0 => None,
            1 => {
                let cached = if let Some(hash) = immutable.as_deref().and_then(raw_hash) {
                    Some(hash)
                } else if reuse {
                    known.query_row([path.as_str()], |row| {
                        let (ValueRef::Blob(signature), ValueRef::Text(hash)) = (row.get_ref(0)?, row.get_ref(1)?) else { return Ok(None) };
                        Ok((Source::decode(signature).ok().as_ref() == Some(&source))
                            .then(|| std::str::from_utf8(hash).ok().and_then(raw_hash)).flatten())
                    }).optional()?.flatten()
                } else { None };
                Some(if let Some(hash) = cached {
                    work.hash_reused += 1;
                    hash
                } else {
                    let bytes = objects.source_bytes(p, &source)?;
                    work.files_hashed += 1;
                    Sha256::digest(&bytes).into()
                })
            }
            2 => {
                let target = source.target.as_ref().and_then(|p| p.to_str()).ok_or("invalid link target")?;
                Some(Sha256::digest(target.as_bytes()).into())
            }
            _ => return Err("invalid source kind".into()),
        };
        if accepted_version.is_none() && crate::coherent_capture::authored_path(p) {
            let part = match source.kind {
                0 => Part::Directory,
                1 => Part::File { len: source.size, hash: hash.as_ref().ok_or("missing file hash")? },
                2 => Part::Link {
                    len: source.target.as_ref().and_then(|p| p.to_str()).ok_or("invalid link target")?.len() as u64,
                    hash: hash.as_ref().ok_or("missing link hash")?,
                },
                _ => unreachable!(),
            };
            source_version::add(&mut version, p, part)?;
        }
        let hash = hash.map(|hash| {
            let hash: sha2::digest::Output<Sha256> = hash.into();
            format!("{hash:x}")
        });
        update.execute(params![path, hash])?;
        if immutable != hash && let Some(blob) = &source.blob {
            tx.execute("UPDATE git_sources SET content_hash=?2 WHERE path=?1 AND oid=?3", params![path, hash, blob])?;
        }
    }
    drop(rows);
    drop(entries);
    drop(known);
    drop(update);
    tx.commit()?;
    Ok(accepted_version.map(str::to_owned)
        .unwrap_or_else(|| format!("{:x}", version.finalize())))
}

fn safe_regular(path: &Path) -> Result<()> {
    let m = fs::symlink_metadata(path)?;
    require(m.is_file() && m.nlink() == 1, "unsafe derived cache file")
}
fn cache(root: &Path) -> Result<(Connection, PathBuf)> {
    let directory = root.join(".sifttext");
    match fs::create_dir(&directory) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
    }
    crate::real_dir(&directory)?;
    let path = directory.join("index.sqlite");
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let p = directory.join(format!("index.sqlite{suffix}"));
        match fs::symlink_metadata(&p) {
            Ok(_) => safe_regular(&p)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
    }
    let connection = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    safe_regular(&path)?;
    connection.busy_timeout(Duration::from_secs(5))?;
    connection
        .execute_batch("PRAGMA cache_size=-2048; PRAGMA mmap_size=0; PRAGMA temp_store=FILE; PRAGMA temp.cache_size=-1024; PRAGMA synchronous=FULL;")?;
    Ok((connection, directory))
}
fn meta(connection: &Connection, key: &str) -> Result<Option<String>> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='metadata')",
        [],
        |r| r.get(0),
    )?;
    if !exists {
        return Ok(None);
    }
    Ok(connection
        .query_row("SELECT value FROM metadata WHERE key=?1", [key], |r| {
            r.get(0)
        })
        .optional()?)
}
fn put(connection: &Connection, key: &str, value: &str) -> Result<()> {
    connection.execute("INSERT INTO metadata(key,value) VALUES (?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value WHERE value<>excluded.value", params![key,value])?;
    Ok(())
}
fn compatible(connection: &Connection) -> Result<bool> {
    Ok(meta(connection, "schema")?.as_deref() == Some(SCHEMA)
        && meta(connection, "source_version_algorithm")?.as_deref() == Some(crate::source_version::ALGORITHM)
        && meta(connection, "processing")?.as_deref() == Some(processing_identity()))
}

fn current(connection: &Connection, snapshot: &Snapshot) -> Result<Option<(String, String)>> {
    if !compatible(connection)? || meta(connection, "artifacts_ready")?.as_deref() != Some("1") {
        return Ok(None);
    }
    let (Some(version), Some(generation)) = (meta(connection, "version")?, meta(connection, "generation")?) else {
        return Ok(None);
    };
    if meta(connection, "git_working")?.as_deref() != Some(&snapshot.identity)
        || !crate::outlines::indexed_current(connection)? {
        return Ok(None);
    }
    if meta(connection, "artifacts_ready")?.as_deref() != Some("1")
        || meta(connection, "generation")?.as_deref() != Some(&generation) {
        return Ok(None);
    }
    Ok(Some((version, generation)))
}

// A donor is an optional exact-input memo, never an acceptance or source-freshness
// authority. Hold its read transaction for the complete record/artifact reuse.
fn donor(path: &Path) -> Result<(Connection, PathBuf)> {
    let (connection, root) = cached(path)?;
    require(compatible(&connection)?, "incompatible prepared cache")?;
    Ok((connection, root))
}

// Ownership hashes remain useful after a processor upgrade. Read the retained
// generation without refreshing away the removed owner's evidence.
pub(crate) fn cached(path: &Path) -> Result<(Connection, PathBuf)> {
    let root = working_path(path, None)?;
    crate::real_dir(&root.join(".sifttext"))?;
    let db = root.join(".sifttext/index.sqlite");
    for suffix in ["", "-wal", "-shm", "-journal"] {
        let p = root.join(format!(".sifttext/index.sqlite{suffix}"));
        match fs::symlink_metadata(&p) {
            Ok(_) => safe_regular(&p)?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && !suffix.is_empty() => (),
            Err(e) => return Err(e.into()),
        }
    }
    let connection = Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW)?;
    connection.execute_batch("PRAGMA cache_size=-1024; PRAGMA mmap_size=0; PRAGMA temp_store=FILE; BEGIN;")?;
    Ok((connection, root))
}
fn memo(connection: &Connection, id: &str, hash: &str) -> Option<(Value, String)> {
    let read = || -> Result<(Value, String)> {
        let (record, body): (String, String) = connection.query_row(
            "SELECT n.record,f.body FROM nodes n JOIN sources s ON s.path='nodes/'||n.id||'/node.md' JOIN search_ids i ON i.id=n.id JOIN search_fts f ON f.rowid=i.rowid WHERE n.id=?1 AND s.content_hash=?2",
            params![id,hash], |r| Ok((r.get(0)?,r.get(1)?)))?;
        let row: Value = serde_json::from_str(&record)?;
        require(row["id"].as_str() == Some(id), "invalid memo owner")?;
        Ok((row, body))
    };
    read().ok()
}
fn lock(directory: &Path) -> Result<File> {
    let path = directory.join("refresh.lock");
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)?;
    let m = file.metadata()?;
    require(m.is_file() && m.nlink() == 1, "unsafe refresh lock")?;
    file.lock()?;
    let actual = fs::symlink_metadata(path)?;
    require(
        actual.is_file()
            && actual.nlink() == 1
            && actual.dev() == m.dev()
            && actual.ino() == m.ino(),
        "replaced refresh lock",
    )?;
    Ok(file)
}

fn setup(connection: &Connection) -> Result<()> {
    connection.execute_batch("PRAGMA journal_mode=WAL;
        CREATE TABLE IF NOT EXISTS nodes (
            id TEXT PRIMARY KEY, name TEXT NOT NULL, micro_summary TEXT NOT NULL,
            scope TEXT NOT NULL, content TEXT NOT NULL, status TEXT NOT NULL,
            children TEXT NOT NULL, record TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS links (source TEXT NOT NULL, target TEXT NOT NULL,
            reason TEXT NOT NULL, position INTEGER NOT NULL, PRIMARY KEY(source,target));
        CREATE TABLE IF NOT EXISTS metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS sources (path TEXT PRIMARY KEY, signature BLOB NOT NULL, content_hash TEXT);
        CREATE TABLE IF NOT EXISTS child_links (source TEXT NOT NULL, target TEXT NOT NULL, PRIMARY KEY(source,target));
        CREATE INDEX IF NOT EXISTS child_links_target ON child_links(target);
        CREATE TABLE IF NOT EXISTS tree_roots (id TEXT PRIMARY KEY);
        CREATE TABLE IF NOT EXISTS relationship_mismatches (source TEXT PRIMARY KEY, detail TEXT NOT NULL);
        CREATE TEMP TABLE IF NOT EXISTS projected (id TEXT PRIMARY KEY, record TEXT NOT NULL, body TEXT);
        DELETE FROM projected;
        CREATE TEMP TABLE IF NOT EXISTS expected_sources (path TEXT PRIMARY KEY, signature BLOB NOT NULL, content_hash TEXT, ordering TEXT NOT NULL);
        CREATE INDEX IF NOT EXISTS temp.expected_source_order ON expected_sources(ordering);
        DELETE FROM expected_sources;")?;
    Ok(())
}

fn ids(connection: &Connection) -> Result<BTreeSet<String>> {
    Ok(connection.prepare("SELECT substr(path,7) FROM expected_sources WHERE path GLOB 'nodes/*' AND length(path)=42 AND substr(signature,1,2)=x'0100'")?
        .query_map([], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?)
}
fn project(connection: &Connection, objects: &mut Objects<'_>, id: &str) -> Result<Value> {
    let relative = PathBuf::from(format!("nodes/{id}/node.md"));
    crate::source_error::context(
        (|| {
            let bytes = bytes(connection, objects, &relative)?;
            let parsed = crate::markdown(std::str::from_utf8(&bytes)?)?;
            require(
                parsed.authored.id == id && crate::uuid(&parsed.authored.id),
                "node UUID disagrees with path",
            )?;
            let mut row = parsed
                .fields
                .iter()
                .map(|(k, v)| Ok(((*k).to_owned(), serde_json::from_str(v)?)))
                .collect::<Result<serde_json::Map<String, Value>>>()?;
            require(
                !row.contains_key("content"),
                "body cannot also be authored as metadata",
            )?;
            require(
                row.get("status").is_some_and(Value::is_string),
                "missing or invalid status",
            )?;
            for field in [
                "scope",
                "warnings",
                "blocker",
                "deferred_reason",
                "discard_reason",
            ] {
                require(
                    row.get(field).is_none_or(Value::is_string),
                    "invalid authored field type",
                )?;
            }
            let order = bytes_for_order(connection, objects, id)?;
            row.insert("content".into(), json!(parsed.authored.content));
            row.insert("children".into(), json!(order));
            Ok(Value::Object(row))
        })(),
        &relative,
        None,
    )
}
fn bytes_for_order(connection: &Connection, objects: &mut Objects<'_>, id: &str) -> Result<Vec<String>> {
    let p = PathBuf::from(format!("nodes/{id}/children.order"));
    let legacy: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM expected_sources WHERE path=?1)", [p.to_str()], |row| row.get(0))?;
    if legacy {
        let bytes = bytes(connection, objects, &p)?;
        let text = std::str::from_utf8(&bytes)?;
        return text.lines().map(|line| {
            require(crate::uuid(line), "children.order entry is not a UUID")?;
            Ok(line.to_owned())
        }).collect();
    }
    let own = bytes(connection, objects, &PathBuf::from(format!("nodes/{id}/node.md")))?;
    require(crate::ordering::position(std::str::from_utf8(&own)?)?.is_some(), "missing children.order or per-node position")?;
    let prefix = format!("nodes/{id}/children/");
    let names = connection.prepare("SELECT substr(path,length(?1)+1) FROM expected_sources WHERE path>=?1 AND path<?2 AND substr(signature,1,2)=x'0102'")?
        .query_map(params![prefix, format!("nodes/{id}/children0")], |row| row.get::<_, String>(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    crate::ordering::sort(names, |child| {
        let text = bytes(connection, objects, &PathBuf::from(format!("nodes/{child}/node.md")))?;
        crate::ordering::position(std::str::from_utf8(&text)?)
    })
}
fn write_record(tx: &Transaction<'_>, row: &Value, body: Option<&str>) -> Result<()> {
    let id = row["id"].as_str().ok_or("missing id")?;
    tx.execute(
        "INSERT OR REPLACE INTO nodes VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![
            id,
            row["name"].as_str(),
            row["micro_summary"].as_str(),
            row["scope"].as_str().unwrap_or(""),
            row["content"].as_str(),
            row["status"].as_str(),
            serde_json::to_string(&row["children"])?,
            serde_json::to_string(row)?
        ],
    )?;
    tx.execute("DELETE FROM links WHERE source=?1", [id])?;
    // Duplicate references remain visible in record JSON. The legacy links schema
    // has one row per source/target, so retain its last authored position there.
    for (i, link) in row["see_also"]
        .as_array()
        .ok_or("invalid see_also")?
        .iter()
        .enumerate()
    {
        tx.execute(
            "INSERT OR REPLACE INTO links VALUES (?1,?2,?3,?4)",
            params![
                id,
                link["target"].as_str(),
                link["reason"].as_str(),
                i as i64
            ],
        )?;
    }
    if let Some(body) = body {
        crate::leviathan_search::maintain_one(
            tx,
            id,
            row["name"].as_str().ok_or("missing name")?,
            body,
        )?;
    }
    Ok(())
}
fn relations(tx: &Transaction<'_>, id: &str, row: Option<&Value>) -> Result<()> {
    tx.execute("DELETE FROM child_links WHERE source=?1", [id])?;
    tx.execute("DELETE FROM relationship_mismatches WHERE source=?1", [id])?;
    let Some(row) = row else { return Ok(()) };
    let prefix = format!("nodes/{id}/children/");
    let mut links = BTreeSet::new();
    let mut entries = tx.prepare("SELECT substr(path,length(?1)+1) FROM expected_sources WHERE path>=?1 AND path<?2 AND substr(signature,1,2)=x'0102'")?;
    for child in entries.query_map(params![prefix, format!("nodes/{id}/children0")], |r| r.get::<_, String>(0))? {
        let child = child?;
        tx.execute("INSERT INTO child_links VALUES (?1,?2)", params![id, child])?;
        links.insert(child);
    }
    let order = row["children"].as_array().ok_or("invalid children")?;
    let ordered: BTreeSet<_> = order
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect();
    if ordered != links || order.len() != ordered.len() {
        tx.execute(
            "INSERT INTO relationship_mismatches VALUES (?1,?2)",
            params![id, "children.order differs from authored children links"],
        )?;
    }
    Ok(())
}

fn refresh(
    connection: &mut Connection,
    root: &Path,
    snapshot: &Snapshot,
    donor: Option<(&Connection, &Path)>,
    accepted: Option<(&str, &str)>,
    work: &mut Work,
) -> Result<bool> {
    work.refreshed = true;
    let force = !compatible(connection)?;
    // Completed derived data survives reboot. Explicit preparation/repair owns
    // global work; ordinary reads do not re-audit the forest on a new boot.
    let force_artifact_verification = meta(connection, "artifacts_ready")?.as_deref() != Some("1")
        || !crate::outlines::indexed_current(connection)?;
    setup(connection)?;
    let version = version(connection, snapshot, !force, snapshot.accepted_version(accepted), work)?;
    let mut objects = Objects::new(&snapshot.repo);
    let present = ids(connection)?;
    // Compute the actual path diff once. Only affected owners are projected;
    // main remains the starting point and all unrelated rows stay in place.
    let mut changes = BTreeSet::new();
    let mut delta = connection.prepare("SELECT e.path FROM expected_sources e LEFT JOIN sources s ON e.path=s.path WHERE s.path IS NULL OR e.signature<>s.signature OR e.content_hash IS NOT s.content_hash UNION SELECT s.path FROM sources s LEFT JOIN expected_sources e ON e.path=s.path WHERE e.path IS NULL")?;
    for path in delta.query_map([], |r| r.get::<_, String>(0))? {
        work.source_rows_changed += 1;
        let path = PathBuf::from(path?);
        let mut parts = path.components();
        if parts.next().is_some_and(|p| p.as_os_str() == "nodes")
            && let Some(id) = parts.next().and_then(|p| p.as_os_str().to_str())
            && crate::uuid(id) {
            changes.insert(id.to_owned());
        }
    }
    drop(delta);
    // A child's position belongs to that child, but changes its parent's view.
    let changed: Vec<_> = changes.iter().cloned().collect();
    for child in changed {
        let mut parents = connection.prepare("SELECT source FROM child_links WHERE target=?1")?;
        changes.extend(parents.query_map([child], |row| row.get::<_, String>(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?);
    }
    if force {
        changes.extend(present.iter().cloned());
        changes.extend(connection.prepare("SELECT id FROM nodes")?.query_map([], |r| r.get::<_, String>(0))?.collect::<rusqlite::Result<BTreeSet<_>>>()?);
    }
    for id in changes.iter().filter(|id| present.contains(*id)) {
        let hash: String = connection.query_row("SELECT content_hash FROM expected_sources WHERE path=?1", [format!("nodes/{id}/node.md")], |r| r.get(0))?;
        let main = (!force).then(|| memo(connection, id, &hash)).flatten();
        let (mut row, body) = if let Some((row, _)) = main {
            work.nodes_main_reused += 1;
            (row, None)
        } else if let Some((row, body)) = donor.and_then(|(db, _)| memo(db, id, &hash)) {
            work.nodes_donor_reused += 1;
            (row, Some(body))
        } else {
            work.nodes_parsed += 1;
            let row = project(connection, &mut objects, id)?;
            let body = super::project_body(&row)?;
            (row, Some(body))
        };
        // A Markdown hash authorizes only its projected fields, never hierarchy.
        row["children"] = json!(bytes_for_order(connection, &mut objects, id)?);
        connection.execute("INSERT INTO projected VALUES (?1,?2,?3)", params![id,serde_json::to_string(&row)?,body])?;
    }
    drop(objects);
    let generation = meta(connection, "generation")?.and_then(|s| s.parse::<u64>().ok()).unwrap_or(0).checked_add(1).ok_or("cache generation overflow")?.to_string();
    // Both markers commit together before any filesystem publication. A fresh
    // observer rejects even a completed same-version refresh during its query.
    {
        let tx = connection.transaction()?;
        put(&tx, "artifacts_ready", "0")?;
        put(&tx, "generation", &generation)?;
        tx.commit()?;
    }
    let writer = Instant::now();
    let result = (|| -> Result<bool> {
        let tx = connection.transaction()?;
        crate::leviathan_search::initialize_incremental(&tx)?;
        for id in &changes {
            if !present.contains(id) {
                tx.execute("DELETE FROM nodes WHERE id=?1", [id])?;
                tx.execute("DELETE FROM links WHERE source=?1", [id])?;
                crate::leviathan_search::remove_one(&tx, id)?;
                relations(&tx, id, None)?;
            } else {
                let (record, body): (String, Option<String>) = tx.query_row("SELECT record,body FROM projected WHERE id=?1", [id], |r| Ok((r.get(0)?,r.get(1)?)))?;
                let row: Value = serde_json::from_str(&record)?;
                write_record(&tx, &row, body.as_deref())?;
                relations(&tx, id, Some(&row))?;
            }
        }
        // Root shortcuts are authored data, independent of Markdown changes.
        tx.execute("DELETE FROM tree_roots WHERE id NOT IN (SELECT substr(path,7) FROM expected_sources WHERE path GLOB 'trees/*' AND typeof(signature)='blob' AND substr(signature,1,2)=x'0102')", [])?;
        tx.execute("INSERT OR IGNORE INTO tree_roots SELECT substr(path,7) FROM expected_sources WHERE path GLOB 'trees/*' AND typeof(signature)='blob' AND substr(signature,1,2)=x'0102'", [])?;
        // Only compact changed IDs survive preparation, and a forced whole-view
        // repair needs no per-owner dirty set. Release both before rendering.
        drop(present);
        if force || force_artifact_verification { changes.clear(); }
        // Project/render from the complete logical overlay. git_sources retains
        // only immutable baseline metadata in this same rebuildable SQLite cache.
        work.outline_rows_written += crate::outlines::refresh_indexed_from(&tx, root, &version, &changes, force || force_artifact_verification, donor)?;
        // Git reconciles tracked, staged, untracked and deleted inputs, including
        // sparse omissions. Generated-output retirement cannot change this identity.
        if !snapshot.unchanged()? { return Ok(false) }
        tx.execute("DELETE FROM sources WHERE path NOT IN (SELECT path FROM expected_sources)", [])?;
        tx.execute("INSERT INTO sources SELECT path,signature,content_hash FROM expected_sources WHERE true ON CONFLICT(path) DO UPDATE SET signature=excluded.signature,content_hash=excluded.content_hash WHERE signature<>excluded.signature OR content_hash IS NOT excluded.content_hash", [])?;
        let count: i64 = tx.query_row("SELECT count(*) FROM sources", [], |r| r.get(0))?;
        put(&tx, "source_count", &count.to_string())?;
        put(&tx, "schema", SCHEMA)?;
        put(&tx, "source_version_algorithm", crate::source_version::ALGORITHM)?;
        put(&tx, "processing", processing_identity())?;
        put(&tx, "version", &version)?;
        put(&tx, "git_working", &snapshot.identity)?;
        put(&tx, "artifacts_ready", "1")?;
        tx.commit()?;
        Ok(true)
    })();
    work.sql_writer_held_us += writer.elapsed().as_micros();
    result
}

/// Explicit repair of lost/damaged generated outputs. Ordinary retrieval trusts
/// the completed generation instead of inspecting every unrelated output file.
pub(crate) fn repair(policy: &Policy, path: &Path) -> Result<(String, i64)> {
    let root = working_path(path, None)?;
    {
        let (mut connection, directory) = cache(&root)?;
        let _lock = lock(&directory)?;
        setup(&connection)?;
        let tx = connection.transaction()?;
        put(&tx, "artifacts_ready", "0")?;
        tx.execute("DELETE FROM metadata WHERE key IN ('schema','git_tree')", [])?;
        tx.commit()?;
    }
    prepare(policy, &root, None)
}

/// Explicit creation/update boundary. Reuse unchanged data when compatible;
/// processor/schema changes and lost caches may only rebuild at this boundary.
pub(crate) fn prepare(
    _policy: &Policy, path: &Path, donor: Option<&Path>,
) -> Result<(String, i64)> {
    with_source(path, donor, None, true, false, |tx, _| {
        Ok(tx.query_row("SELECT count(*) FROM nodes", [], |row| row.get(0))?)
    })
}

/// Complete logical working-version retrieval. Only refreshers hold the advisory lock;
/// unchanged queries use deferred read transactions and bounded SQLite caches.
pub(crate) fn with_current<T>(
    policy: &Policy,
    path: &Path,
    query: impl FnMut(&Transaction<'_>, &str) -> Result<T>,
) -> Result<(String, T)> {
    with_current_from(policy, path, None, query)
}

pub(crate) fn with_current_from<T>(
    _policy: &Policy,
    path: &Path,
    prepared: Option<&Path>,
    query: impl FnMut(&Transaction<'_>, &str) -> Result<T>,
) -> Result<(String, T)> {
    with_source(path, prepared, None, false, true, query)
}

/// Reader advance needs completed derived state, not a query over mutable rows.
/// Reuse the final refresh barrier instead of opening a query and rediscovering
/// the same inputs twice afterward. A warm completion still captures Git once.
/// The accepted version is reusable only for an exact, clean Git tree.
pub(crate) fn refresh_current(
    path: &Path, prepared: Option<&Path>, accepted: Option<(&str, &str)>,
) -> Result<String> {
    Ok(with_source(path, prepared, accepted, false, false, |_, _| Ok(()))?.0)
}

fn with_source<T>(
    path: &Path, prepared: Option<&Path>,
    accepted: Option<(&str, &str)>, prepare: bool, verify_query: bool,
    mut query: impl FnMut(&Transaction<'_>, &str) -> Result<T>,
) -> Result<(String, T)> {
    let mut work = Work::default();
    let root = working_path(path, None)?;
    let (mut connection, directory) = cache(&root)?;
    let directory_id = identity(&directory)?;
    let database = directory.join("index.sqlite");
    let database_id = identity(&database)?;
    let donor = prepared.and_then(|p| donor(p).ok());
    let capture = || Snapshot::read(&root);
    for _ in 0..4 {
        let snapshot = match capture() {
            Ok(snapshot) => snapshot,
            Err(error) if super::Refusal::is_changed_source(error.as_ref()) => continue,
            Err(error) => return Err(error),
        };
        let mut refresh_lock = None;
        let tx = connection.transaction()?;
        require(prepare || (compatible(&tx)? && crate::outlines::indexed_current(&tx)?),
            &format!("derived data requires update preparation, not a read-time rebuild. Run `sifttext index {}` (managed installation: `sifttext update --prepare`), then retry", root.display()))?;
        let tx = if current(&tx, &snapshot)?.is_none() {
            drop(tx);
            let waiting = Instant::now();
            refresh_lock = Some(lock(&directory)?);
            work.refresh_lock_wait_us += waiting.elapsed().as_micros();
            // Another refresher may have published while this caller waited.
            // Reuse our inputs: mutable bytes validate their native identities
            // when read, and the final barrier catches any intervening changes.
            if current(&connection, &snapshot)?.is_none() {
                if !refresh(&mut connection, &root, &snapshot,
                    donor.as_ref().map(|(c,p)| (c,p.as_path())), accepted, &mut work)? {
                    continue;
                }
            } else if !snapshot.unchanged()? {
                continue;
            }
            connection.transaction()?
        } else { tx };
        let Some((version, generation)) = current(&tx, &snapshot)? else { continue };
        let result = query(&tx, &version)?;
        tx.commit()?;
        // Real queries retain their post-query source barrier. Ensure-only
        // callers have no intervening query work; refresh already checked it.
        if verify_query && !snapshot.unchanged()? { continue }
        let (observer, _) = cache(&root)?;
        // A fresh connection observes another publisher's dirty generation;
        // the original read transaction cannot. No additional source scan.
        if current(&observer, &snapshot)? != Some((version.clone(), generation)) {
            continue;
        }
        for (p, expected) in [(&directory, directory_id), (&database, database_id)] {
            let actual = identity(p)?;
            require((actual.0, actual.1) == (expected.0, expected.1), "replaced cache identity")?;
        }
        drop(refresh_lock);
        return Ok((version, result));
    }
    Err(super::Refusal::changed_source().into())
}
