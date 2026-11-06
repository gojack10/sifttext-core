//! Local navigation generated from the indexed authored relationships.
use crate::{Result, real_dir, require, uuid};
use pulldown_cmark::{Event, Parser, Tag};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
};

const VERSION: &str = "node_links_format_version";
pub(crate) const EXCLUDES: &[&str] = &[
    "/nodes/*/parent",
    "/nodes/*/links-to/",
    "/nodes/*/linked-from/",
];

fn config(root: &Path) -> Result<Value> {
    let path = root.join(".sifttext/config.json");
    let value = match fs::symlink_metadata(&path) {
        Ok(_) => serde_json::from_slice(&crate::read_regular(&path)?)?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => json!({}),
        Err(e) => return Err(e.into()),
    };
    require(value.is_object(), "home configuration must be an object")?;
    Ok(value)
}

pub(crate) fn enabled(root: &Path) -> Result<bool> {
    Ok(config(root)?[VERSION].as_u64().is_some_and(|v| v >= 1))
}

fn save_config(root: &Path, value: &Value) -> Result<()> {
    let state = root.join(".sifttext");
    let mut file = tempfile::NamedTempFile::new_in(&state)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.as_file().sync_all()?;
    file.persist(state.join("config.json"))?;
    fs::File::open(state)?.sync_all()?;
    Ok(())
}

pub fn prepare(root: &Path) -> Result<()> {
    // Linked/sparse drafts reuse the index but materialize no navigation files.
    if !root.join(".git").is_dir() || enabled(root)? {
        return Ok(());
    }
    if crate::local::reader_git(root, &["symbolic-ref", "--quiet", "HEAD"]).is_ok() {
        return Ok(());
    }
    crate::local::exclude_local_artifacts(root, false, EXCLUDES)?;
    crate::index::prepare_node_links(root)?;
    let mut value = config(root)?;
    value[VERSION] = json!(1);
    save_config(root, &value)
}

pub(crate) fn retire(root: &Path) -> Result<()> {
    if !root.join(".git").is_dir() {
        return Ok(());
    }
    let mut value = config(root)?;
    if value.as_object_mut().unwrap().remove(VERSION).is_some() {
        save_config(root, &value)?;
    }
    update(root, &existing(root)?, &BTreeSet::new())?;
    crate::index::invalidate_node_links(root)
}

// The parser identifies actual links (including reference-style links) and
// skips examples in code. Only canonical local node destinations are indexed.
pub(crate) fn index_body(connection: &Connection, id: &str, body: &str) -> Result<()> {
    connection.execute("DELETE FROM body_links WHERE source=?1", [id])?;
    if !body.contains("/node.md") {
        return Ok(());
    }
    let mut insert =
        connection.prepare_cached("INSERT OR IGNORE INTO body_links VALUES (?1,?2)")?;
    for event in Parser::new(body) {
        if let Event::Start(Tag::Link { dest_url, .. }) = event
            && let Some(path) = dest_url.strip_prefix("../")
            && let Some(target) = path
                .split('#')
                .next()
                .and_then(|p| p.strip_suffix("/node.md"))
            && uuid(target)
            && target != id
        {
            insert.execute(params![id, target])?;
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Kind {
    Parent,
    LinksTo,
    LinkedFrom,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Parent => "parent",
            Self::LinksTo => "links-to",
            Self::LinkedFrom => "linked-from",
        }
    }
}

#[derive(Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct Shortcut {
    owner: String,
    kind: Kind,
    peer: String,
}

impl Shortcut {
    fn new(owner: &str, kind: Kind, peer: &str) -> Self {
        Self {
            owner: owner.into(),
            kind,
            peer: peer.into(),
        }
    }

    fn paths(&self, root: &Path) -> (PathBuf, PathBuf) {
        let path = root.join("nodes").join(&self.owner).join(self.kind.name());
        match self.kind {
            Kind::Parent => (path, Path::new("..").join(&self.peer)),
            _ => (path.join(&self.peer), Path::new("../..").join(&self.peer)),
        }
    }
}

fn collect_rows(
    statement: &mut rusqlite::Statement<'_>,
    parameters: impl rusqlite::Params,
    kind: Kind,
    result: &mut BTreeSet<Shortcut>,
) -> Result<()> {
    let mut rows = statement.query(parameters)?;
    while let Some(row) = rows.next()? {
        let owner: String = row.get(0)?;
        let peer: String = row.get(1)?;
        require(uuid(&owner) && uuid(&peer), "invalid indexed node link")?;
        result.insert(Shortcut::new(&owner, kind, &peer));
        if kind == Kind::LinksTo {
            result.insert(Shortcut::new(&peer, Kind::LinkedFrom, &owner));
        }
    }
    Ok(())
}

/// Before and after the index write, read only relations involving changed
/// owners or peers. Checking both sides also handles deleted/reappearing nodes.
pub(crate) fn affected(
    connection: &Connection,
    ids: &BTreeSet<String>,
) -> Result<BTreeSet<Shortcut>> {
    let mut result = BTreeSet::new();
    for table in ["links", "body_links", "child_links"] {
        let kind = if table == "child_links" {
            Kind::Parent
        } else {
            Kind::LinksTo
        };
        let columns = if kind == Kind::Parent {
            "e.target,e.source"
        } else {
            "e.source,e.target"
        };
        for side in ["source", "target"] {
            let mut stmt = connection.prepare_cached(&format!(
                "SELECT {columns} FROM {table} e
                 JOIN nodes s ON s.id=e.source JOIN nodes t ON t.id=e.target WHERE e.{side}=?1"
            ))?;
            for id in ids {
                collect_rows(&mut stmt, [id], kind, &mut result)?;
            }
        }
    }
    Ok(result)
}

pub(crate) fn rebuild(connection: &Connection, root: &Path) -> Result<u64> {
    let mut desired = BTreeSet::new();
    for (table, kind, columns) in [
        ("links", Kind::LinksTo, "e.source,e.target"),
        ("body_links", Kind::LinksTo, "e.source,e.target"),
        ("child_links", Kind::Parent, "e.target,e.source"),
    ] {
        let mut statement = connection.prepare(&format!(
            "SELECT {columns} FROM {table} e
             JOIN nodes s ON s.id=e.source JOIN nodes t ON t.id=e.target"
        ))?;
        collect_rows(&mut statement, [], kind, &mut desired)?;
    }
    update(root, &existing(root)?, &desired)
}

fn parent_target(target: &Path) -> Option<&str> {
    let id = target.to_str()?.strip_prefix("../")?;
    uuid(id).then_some(id)
}

pub(crate) fn is_parent_link(path: &Path) -> bool {
    fs::read_link(path).is_ok_and(|target| parent_target(&target).is_some())
}

// The full inspection is for initial preparation and interrupted refresh only.
// Unknown files are kept. A foreign occupant of a required path is an error.
fn existing(root: &Path) -> Result<BTreeSet<Shortcut>> {
    real_dir(&root.join("nodes"))?;
    let mut found = BTreeSet::new();
    for node in fs::read_dir(root.join("nodes"))? {
        let node = node?;
        let name = node.file_name();
        let Some(owner) = name.to_str().filter(|id| uuid(id)) else {
            continue;
        };
        if !node.file_type()?.is_dir() {
            continue;
        }
        let path = node.path();
        if let Ok(target) = fs::read_link(path.join("parent"))
            && let Some(peer) = parent_target(&target)
        {
            found.insert(Shortcut::new(owner, Kind::Parent, peer));
        }
        for kind in [Kind::LinksTo, Kind::LinkedFrom] {
            let directory = path.join(kind.name());
            if !fs::symlink_metadata(&directory).is_ok_and(|m| m.is_dir()) {
                continue;
            }
            for entry in fs::read_dir(directory)? {
                let entry = entry?;
                let name = entry.file_name();
                let Some(peer) = name.to_str().filter(|id| uuid(id)) else {
                    continue;
                };
                if entry.file_type()?.is_symlink()
                    && fs::read_link(entry.path())? == Path::new("../..").join(peer)
                {
                    found.insert(Shortcut::new(owner, kind, peer));
                }
            }
        }
    }
    Ok(found)
}

pub(crate) fn update(
    root: &Path,
    before: &BTreeSet<Shortcut>,
    after: &BTreeSet<Shortcut>,
) -> Result<u64> {
    real_dir(&root.join("nodes"))?;
    let mut writes = 0;
    for link in before.difference(after) {
        let (path, target) = link.paths(root);
        let owner = root.join("nodes").join(&link.owner);
        if !owner.try_exists()? {
            continue;
        }
        real_dir(&owner)?;
        if link.kind != Kind::Parent {
            let directory = path.parent().unwrap();
            if !directory.try_exists()? {
                continue;
            }
            real_dir(directory)?;
        }
        match fs::read_link(&path) {
            Ok(actual) => {
                require(
                    actual == target,
                    &format!("foreign symlink at {}", path.display()),
                )?;
                fs::remove_file(&path)?;
                writes += 1;
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
        if link.kind != Kind::Parent {
            match fs::remove_dir(path.parent().unwrap()) {
                Ok(()) => (),
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::DirectoryNotEmpty
                    ) =>
                {
                    ()
                }
                Err(e) => return Err(e.into()),
            }
        }
    }
    for link in after.difference(before) {
        let (path, target) = link.paths(root);
        real_dir(&root.join("nodes").join(&link.owner))?;
        if link.kind != Kind::Parent {
            let directory = path.parent().unwrap();
            match fs::create_dir(directory) {
                Ok(()) => (),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => real_dir(directory)?,
                Err(e) => return Err(e.into()),
            }
        }
        match symlink(&target, &path) {
            Ok(()) => writes += 1,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                require(
                    fs::read_link(&path).is_ok_and(|actual| actual == target),
                    &format!("foreign entry at {}", path.display()),
                )?;
            }
            Err(e) => return Err(e.into()),
        }
    }
    Ok(writes)
}
