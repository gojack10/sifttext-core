//! Shared inventory-to-graph boundary and race-aware source captures.
//! Freshness reads the working view; generated-file ownership uses the retained cache.
use crate::{
    Entry, Result, Store,
    candidate_validation::{structural_context, structural_guard},
    markdown, require, source_error, uuid,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

pub(crate) fn authored_path(p: &Path) -> bool {
    let parts: Vec<_> = p.components().collect();
    parts.first().is_some_and(|p| p.as_os_str() == "trees")
        || (parts.len() >= 2
            && parts[0].as_os_str() == "nodes"
            && (parts.len() == 2
                || ["node.md", "children.order", "children"]
                    .iter()
                    .any(|n| parts[2].as_os_str() == *n)))
}
pub(crate) fn logical(store: &Store) -> BTreeMap<PathBuf, Entry> {
    logical_inventory(&store.inventory)
}
fn logical_inventory(inventory: &BTreeMap<PathBuf, Entry>) -> BTreeMap<PathBuf, Entry> {
    inventory
        .iter()
        .filter(|(p, _)| authored_path(p))
        .map(|(p, e)| (p.clone(), e.clone()))
        .collect()
}

pub(crate) type Metadata = BTreeMap<String, BTreeMap<String, String>>;

/// One Markdown grammar and intrinsic identity boundary for filesystem and Git.
pub(crate) fn node_markdown<'a>(bytes: &'a [u8], id: &str) -> Result<crate::Markdown<'a>> {
    let file = PathBuf::from(format!("nodes/{id}/node.md"));
    let parsed = source_error::context(
        (|| markdown(std::str::from_utf8(bytes)?))(), &file, None)?;
    let authored = &parsed.authored;
    structural_guard(authored.id == id, Some(id), Some(&authored.id),
        "src/lib.rs:Store.read_view/node_uuid_identity", "duplicate/mismatched node UUID")?;
    for (field, value) in [("name", &authored.name), ("micro_summary", &authored.micro_summary)] {
        source_error::context(require(!value.trim().is_empty(), "blank node name/micro-summary"),
            &file, Some(field))?;
    }
    for reference in &authored.see_also {
        source_error::context(require(uuid(&reference.target), "reference target is not a UUID"),
            &file, Some("see_also.target"))?;
        source_error::context(require(!reference.reason.trim().is_empty(), "blank relationship reason"),
            &file, Some("see_also.reason"))?;
    }
    Ok(parsed)
}

/// Group relationships once, then parse only the bytes held in the inventory.
/// Physical checks are still performed for every canonical node/link; sparse
/// inheritance changes a link's physical destination, never its authored target.
pub(crate) fn from_inventory_mode(
    path: PathBuf,
    inventory: BTreeMap<PathBuf, Entry>,
    dirs: BTreeMap<String, PathBuf>,
    sparse: bool,
    compact: bool,
) -> Result<(Store, Metadata)> {
    let empty = |e: &Entry| e.file_identity().is_some_and(|(len, _)| len == 0);
    let mut nonblank = BTreeSet::new();
    let mut members: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut roots = Vec::new();
    for (p, e) in &inventory {
        #[cfg(test)]
        count(|c| c.relationship_entries += 1);
        // Markers and real directories are structural inputs, not arbitrary assets.
        let parts: Vec<_> = p.components().collect();
        if p == Path::new("nodes/.keep") {
            source_error::context(
                require(
                    empty(e),
                    "nonempty node directory marker",
                ),
                p,
                None,
            )?;
            continue;
        }
        if p == Path::new("nodes")
            || p == Path::new("trees")
            || (parts.len() == 2 && parts[0].as_os_str() == "nodes")
            || (parts.len() == 3
                && parts[0].as_os_str() == "nodes"
                && parts[2].as_os_str() == "children")
        {
            require(matches!(e, Entry::Dir), "unsafe structural directory")?;
        }
        if p.parent() == Some(Path::new("trees"))
            || (parts.len() == 4
                && parts[0].as_os_str() == "nodes"
                && parts[2].as_os_str() == "children")
        {
            if p.file_name().is_some_and(|n| n == ".keep") {
                require(
                    empty(e),
                    if p.parent() == Some(Path::new("trees")) {
                        "nonempty root directory marker"
                    } else {
                        "nonempty relationship directory marker"
                    },
                )?;
            } else {
                require(
                    matches!(e, Entry::Link(_)),
                    "child/root entry must be a symlink",
                )?;
            }
        }
        if let Entry::Link(target) = e {
            let child = crate::name(p)?;
            let root = p.parent() == Some(Path::new("trees"));
            let parent = (!root)
                .then(|| parts.get(1).and_then(|p| p.as_os_str().to_str()))
                .flatten();
            structural_guard(
                uuid(&child),
                parent,
                Some(&child),
                if root {
                    "src/lib.rs:Store.read_view/root_membership"
                } else {
                    "src/lib.rs:Store.read_view/child_uuid"
                },
                if root {
                    "unknown root UUID"
                } else {
                    "child entry is not a UUID"
                },
            )?;
            structural_guard(
                dirs.contains_key(&child),
                parent,
                Some(&child),
                if root {
                    "src/lib.rs:Store.read_view/root_membership"
                } else {
                    "src/lib.rs:Store.read_view/child_membership"
                },
                if root {
                    "unknown root UUID"
                } else {
                    "unknown child UUID"
                },
            )?;
            let destination = &dirs[&child];
            crate::real_dir(destination)?;
            let expected = if parts.len() == 2 && parts[0].as_os_str() == "trees" {
                roots.push(child.clone());
                PathBuf::from(format!("../nodes/{child}"))
            } else {
                require(
                    parts.len() == 4
                        && parts[0].as_os_str() == "nodes"
                        && parts[2].as_os_str() == "children",
                    "unsafe relationship path",
                )?;
                let parent = parts[1].as_os_str().to_str().ok_or("non-UTF-8 parent")?;
                require(dirs.contains_key(parent), "unknown relationship parent")?;
                members
                    .entry(parent.to_owned())
                    .or_default()
                    .insert(child.clone());
                PathBuf::from(format!("../../{child}"))
            };
            structural_context(
                require(*target == expected, "unsafe child/root link target"),
                parent,
                Some(&child),
                "src/lib.rs:safe_link",
            )?;
            if !sparse {
                structural_context(
                    crate::safe_link(&path.join(p), &expected, destination),
                    parent,
                    Some(&child),
                    "src/lib.rs:safe_link",
                )?;
            }
        }
    }
    let mut nodes = BTreeMap::new();
    let mut children = BTreeMap::new();
    let mut metadata = BTreeMap::new();
    for (id, dir) in &dirs {
        require(uuid(id), "node directory is not a canonical full UUID")?;
        crate::real_dir(dir)?;
        crate::real_dir(&dir.join("children"))?;
        let bytes = |file: &str| -> Result<std::borrow::Cow<'_, [u8]>> {
            match inventory.get(&PathBuf::from(format!("nodes/{id}/{file}"))) {
                Some(e @ (Entry::File(_) | Entry::FileRef { .. })) => e.bytes(),
                _ => Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "No such file or directory (missing captured node file)",
                )
                .into()),
            }
        };
        let file = PathBuf::from(format!("nodes/{id}/node.md"));
        let node_bytes = source_error::context(bytes("node.md"), &file, None)?;
        let parsed = node_markdown(&node_bytes, id)?;
        let mut authored = parsed.authored;
        if compact {
            if !authored.content.trim().is_empty() { nonblank.insert(id.clone()); }
            authored.content = String::new();
        }
        let order_file = PathBuf::from(format!("nodes/{id}/children.order"));
        let order_bytes = source_error::context(bytes("children.order"), &order_file, None)?;
        let order: Vec<String> = source_error::context(
            (|| Ok(std::str::from_utf8(&order_bytes)?))(),
            &order_file,
            None,
        )?
        .lines()
        .map(str::to_owned)
        .collect();
        structural_guard(
            order.iter().all(|c| uuid(c)),
            Some(id),
            None,
            "src/lib.rs:Store.read_view/order_uuid",
            "malformed children.order UUID",
        )?;
        let ordered: BTreeSet<_> = order.iter().cloned().collect();
        let members = members.remove(id).unwrap_or_default();
        let mut seen = BTreeSet::new();
        let offender = order
            .iter()
            .find(|c| !seen.insert(*c) || !members.contains(*c))
            .or_else(|| members.difference(&ordered).next());
        structural_guard(
            order.len() == ordered.len() && ordered == members,
            Some(id),
            offender.map(String::as_str),
            "src/lib.rs:Store.read_view/order_membership",
            "missing, duplicate or non-child order member",
        )?;
        metadata.insert(
            id.clone(),
            parsed
                .fields
                .into_iter()
                .map(|(k, v)| (k.to_owned(), v.to_owned()))
                .collect(),
        );
        nodes.insert(id.clone(), authored);
        children.insert(id.clone(), order);
    }
    Ok((
        Store {
            path,
            nodes,
            children,
            roots,
            inventory,
            compact_bodies: compact.then_some(nonblank),
        },
        metadata,
    ))
}

/// Source-less owners verified against the retained generated-byte cache.
#[derive(Default)]
pub(crate) struct Residues {
    owners: BTreeSet<String>,
    cached: Option<(rusqlite::Connection, PathBuf)>,
    files: Vec<PathBuf>,
    physical: BTreeMap<PathBuf, Identity>,
    head: String,
}
impl Residues {
    pub(crate) fn contains(&self, id: &str) -> bool {
        self.owners.contains(id)
    }
    pub(crate) fn unchanged(&self) -> Result<()> {
        if let Some((connection, root)) = &self.cached {
            require(crate::local::reader_git(root, &["rev-parse", "HEAD"])? == self.head,
                "changed residue Git source")?;
            for owner in &self.owners {
                crate::index::residue::verify(connection, root, owner)?;
            }
        }
        for (p, expected) in &self.physical {
            let m = fs::symlink_metadata(p)?;
            require(
                (m.dev(), m.ino(), m.ctime(), m.ctime_nsec()) == *expected,
                "changed residue identity",
            )?;
        }
        Ok(())
    }
    fn cleanup(&self) -> Result<()> {
        self.unchanged()?;
        if let Some((connection, root)) = &self.cached {
            for owner in &self.owners {
                crate::index::residue::retire(connection, root, owner)?;
            }
        }
        Ok(())
    }
}

pub(crate) fn residues(path: &Path) -> Result<Residues> {
    for dir in [path.to_path_buf(), path.join("nodes"), path.join("trees")] {
        let relative = dir.strip_prefix(path)?;
        source_error::context(
            crate::real_dir(&dir),
            if relative.as_os_str().is_empty() {
                Path::new(".")
            } else {
                relative
            },
            None,
        )?;
    }
    let mut result = Residues::default();
    for dir in crate::node_entries(&path.join("nodes"))? {
        source_error::context(
            (|| -> Result<()> {
                crate::real_dir(&dir)?;
                if fs::symlink_metadata(dir.join("node.md"))
                    .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
                {
                    let id = crate::name(&dir)?;
                    require(
                        crate::uuid(&id),
                        "node directory is not a canonical full UUID",
                    )?;
                    let files = crate::entries(&dir)?;
                    require(!files.is_empty(), "empty source-less node directory")?;
                    for p in &files {
                        let rel = p.strip_prefix(path)?;
                        require(
                            crate::outlines::derived(rel) || crate::token_metadata::derived(rel),
                            "unknown source-less node resource",
                        )?;
                        let m = fs::symlink_metadata(p)?;
                        require(m.is_file() && m.nlink() == 1, "unsafe residue resource")?;
                        result.files.push(p.clone());
                        result
                            .physical
                            .insert(p.clone(), (m.dev(), m.ino(), m.ctime(), m.ctime_nsec()));
                    }
                    let m = fs::symlink_metadata(&dir)?;
                    result
                        .physical
                        .insert(dir.clone(), (m.dev(), m.ino(), m.ctime(), m.ctime_nsec()));
                    result.owners.insert(id);
                }
                Ok(())
            })(),
            dir.strip_prefix(path)?,
            None,
        )?;
    }
    if result.owners.is_empty() {
        return Ok(result);
    }
    require(fs::canonicalize(crate::local::reader_git(path, &["rev-parse", "--show-toplevel"])?)? == path,
        "residue store must be its Git root")?;
    result.head = crate::local::reader_git(path, &["rev-parse", "HEAD"])?;
    for id in &result.owners {
        require(crate::local::reader_git(path, &["ls-tree", &result.head, "--", &format!("nodes/{id}")])?.is_empty(),
            "residue owner still belongs to current Git source")?;
    }
    // Reuse exact recorded output bytes, not historical regeneration or the
    // release builder's directory. Missing proof preserves every remaining file.
    result.cached = Some(crate::index::cached(path).map_err(|error|
        format!("preserved source-less nodes: ownership cache unavailable ({error}); restore authored files or recover the prior cache"))?);
    result.unchanged()?;
    Ok(result)
}

type Identity = (u64, u64, i64, i64);
#[cfg(test)]
thread_local! {
    pub(crate) static IDENTITY_SPAWN_FAILURE: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}
fn identities(path: &Path, residues: &Residues) -> Result<BTreeMap<PathBuf, Identity>> {
    identities_with_workers(path, residues, 4)
}
pub(crate) fn identities_with_workers(
    path: &Path,
    residues: &Residues,
    workers: usize,
) -> Result<BTreeMap<PathBuf, Identity>> {
    let mut out = BTreeMap::new();
    for dir in [path.to_path_buf(), path.join("nodes"), path.join("trees")] {
        let relative = dir.strip_prefix(path).unwrap_or(Path::new("."));
        let m = source_error::context(
            (|| {
                let m = fs::symlink_metadata(&dir)?;
                require(m.is_dir(), "unsafe store directory")?;
                Ok(m)
            })(),
            if relative.as_os_str().is_empty() {
                Path::new(".")
            } else {
                relative
            },
            None,
        )?;
        // Root/node ctime includes generated output; root-shortcut directory
        // ctime is authored membership (also detects create/delete ABA).
        let times = if dir == path.join("trees") {
            (m.ctime(), m.ctime_nsec())
        } else {
            (0, 0)
        };
        out.insert(dir, (m.dev(), m.ino(), times.0, times.1));
    }
    let paths = crate::node_entries(&path.join("nodes"))?;
    let owners = &residues.owners;
    let marker = path.join("nodes/.keep");
    match fs::symlink_metadata(&marker) {
        Ok(m) => {
            out.insert(marker, (m.dev(), m.ino(), m.ctime(), m.ctime_nsec()));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => {
            return source_error::context(Err(error.into()), Path::new("nodes/.keep"), None);
        }
    }
    if paths.len() >= 64 && workers > 1 {
        // Each complete physical sweep still settles before byte work starts or
        // its result is accepted. Only disjoint node subtrees run concurrently.
        let (startup, batches) = std::thread::scope(|scope| {
            let mut handles = Vec::new();
            let mut startup = Ok(());
            for chunk in paths.chunks(paths.len().div_ceil(workers)) {
                #[cfg(test)]
                if IDENTITY_SPAWN_FAILURE.with(|failure| failure.get() == Some(handles.len())) {
                    startup = Err(std::io::Error::other("injected identity spawn failure"));
                    break;
                }
                match std::thread::Builder::new().spawn_scoped(scope, move || {
                    let mut batch = BTreeMap::new();
                    node_identities(chunk, owners, &mut batch)
                        .map(|()| batch)
                        .map_err(source_error::WorkerError::capture)
                }) {
                    Ok(handle) => handles.push(handle),
                    Err(error) => {
                        startup = Err(error);
                        break;
                    }
                }
            }
            let batches = handles
                .into_iter()
                .map(|handle| handle.join())
                .collect::<Vec<_>>();
            (startup, batches)
        });
        startup?;
        for batch in batches {
            let batch = batch
                .map_err(|_| "identity worker panicked")?
                .map_err(source_error::WorkerError::restore)?;
            out.extend(batch);
        }
    } else {
        node_identities(&paths, owners, &mut out)?;
    }
    identity_walk(&path.join("trees"), &mut out)?;
    Ok(out)
}
fn identity_walk(p: &Path, out: &mut BTreeMap<PathBuf, Identity>) -> Result<()> {
    for entry in crate::entries(p)? {
        let m = fs::symlink_metadata(&entry)?;
        out.insert(entry.clone(), (m.dev(), m.ino(), m.ctime(), m.ctime_nsec()));
        if m.is_dir() && entry.file_name().is_some_and(|n| n == "children") {
            identity_walk(&entry, out)?;
        }
    }
    Ok(())
}
fn node_identities(
    paths: &[PathBuf],
    owners: &BTreeSet<String>,
    out: &mut BTreeMap<PathBuf, Identity>,
) -> Result<()> {
    for dir in paths {
        let relative = PathBuf::from("nodes").join(crate::name(dir)?);
        let m = source_error::context(
            (|| {
                let m = fs::symlink_metadata(&dir)?;
                require(m.is_dir(), "unsafe node directory")?;
                Ok(m)
            })(),
            &relative,
            None,
        )?;
        if owners.contains(&crate::name(&dir)?) {
            continue;
        }
        out.insert(dir.clone(), (m.dev(), m.ino(), 0, 0));
        for file in ["node.md", "children.order", "children"] {
            let p = dir.join(file);
            let m = source_error::context(
                fs::symlink_metadata(&p).map_err(Into::into),
                &relative.join(file),
                None,
            )?;
            out.insert(p.clone(), (m.dev(), m.ino(), m.ctime(), m.ctime_nsec()));
            if file == "children" {
                identity_walk(&p, out)?;
            }
        }
    }
    Ok(())
}

pub(crate) struct Capture {
    pub store: Store,
    pub metadata: Metadata,
    pub base: Option<Store>,
    physical: BTreeMap<PathBuf, Identity>,
    base_physical: Option<BTreeMap<PathBuf, Identity>>,
    residues: Residues,
    base_residues: Residues,
}
impl Capture {
    pub fn read(path: &Path, base: Option<&Path>) -> Result<Self> {
        let capture = Self::capture(path, base, false)?;
        capture.unchanged()?;
        Ok(capture)
    }

    /// Candidate validation executes unchanged_all after its policy/field checks.
    /// Keep the initial identity/ABA guard, but defer its duplicate byte inventory
    /// to that mandatory final barrier. No other consumer may accept this alone.
    pub(crate) fn read_for_validation(path: &Path) -> Result<Self> {
        let capture = Self::capture(path, None, true)?;
        capture.residues.unchanged()?;
        require(
            identities(&capture.store.path, &capture.residues)? == capture.physical,
            "changed source identity",
        )?;
        Ok(capture)
    }

    fn capture(path: &Path, base: Option<&Path>, compact: bool) -> Result<Self> {
        source_error::context(crate::real_dir(path), Path::new("."), None)?;
        if let Some(base) = base {
            crate::real_dir(base)?;
        }
        let path = fs::canonicalize(path)?;
        let base_path = base.map(fs::canonicalize).transpose()?;
        require(
            !base_path
                .as_ref()
                .is_some_and(|b| path.starts_with(b) || b.starts_with(&path)),
            "working view and immutable base must be disjoint",
        )?;
        let residues = residues(&path)?;
        let base_residues = base_path
            .as_deref()
            .map(self::residues)
            .transpose()?
            .unwrap_or_default();
        let physical = identities(&path, &residues)?;
        let base_physical = base_path
            .as_deref()
            .map(|p| identities(p, &base_residues))
            .transpose()?;
        let base = base_path
            .as_deref()
            .map(|p| Store::capture_classified_mode(p, None, &base_residues, compact).map(|s| s.0))
            .transpose()?;
        let (store, metadata) = Store::capture_classified_mode(&path, base.as_ref(), &residues, compact)?;
        Ok(Self {
            store,
            metadata,
            base,
            physical,
            base_physical,
            residues,
            base_residues,
        })
    }
    fn current(&self) -> Result<BTreeMap<PathBuf, Entry>> {
        self.residues.unchanged()?;
        self.base_residues.unchanged()?;
        require(
            identities(&self.store.path, &self.residues)? == self.physical,
            "changed source identity",
        )?;
        let base = if let Some(base) = &self.base {
            require(
                Some(identities(&base.path, &self.base_residues)?) == self.base_physical,
                "changed base identity",
            )?;
            let (now, _) = Store::inventory_classified_mode(&base.path, None, &self.base_residues, true)?;
            require(
                logical_inventory(&now) == logical(base),
                "changed base bytes",
            )?;
            Some(now)
        } else {
            None
        };
        let (now, _) = Store::inventory_classified_mode(
            &self.store.path,
            self.base
                .as_ref()
                .zip(base.as_ref())
                .map(|(b, files)| (b.path.as_path(), files)),
            &self.residues,
            true,
        )?;
        require(
            identities(&self.store.path, &self.residues)? == self.physical,
            "changed source identity",
        )?;
        if let Some(base) = &self.base {
            require(
                Some(identities(&base.path, &self.base_residues)?) == self.base_physical,
                "changed base identity",
            )?;
        }
        self.residues.unchanged()?;
        self.base_residues.unchanged()?;
        Ok(now)
    }
    pub fn unchanged(&self) -> Result<()> {
        require(
            logical_inventory(&self.current()?) == logical(&self.store),
            "changed authored bytes",
        )
    }
    // Status and neighborhoods historically protect unrelated inventoried assets too.
    pub fn unchanged_all(&self) -> Result<()> {
        require(
            self.current()? == self.store.inventory,
            "changed store bytes",
        )
    }
    pub(crate) fn cleanup_residues(&mut self) -> Result<()> {
        self.residues.cleanup()?;
        for owner in &self.residues.owners {
            let dir = self.store.path.join("nodes").join(owner);
            self.physical.remove(&dir);
        }
        for path in &self.residues.files {
            self.physical.remove(path);
        }
        self.residues = Residues::default();
        Ok(())
    }
}

/// Ephemeral reverse lookups preserve source UUID/reference order, not a second
/// authored hierarchy. Construct only after the graph policy has accepted it.
pub(crate) struct Relationships<'a> {
    pub parents: BTreeMap<&'a str, &'a str>,
    pub incoming: BTreeMap<&'a str, Vec<(&'a str, &'a str)>>,
}
impl<'a> Relationships<'a> {
    pub fn new(store: &'a Store) -> Self {
        let mut parents = BTreeMap::new();
        let mut incoming: BTreeMap<&str, Vec<(&str, &str)>> = BTreeMap::new();
        for (id, children) in &store.children {
            for child in children {
                parents.insert(child.as_str(), id.as_str());
            }
        }
        for (id, node) in &store.nodes {
            for r in &node.see_also {
                incoming.entry(&r.target).or_default().push((id, &r.reason));
            }
        }
        Self { parents, incoming }
    }
}

#[cfg(test)]
#[derive(Default, Debug, PartialEq, Eq)]
pub(crate) struct Work {
    pub inventories: usize,
    pub parses: usize,
    pub relationship_entries: usize,
}
#[cfg(test)]
thread_local! {
    static WORK: std::cell::RefCell<Work> = std::cell::RefCell::new(Work::default());
    static AFTER_INVENTORY: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
}
#[cfg(test)]
pub(crate) fn on_inventory(f: impl FnOnce() + 'static) {
    AFTER_INVENTORY.with(|h| *h.borrow_mut() = Some(Box::new(f)));
}
#[cfg(test)]
pub(crate) fn after_inventory() {
    let hook = AFTER_INVENTORY.with(|h| h.borrow_mut().take());
    if let Some(h) = hook {
        h();
    }
}
#[cfg(test)]
pub(crate) fn count(f: impl FnOnce(&mut Work)) {
    WORK.with(|w| f(&mut w.borrow_mut()));
}
#[cfg(test)]
pub(crate) fn take_work() -> Work {
    WORK.with(|w| std::mem::take(&mut *w.borrow_mut()))
}
