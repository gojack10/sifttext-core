//! Rebuildable per-working-view SQLite. Authored files remain the only source.
use crate::coherent_capture::{Capture as Snapshot, logical};
use crate::{Entry, Policy, Result, Store, require, sha256, status::Status};
use rusqlite::{Connection, OpenFlags, OptionalExtension, TransactionBehavior, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

/// Fixed, privacy-safe failure identity. Never contains source/error text or paths.
#[derive(Clone, Copy, Debug, Serialize, PartialEq, Eq)]
pub struct Refusal {
    pub stage: &'static str,
    pub kind: &'static str,
}
impl Refusal {
    pub fn from_error(stage: &'static str, error: &(dyn std::error::Error + 'static)) -> Self {
        let kind = error_kind(error);
        Self {
            stage,
            kind: if kind == "rejected" { "unknown" } else { kind },
        }
    }
}
impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "retrieval refused ({}: {})", self.stage, self.kind)
    }
}
impl std::error::Error for Refusal {}

// Retrieval classification is restricted to typed errors and fixed identities.
fn error_kind(error: &(dyn std::error::Error + 'static)) -> &'static str {
    if let Some(refusal) = error.downcast_ref::<Refusal>() {
        return refusal.kind;
    }
    if error.is::<serde_json::Error>() {
        return "invalid_request";
    }
    if let Some(error) = error.downcast_ref::<std::io::Error>() {
        return match error.kind() {
            std::io::ErrorKind::NotFound => "not_found",
            std::io::ErrorKind::PermissionDenied => "permission_denied",
            std::io::ErrorKind::StorageFull
            | std::io::ErrorKind::FileTooLarge
            | std::io::ErrorKind::OutOfMemory => "resource_exhausted",
            std::io::ErrorKind::TimedOut => "timeout",
            std::io::ErrorKind::WouldBlock => "busy",
            _ => "io",
        };
    }
    if let Some(rusqlite::Error::SqliteFailure(error, _)) = error.downcast_ref() {
        return match error.code {
            rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked => "busy",
            rusqlite::ErrorCode::DiskFull
            | rusqlite::ErrorCode::OutOfMemory
            | rusqlite::ErrorCode::TooBig => "resource_exhausted",
            rusqlite::ErrorCode::PermissionDenied | rusqlite::ErrorCode::ReadOnly => {
                "permission_denied"
            }
            _ => "sqlite",
        };
    }
    match error.to_string().as_str() {
        "policy process timed out"
        | "barrier expired"
        | "inspection barrier expired"
        | "refresh lock expired" => "timeout",
        "oversized request"
        | "oversized search"
        | "oversized node file"
        | "oversized authored file"
        | "policy process output exceeds limit" => "resource_exhausted",
        "unsafe or oversized authored file" => "unsafe_or_oversized_source",
        "policy process exited nonzero" => "helper_failed",
        "unexpected policy stderr" | "malformed checked graph verdict" => "helper_invalid_output",
        "checked graph rejected or malformed verdict" => "invalid_source",
        "changed source identity"
        | "changed base identity"
        | "changed base bytes"
        | "changed authored bytes" => "source_changed",
        "checked candidate changed; rebuild and recheck"
        | "candidate file set changed; rebuild and recheck" => "runtime_changed",
        _ => "rejected",
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Full,
    Incremental,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PauseBeforeAccept {
    pub ready: PathBuf,
    pub release: PathBuf,
}
impl PauseBeforeAccept {
    fn wait(&self, store: &Path, base: Option<&Path>) -> Result<()> {
        require(self.ready != self.release, "barrier paths must differ")?;
        for p in [&self.ready, &self.release] {
            let parent = fs::canonicalize(p.parent().ok_or("missing barrier parent")?)?;
            require(
                !parent.starts_with(store) && !base.is_some_and(|b| parent.starts_with(b)),
                "barrier must be outside authored stores",
            )?;
        }
        OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&self.ready)?
            .write_all(b"computed\n")?;
        let start = Instant::now();
        while !self.release.try_exists()? {
            require(start.elapsed() < Duration::from_secs(10), "barrier expired")?;
            thread::sleep(Duration::from_millis(5));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Link {
    pub source: String,
    pub target: String,
    pub reason: String,
}
#[derive(Debug, Serialize)]
pub struct View {
    /// Kept out of the legacy serialized view; adapters may opt into diagnostics.
    #[serde(skip)]
    pub refusal: Option<Refusal>,
    pub status: &'static str,
    pub version: Option<String>,
    pub nodes: Vec<Value>,
    pub links: Vec<Link>,
}
impl View {
    fn unavailable(status: &'static str, refusal: Refusal) -> Self {
        Self {
            refusal: Some(refusal),
            status,
            version: None,
            nodes: vec![],
            links: vec![],
        }
    }
}

/// In-process verified data, never a deserializable trust token. Cache loss or
/// incompatible processing requires preparation again from complete authored bytes.
pub struct PreparedIndex {
    store: Store,
    rows: BTreeMap<String, Value>,
    search_bodies: BTreeMap<String, String>,
}
impl PreparedIndex {
    fn source(&self, id: &str) -> Option<&[u8]> {
        match self
            .store
            .inventory
            .get(&PathBuf::from(format!("nodes/{id}/node.md")))?
        {
            Entry::File(bytes) => Some(bytes),
            _ => None,
        }
    }
    pub fn version(&self) -> String {
        version(&self.store)
    }
}

#[derive(Default, Debug, Serialize)]
pub struct Reuse {
    pub accepted_main: usize,
    pub prepared_worktree: usize,
    pub computed: usize,
    pub changed_dependencies: usize,
    pub search_bodies_reused: usize,
    pub changed_rows: usize,
    pub sqlite_changes: u64,
}

fn processing_identity() -> &'static str {
    static ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ID.get_or_init(|| {
        sha256(
            concat!(
                "sifttext-index/1;o200k_base;tiktoken-rs/0.6.0;",
                include_str!("lib.rs"),
                include_str!("coherent_capture.rs"),
                include_str!("index.rs"),
                include_str!("status.rs"),
                include_str!("search.rs"),
                include_str!("search_document.rs"),
                include_str!("token_metadata.rs"),
                include_str!("vendor/leviathan/search.rs"),
                include_str!("vendor/leviathan/query_text.rs"),
                include_str!("vendor/leviathan/relevance.rs"),
                include_str!("vendor/leviathan/card.rs"),
                include_str!("vendor/leviathan/snippet.rs"),
                include_str!("../Cargo.lock")
            )
            .as_bytes(),
        )
    })
}

/// Verify an existing prepared cache against complete current source and actual
/// contents. Missing/corrupt/incompatible/forged data returns None (no reuse).
/// Caller must supply the accepted-main store it actually intends to retain;
/// this does not confer Git publication authority.
pub fn prepare_reuse(
    policy: &Policy,
    path: &Path,
    base: Option<&Path>,
) -> Result<Option<PreparedIndex>> {
    let verify = || -> Result<PreparedIndex> {
        let captured = Snapshot::read(path, base)?;
        let rows = records(policy, &captured)?;
        let dir = captured.store.path.join(".sifttext");
        crate::real_dir(&dir)?;
        let db = dir.join("index.sqlite");
        let dir_id = identity(&dir)?;
        let db_id = identity(&db)?;
        require(
            fs::symlink_metadata(&db)?.nlink() == 1,
            "hard-linked prepared cache",
        )?;
        let mut connection = Connection::open_with_flags(
            &db,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NOFOLLOW,
        )?;
        connection.busy_timeout(Duration::from_secs(5))?;
        let tx = connection.transaction()?;
        for (key, expected) in [
            ("schema", "sifttext-index/1".to_owned()),
            ("processing", processing_identity().to_owned()),
            ("version", version(&captured.store)),
        ] {
            let got: String =
                tx.query_row("SELECT value FROM metadata WHERE key=?1", [key], |r| {
                    r.get(0)
                })?;
            require(got == expected, "incompatible prepared identity")?;
        }
        let cached: Vec<Value> = tx
            .prepare("SELECT record FROM nodes ORDER BY id")?
            .query_map([], |r| r.get::<_, String>(0))?
            .map(|r| Ok(serde_json::from_str(&r?)?))
            .collect::<Result<_>>()?;
        require(cached == rows, "forged prepared records")?;
        for row in &rows {
            let columns: Vec<String> = tx.query_row(
                "SELECT name,micro_summary,scope,content,status,children FROM nodes WHERE id=?1",
                [row["id"].as_str().ok_or("missing id")?],
                |r| (0..6).map(|i| r.get(i)).collect(),
            )?;
            let mut expected: Vec<String> = ["name", "micro_summary", "scope", "content", "status"]
                .iter()
                .map(|k| row[*k].as_str().unwrap_or("").to_owned())
                .collect();
            expected.push(serde_json::to_string(&row["children"])?);
            require(columns == expected, "forged prepared columns")?;
        }
        let links: Vec<(String, String, String, i64)> = tx
            .prepare("SELECT source,target,reason,position FROM links ORDER BY source,position")?
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
            .collect::<rusqlite::Result<_>>()?;
        let expected: Vec<_> = captured
            .store
            .nodes
            .iter()
            .flat_map(|(id, n)| {
                n.see_also
                    .iter()
                    .enumerate()
                    .map(|(i, r)| (id.clone(), r.target.clone(), r.reason.clone(), i as i64))
            })
            .collect();
        require(links == expected, "forged prepared links")?;
        captured.unchanged()?;
        require(
            identity(&db)? == db_id && identity(&dir)? == dir_id,
            "changed prepared cache",
        )?;
        let search_bodies = rows
            .iter()
            .map(|r| Ok((r["id"].as_str().unwrap().to_owned(), project_body(r)?)))
            .collect::<Result<_>>()?;
        Ok(PreparedIndex {
            store: captured.store,
            rows: cached
                .into_iter()
                .map(|r| (r["id"].as_str().unwrap().to_owned(), r))
                .collect(),
            search_bodies,
        })
    };
    Ok(verify().ok())
}

fn project_body(row: &Value) -> Result<String> {
    Ok(
        crate::search_document::project(crate::search_document::Fields {
            name: row["name"].as_str().ok_or("missing name")?,
            micro_summary: row["micro_summary"].as_str().ok_or("missing summary")?,
            scope: row["scope"].as_str().unwrap_or(""),
            crystallization: row["content"].as_str().ok_or("missing content")?,
        })?
        .body,
    )
}

/// Refresh the exact combined logical candidate. Main data takes precedence for
/// identical inputs; a changed dependency is recomputed, not copied from X.
/// FTS text is projected/reused, never copied as opaque posting pages or settings.
pub fn refresh_prepared(
    policy: &Policy,
    path: &Path,
    base: Option<&Path>,
    accepted_main: &PreparedIndex,
    worktree: Option<&PreparedIndex>,
    pause: Option<&PauseBeforeAccept>,
) -> Result<(View, Reuse)> {
    refresh_for_publication(policy, path, base, Some(accepted_main), worktree, pause)
}

/// Optional donors let native publication reconstruct missing caches honestly.
/// Roles are explicit: a worktree never receives accepted-main reuse credit.
pub(crate) fn refresh_for_publication(
    policy: &Policy,
    path: &Path,
    base: Option<&Path>,
    accepted_main: Option<&PreparedIndex>,
    worktree: Option<&PreparedIndex>,
    pause: Option<&PauseBeforeAccept>,
) -> Result<(View, Reuse)> {
    let mut prepared: Vec<_> = accepted_main.into_iter().map(|p| (p, true)).collect();
    prepared.extend(worktree.map(|p| (p, false)));
    let mut reuse = Reuse::default();
    let mut bodies_reused = 0;
    let (view, _) = with_query_prepared(
        policy,
        path,
        base,
        Mode::Incremental,
        pause,
        &prepared,
        &mut reuse,
        |tx, store, view, _| {
            let bodies = view
                .nodes
                .iter()
                .map(|row| {
                    let id = row["id"].as_str().unwrap();
                    let Entry::File(bytes) =
                        &store.inventory[&PathBuf::from(format!("nodes/{id}/node.md"))]
                    else {
                        return Err("missing source".into());
                    };
                    if let Some(body) = prepared.iter().find_map(|(p, _)| {
                        (p.source(id) == Some(bytes.as_slice()))
                            .then(|| p.search_bodies.get(id))
                            .flatten()
                    }) {
                        bodies_reused += 1;
                        Ok(body.clone())
                    } else {
                        project_body(row)
                    }
                })
                .collect::<Result<Vec<_>>>()?;
            let docs = view
                .nodes
                .iter()
                .zip(&bodies)
                .map(|(r, body)| crate::leviathan_search::Document {
                    id: r["id"].as_str().unwrap(),
                    title: r["name"].as_str().unwrap(),
                    body,
                    permitted: false,
                    boost: 1.0,
                })
                .collect::<Vec<_>>();
            crate::leviathan_search::maintain(tx, &docs)
        },
    )?;
    if view.status == "current" {
        reuse.search_bodies_reused = bodies_reused;
    } else {
        reuse = Reuse::default();
    }
    Ok((view, reuse))
}

type Identity = (u64, u64, i64, i64);
fn identity(path: &Path) -> Result<Identity> {
    let m = fs::symlink_metadata(path)?;
    Ok((m.dev(), m.ino(), m.ctime(), m.ctime_nsec()))
}
fn records(policy: &Policy, captured: &Snapshot) -> Result<Vec<Value>> {
    records_with(policy, captured, &[], &mut Reuse::default())
}

fn records_with(
    policy: &Policy,
    captured: &Snapshot,
    prepared: &[(&PreparedIndex, bool)],
    reuse: &mut Reuse,
) -> Result<Vec<Value>> {
    policy.validate(&captured.store)?;
    project_records(captured, prepared, reuse)
}

// Graph acceptance belongs to the caller; authored/status/reference projection
// remains independent even when a same-invocation validation capability is used.
fn project_records(
    captured: &Snapshot,
    prepared: &[(&PreparedIndex, bool)],
    reuse: &mut Reuse,
) -> Result<Vec<Value>> {
    let store = &captured.store;
    let mut rows = Vec::new();
    let mut states = BTreeMap::new();
    for (id, node) in &store.nodes {
        let Entry::File(bytes) = &store.inventory[&PathBuf::from(format!("nodes/{id}/node.md"))]
        else {
            return Err("missing node bytes".into());
        };
        if let Some((main, row)) = prepared.iter().find_map(|(p, main)| {
            (p.source(id) == Some(bytes.as_slice())
                && p.store.children.get(id) == store.children.get(id))
            .then(|| p.rows.get(id).map(|r| (*main, r.clone())))
            .flatten()
        }) {
            if main {
                reuse.accepted_main += 1;
            } else {
                reuse.prepared_worktree += 1;
            }
            states.insert(id.clone(), serde_json::from_value(row["status"].clone())?);
            rows.push(row);
            continue;
        }
        reuse.computed += 1;
        if prepared
            .iter()
            .any(|(p, _)| p.source(id) == Some(bytes.as_slice()))
        {
            reuse.changed_dependencies += 1;
        }
        let mut row: serde_json::Map<String, Value> = captured.metadata[id]
            .iter()
            .map(|(k, v)| Ok((k.to_owned(), serde_json::from_str(v)?)))
            .collect::<Result<_>>()?;
        require(
            !row.contains_key("content"),
            "body cannot also be authored as metadata",
        )?;
        let status: Status =
            serde_json::from_value(row.get("status").ok_or("missing status")?.clone())?;
        for key in [
            "scope",
            "warnings",
            "blocker",
            "deferred_reason",
            "discard_reason",
        ] {
            if let Some(v) = row.get(key) {
                require(v.is_string(), "invalid authored field type")?;
            }
        }
        let reason = match status {
            Status::Stuck => Some("blocker"),
            Status::Deferred => Some("deferred_reason"),
            Status::Discarded => Some("discard_reason"),
            _ => None,
        };
        if let Some(key) = reason {
            require(
                row.get(key)
                    .and_then(Value::as_str)
                    .is_some_and(|s| !s.trim().is_empty()),
                "blank active reason",
            )?;
        }
        if status == Status::Resolved {
            require(
                !node.content.trim().is_empty(),
                "blank resolved crystallization",
            )?;
        }
        let mut targets = BTreeSet::new();
        for r in &node.see_also {
            require(
                r.target != *id && targets.insert(&r.target),
                "self or duplicate reference",
            )?;
        }
        states.insert(id.clone(), status);
        row.insert("content".into(), json!(node.content));
        row.insert("children".into(), json!(store.children[id]));
        rows.push(Value::Object(row));
    }
    for (id, children) in &store.children {
        require(
            states[id] != Status::Resolved || !children.iter().any(|c| states[c].blocks()),
            "active direct child blocks resolved parent",
        )?;
    }
    Ok(rows)
}

fn version(store: &Store) -> String {
    let mut bytes = Vec::new();
    for (path, entry) in logical(store) {
        let p = path.to_string_lossy();
        bytes.extend_from_slice(&(p.len() as u64).to_le_bytes());
        bytes.extend_from_slice(p.as_bytes());
        match entry {
            Entry::Dir => bytes.push(0),
            Entry::File(b) => {
                bytes.push(1);
                bytes.extend_from_slice(&(b.len() as u64).to_le_bytes());
                bytes.extend(b);
            }
            Entry::Link(p) => {
                let p = p.to_string_lossy();
                bytes.push(2);
                bytes.extend_from_slice(&(p.len() as u64).to_le_bytes());
                bytes.extend_from_slice(p.as_bytes());
            }
        }
    }
    sha256(&bytes)
}

/// Every call reconciles authored content, including valid uncommitted edits.
/// Failed/changing snapshots never return old rows marked current. All SQLite
/// readers see either transaction version, never a mix; freshness is checked here.
pub fn refresh(
    policy: &Policy,
    path: &Path,
    base: Option<&Path>,
    mode: Mode,
    pause: Option<&PauseBeforeAccept>,
) -> Result<View> {
    Ok(with_query(policy, path, base, mode, pause, |_, _, _| Ok(()))?.0)
}

/// Compute a query within the refreshed transaction, accepting it only through
/// the same source/cache identity checks as index publication.
pub(crate) fn with_query<T>(
    policy: &Policy,
    path: &Path,
    base: Option<&Path>,
    mode: Mode,
    pause: Option<&PauseBeforeAccept>,
    query: impl FnOnce(&rusqlite::Transaction<'_>, &Store, &View) -> Result<T>,
) -> Result<(View, Option<T>)> {
    with_query_prepared(
        policy,
        path,
        base,
        mode,
        pause,
        &[],
        &mut Reuse::default(),
        |tx, store, view, _| query(tx, store, view),
    )
}

/// Return the requested publication's exact bytes only after the ordinary
/// refresh transaction and all source/artifact freshness barriers succeed.
pub fn outline(policy: &Policy, path: &Path, id: &str, full: bool) -> Result<String> {
    let (view, text) = with_query_prepared(
        policy,
        path,
        None,
        Mode::Incremental,
        None,
        &[],
        &mut Reuse::default(),
        |_, store, _, publication| publication.outline_text(store, &store.resolve(id)?, full),
    )?;
    require(
        view.status == "current",
        "outline not current; inspect authored source",
    )?;
    text.ok_or_else(|| "no current outline returned".into())
}

#[allow(clippy::too_many_arguments)]
fn with_query_prepared<T>(
    policy: &Policy,
    path: &Path,
    base: Option<&Path>,
    mode: Mode,
    pause: Option<&PauseBeforeAccept>,
    prepared: &[(&PreparedIndex, bool)],
    reuse: &mut Reuse,
    query: impl FnOnce(
        &rusqlite::Transaction<'_>,
        &Store,
        &View,
        &crate::outlines::Publication,
    ) -> Result<T>,
) -> Result<(View, Option<T>)> {
    let store_path = match working_path(path, base) {
        Ok(p) => p,
        Err(error) => {
            return Ok((
                View::unavailable(
                    "error",
                    Refusal::from_error("source_capture", error.as_ref()),
                ),
                None,
            ));
        }
    };
    let cache_dir = store_path.join(".sifttext");
    match fs::create_dir(&cache_dir) {
        Ok(()) => (),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => (),
        Err(e) => return Err(e.into()),
    }
    crate::real_dir(&cache_dir)?;
    let dir_identity = identity(&cache_dir)?;
    let path = cache_dir.join("index.sqlite");
    let mut connection = Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    let cache_identity = identity(&path)?;
    require(
        fs::symlink_metadata(&path)?.nlink() == 1,
        "hard-linked derived cache",
    )?;
    connection.busy_timeout(Duration::from_secs(5))?;
    // ponytail: serialize reconciliation per cache. Full source verification
    // remains necessary; aggregate-scale lock duration belongs to qualification.
    let lock_wait = Instant::now();
    retrieval_phase("waiting");
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let lock_held = Instant::now();
    let lock_wait_us = lock_wait.elapsed().as_micros();
    // Inventory also checks generated entries, including temporary outline files.
    // Capture under the publisher lock, not while another writer moves/removes them.
    retrieval_phase("capture");
    let captured = match Snapshot::read(&store_path, base) {
        Ok(s) => s,
        Err(error) => {
            return Ok((
                View::unavailable(
                    "error",
                    Refusal::from_error("source_capture", error.as_ref()),
                ),
                None,
            ));
        }
    };
    let rows = match records_with(policy, &captured, prepared, reuse) {
        Ok(r) => r,
        Err(error) => {
            return Ok((
                View::unavailable(
                    "error",
                    Refusal::from_error("source_validation", error.as_ref()),
                ),
                None,
            ));
        }
    };
    let version = version(&captured.store);
    let initial_changes = tx.total_changes();
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS nodes (
        id TEXT PRIMARY KEY, name TEXT NOT NULL, micro_summary TEXT NOT NULL,
        scope TEXT NOT NULL, content TEXT NOT NULL, status TEXT NOT NULL,
        children TEXT NOT NULL, record TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS links (source TEXT NOT NULL, target TEXT NOT NULL,
            reason TEXT NOT NULL, position INTEGER NOT NULL, PRIMARY KEY(source,target));
        CREATE TABLE IF NOT EXISTS metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
    )?;
    let existing: BTreeMap<String, Vec<String>> = tx
        .prepare("SELECT id,name,micro_summary,scope,content,status,children,record FROM nodes")?
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                (1..8).map(|i| r.get(i)).collect::<rusqlite::Result<_>>()?,
            ))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let mut cached_links: BTreeMap<String, Vec<(String, String, i64)>> = BTreeMap::new();
    for link in tx
        .prepare("SELECT source,target,reason,position FROM links ORDER BY source,position")?
        .query_map([], |r| {
            Ok((r.get::<_, String>(0)?, (r.get(1)?, r.get(2)?, r.get(3)?)))
        })?
    {
        let (source, link) = link?;
        cached_links.entry(source).or_default().push(link);
    }
    if matches!(mode, Mode::Full) {
        tx.execute_batch("DELETE FROM links; DELETE FROM nodes;")?;
    }
    let desired: BTreeMap<String, Vec<String>> = rows
        .iter()
        .map(|r| {
            let mut values = ["name", "micro_summary", "scope", "content", "status"]
                .iter()
                .map(|k| r[*k].as_str().unwrap_or("").to_owned())
                .collect::<Vec<_>>();
            values.push(serde_json::to_string(&r["children"])?);
            values.push(serde_json::to_string(r)?);
            Ok((r["id"].as_str().ok_or("missing id")?.to_owned(), values))
        })
        .collect::<Result<_>>()?;
    let changed: BTreeSet<_> = existing
        .keys()
        .chain(desired.keys())
        .chain(cached_links.keys())
        .filter(|id| {
            let wanted_links: Vec<_> = captured
                .store
                .nodes
                .get(*id)
                .map(|n| {
                    n.see_also
                        .iter()
                        .enumerate()
                        .map(|(i, r)| (r.target.clone(), r.reason.clone(), i as i64))
                        .collect()
                })
                .unwrap_or_default();
            matches!(mode, Mode::Full)
                || existing.get(*id) != desired.get(*id)
                || cached_links.get(*id).map(Vec::as_slice).unwrap_or(&[]) != wanted_links
        })
        .cloned()
        .collect();
    for id in &changed {
        tx.execute("DELETE FROM links WHERE source=?1", [id])?;
        if !desired.contains_key(id) {
            tx.execute("DELETE FROM nodes WHERE id=?1", [id])?;
        }
    }
    for row in &rows {
        let id = row["id"].as_str().unwrap();
        if changed.contains(id) {
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
                    desired[id][6]
                ],
            )?;
            for (i, link) in captured.store.nodes[id].see_also.iter().enumerate() {
                tx.execute(
                    "INSERT INTO links VALUES (?1,?2,?3,?4)",
                    params![id, link.target, link.reason, i as i64],
                )?;
            }
        }
    }
    tx.execute(
        "INSERT INTO metadata VALUES ('version',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value WHERE value<>excluded.value",
        [&version],
    )?;
    tx.execute(
        "INSERT INTO metadata VALUES ('schema','sifttext-index/1') ON CONFLICT(key) DO UPDATE SET value=excluded.value WHERE value<>excluded.value",
        [],
    )?;
    tx.execute(
        "INSERT INTO metadata VALUES ('processing',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value WHERE value<>excluded.value",
        [processing_identity()],
    )?;
    // Read the real transactional rows, not a parallel fixture-only projection.
    let nodes: Vec<Value> = tx
        .prepare("SELECT record FROM nodes ORDER BY id")?
        .query_map([], |r| r.get::<_, String>(0))?
        .map(|r| Ok(serde_json::from_str(&r?)?))
        .collect::<Result<_>>()?;
    let links: Vec<Link> = tx
        .prepare("SELECT source,target,reason FROM links ORDER BY source,position")?
        .query_map([], |r| {
            Ok(Link {
                source: r.get(0)?,
                target: r.get(1)?,
                reason: r.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    let view = View {
        refusal: None,
        status: "current",
        version: Some(version.clone()),
        nodes,
        links,
    };
    let cached: Option<String> = tx
        .query_row("SELECT value FROM metadata WHERE key='outlines'", [], |r| {
            r.get(0)
        })
        .optional()?;
    // The explicit fault-injection barrier needs a real staged candidate for
    // the retained staged-byte tamper check. Also reconcile abandoned stages
    // after an interrupted publisher, rather than retaining crash debris.
    let reused = if matches!(mode, Mode::Incremental)
        && pause.is_none()
        && !cache_dir.join("outline-stage").try_exists()?
    {
        cached
            .as_deref()
            .and_then(|m| serde_json::from_str::<crate::outlines::Manifest>(m).ok())
            .and_then(|m| {
                crate::outlines::Publication::reuse(&captured.store, &view.nodes, &version, &m).ok()
            })
    } else {
        None
    };
    let publication = match reused {
        Some(p) => p,
        None if pause.is_some() || matches!(mode, Mode::Full) => {
            crate::outlines::Publication::prepare(&captured.store, &view.nodes, &version)?
        }
        None => crate::outlines::Publication::prepare_changed(
            &captured.store,
            &view.nodes,
            &version,
            false,
        )?,
    };
    let result = query(&tx, &captured.store, &view, &publication)?;
    let reused_outlines = publication.reused();
    let staged_outline_files = publication.staged_files();
    let manifest = publication.manifest.clone();
    tx.execute(
        "INSERT INTO metadata VALUES ('outlines',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value WHERE value<>excluded.value",
        [serde_json::to_string(&manifest)?],
    )?;
    if let Some(pause) = pause {
        pause.wait(
            &captured.store.path,
            captured.base.as_ref().map(|s| s.path.as_path()),
        )?;
    }
    if let Err(refusal) = freshness(&captured, || publication.unchanged(&captured.store.path)) {
        return Ok((View::unavailable("stale", refusal), None));
    }
    let same_cache = |path: &Path, expected: Identity| -> Result<()> {
        let got = identity(path)?;
        require(
            (got.0, got.1) == (expected.0, expected.1),
            "replaced cache identity",
        )
    };
    same_cache(&cache_dir, dir_identity)?;
    same_cache(&path, cache_identity)?;
    publication.publish(&captured.store.path)?;
    if let Err(refusal) = freshness(&captured, || {
        crate::outlines::verify_manifest(&captured.store.path, &manifest)
    }) {
        return Ok((View::unavailable("stale", refusal), None));
    }
    let sqlite_changes = tx.total_changes() - initial_changes;
    reuse.sqlite_changes = sqlite_changes;
    reuse.changed_rows = changed.len();
    tx.commit()?;
    let lock_held_us = lock_held.elapsed().as_micros();
    // A late edit after the last validation is stale, not silently current.
    if let Err(refusal) = freshness(&captured, || {
        crate::outlines::verify_manifest(&captured.store.path, &manifest)
    }) {
        return Ok((View::unavailable("stale", refusal), None));
    }
    same_cache(&cache_dir, dir_identity)?;
    same_cache(&path, cache_identity)?;
    if std::env::var_os("SIFTTEXT_RETRIEVAL_TRACE").is_some() {
        eprintln!(
            "retrieval-work {}",
            json!({"lock_wait_us":lock_wait_us,
            "lock_held_us":lock_held_us,"sqlite_total_changes":sqlite_changes,
            "node_link_rows_changed":changed.len(),"outlines_reused":reused_outlines,
            "outline_files_staged":staged_outline_files,"outline_files":manifest.files.len()})
        );
    }
    Ok((view, Some(result)))
}

// Check the root/base boundary before any derived write, without traversing live
// generated artifacts. Complete capture/validation still occurs under the lock.
fn working_path(path: &Path, base: Option<&Path>) -> Result<PathBuf> {
    crate::real_dir(path)?;
    let path = fs::canonicalize(path)?;
    if let Some(base) = base {
        crate::real_dir(base)?;
        let base = fs::canonicalize(base)?;
        require(
            !path.starts_with(&base) && !base.starts_with(&path),
            "working view and immutable base must be disjoint",
        )?;
    }
    Ok(path)
}

fn retrieval_phase(phase: &str) {
    if std::env::var_os("SIFTTEXT_RETRIEVAL_TRACE").is_some() {
        eprintln!("retrieval-phase {}", json!({"phase":phase}));
    }
}

fn freshness(
    captured: &Snapshot,
    outlines: impl FnOnce() -> Result<()>,
) -> std::result::Result<(), Refusal> {
    captured
        .unchanged()
        .map_err(|e| Refusal::from_error("source_freshness", e.as_ref()))?;
    outlines().map_err(|e| Refusal::from_error("outline_freshness", e.as_ref()))
}

/// Bounded non-refreshing read: serialize against real publishers and return
/// exact artifact bytes only if the complete source and committed manifest agree.
pub(crate) fn inspect_outlines(
    policy: &Policy,
    path: &Path,
    base: Option<&Path>,
) -> Result<crate::outlines::Inspection> {
    inspect_outlines_with(
        path,
        base,
        true,
        |p| Snapshot::read(p, base),
        |s| records(policy, s),
        |s| s.unchanged(),
    )
}

pub(crate) fn inspect_validated_outlines(
    policy: &Policy,
    path: &Path,
    validated: &crate::candidate_validation::ValidatedCapture,
    include_files: bool,
) -> Result<crate::outlines::Inspection> {
    inspect_outlines_with(
        path,
        None,
        include_files,
        |p| {
            let captured = validated.checked(policy)?;
            require(
                captured.store.path == p,
                "outline validation capture mismatch",
            )?;
            Ok(captured)
        },
        |s| project_records(s, &[], &mut Reuse::default()),
        |_| validated.checked(policy).map(|_| ()),
    )
}

fn inspect_outlines_with<C: std::borrow::Borrow<Snapshot>>(
    path: &Path,
    base: Option<&Path>,
    include_files: bool,
    capture: impl Fn(&Path) -> Result<C>,
    records: impl Fn(&Snapshot) -> Result<Vec<Value>>,
    validation_barrier: impl Fn(&Snapshot) -> Result<()>,
) -> Result<crate::outlines::Inspection> {
    use crate::outlines::{Inspection, Manifest};
    let store_path = match working_path(path, base) {
        Ok(p) => p,
        Err(_) => return Ok(Inspection::unavailable("error")),
    };
    let cache_dir = store_path.join(".sifttext");
    let db = cache_dir.join("index.sqlite");
    if !db.try_exists()? {
        // No cached view to inspect, but malformed authored input is still an error.
        let valid = capture(&store_path).and_then(|s| records(s.borrow()));
        return Ok(Inspection::unavailable(if valid.is_ok() {
            "pending"
        } else {
            "error"
        }));
    }
    crate::real_dir(&cache_dir)?;
    let dir_id = identity(&cache_dir)?;
    let db_id = identity(&db)?;
    require(
        fs::symlink_metadata(&db)?.nlink() == 1,
        "hard-linked derived cache",
    )?;
    let mut connection = Connection::open_with_flags(
        &db,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NOFOLLOW,
    )?;
    connection.busy_timeout(Duration::from_secs(5))?;
    let tx = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
    // Ordinary inspection owns a new capture; publication borrows its opaque
    // invocation-local validation after a full opening check under this lock.
    let captured = match capture(&store_path) {
        Ok(s) => s,
        Err(_) => return Ok(Inspection::unavailable("error")),
    };
    let captured = captured.borrow();
    let rows = match records(captured) {
        Ok(r) => r,
        Err(_) => return Ok(Inspection::unavailable("error")),
    };
    let read = || -> Result<Inspection> {
        let indexed: String =
            tx.query_row("SELECT value FROM metadata WHERE key='version'", [], |r| {
                r.get(0)
            })?;
        let manifest: String =
            tx.query_row("SELECT value FROM metadata WHERE key='outlines'", [], |r| {
                r.get(0)
            })?;
        let manifest: Manifest = serde_json::from_str(&manifest)?;
        require(
            indexed == version(&captured.store) && manifest.version == indexed,
            "stale source or manifest",
        )?;
        let indexed_rows: Vec<Value> = tx
            .prepare("SELECT record FROM nodes ORDER BY id")?
            .query_map([], |r| r.get::<_, String>(0))?
            .map(|r| Ok(serde_json::from_str(&r?)?))
            .collect::<Result<_>>()?;
        require(indexed_rows == rows, "forged indexed records")?;
        for row in &rows {
            let columns: Vec<String> = tx.query_row(
                "SELECT name,micro_summary,scope,content,status,children FROM nodes WHERE id=?1",
                [row["id"].as_str().ok_or("missing id")?],
                |r| (0..6).map(|i| r.get(i)).collect(),
            )?;
            let mut expected: Vec<String> = ["name", "micro_summary", "scope", "content", "status"]
                .iter()
                .map(|k| row[*k].as_str().unwrap_or("").to_owned())
                .collect();
            expected.push(serde_json::to_string(&row["children"])?);
            require(columns == expected, "forged indexed columns")?;
        }
        let links: Vec<Link> = tx
            .prepare("SELECT source,target,reason FROM links ORDER BY source,position")?
            .query_map([], |r| {
                Ok(Link {
                    source: r.get(0)?,
                    target: r.get(1)?,
                    reason: r.get(2)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        let expected_links: Vec<Link> = captured
            .store
            .nodes
            .iter()
            .flat_map(|(id, n)| {
                n.see_also.iter().map(|r| Link {
                    source: id.clone(),
                    target: r.target.clone(),
                    reason: r.reason.clone(),
                })
            })
            .collect();
        require(links == expected_links, "forged indexed links")?;
        let files =
            crate::outlines::inspect_files(&captured.store, &rows, &manifest, include_files)?;
        // Public inspection rechecks this capture; validated inspection instead
        // rechecks its physically identical original (including every asset).
        validation_barrier(captured)?;
        for (p, expected) in [(&cache_dir, dir_id), (&db, db_id)] {
            let got = identity(p)?;
            require(
                (got.0, got.1) == (expected.0, expected.1),
                "replaced inspection cache",
            )?;
        }
        Ok(Inspection {
            status: "current",
            version: Some(indexed),
            files,
        })
    };
    Ok(read().unwrap_or_else(|_| Inspection::unavailable("stale")))
}
