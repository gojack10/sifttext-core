//! Shared inventory-to-graph boundary and race-aware source captures.
//! No persistent cache: every freshness barrier reads the actual working view.
use crate::{
    Entry, Result, Store,
    candidate_validation::{structural_context, structural_guard},
    markdown, require, uuid,
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

/// Group relationships once, then parse only the bytes held in the inventory.
/// Physical checks are still performed for every canonical node/link; sparse
/// inheritance changes a link's physical destination, never its authored target.
pub(crate) fn from_inventory(
    path: PathBuf,
    inventory: BTreeMap<PathBuf, Entry>,
    dirs: BTreeMap<String, PathBuf>,
    sparse: bool,
) -> Result<(Store, Metadata)> {
    let mut members: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut roots = Vec::new();
    for (p, e) in &inventory {
        #[cfg(test)]
        count(|c| c.relationship_entries += 1);
        // Markers and real directories are structural inputs, not arbitrary assets.
        let parts: Vec<_> = p.components().collect();
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
                    matches!(e,Entry::File(b) if b.is_empty()),
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
        let bytes = |file: &str| -> Result<&[u8]> {
            match inventory.get(&PathBuf::from(format!("nodes/{id}/{file}"))) {
                Some(Entry::File(b)) => Ok(b),
                _ => Err(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "No such file or directory (missing captured node file)",
                )
                .into()),
            }
        };
        let parsed = markdown(std::str::from_utf8(bytes("node.md")?)?)?;
        let authored = parsed.authored;
        structural_guard(
            authored.id == *id,
            Some(id),
            Some(&authored.id),
            "src/lib.rs:Store.read_view/node_uuid_identity",
            "duplicate/mismatched node UUID",
        )?;
        require(
            !authored.name.trim().is_empty() && !authored.micro_summary.trim().is_empty(),
            "blank node name/micro-summary",
        )?;
        for r in &authored.see_also {
            require(uuid(&r.target), "reference target is not a UUID")?;
            require(!r.reason.trim().is_empty(), "blank relationship reason")?;
        }
        let order: Vec<String> = std::str::from_utf8(bytes("children.order")?)?
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
        },
        metadata,
    ))
}

/// Checked historical resources, separate from current authored node identities.
#[derive(Default)]
pub(crate) struct Residues {
    owners: BTreeSet<String>,
    receipts: Vec<crate::outlines::RecognizedResidue>,
    physical: BTreeMap<PathBuf, Identity>,
    history: Option<(PathBuf, Vec<u8>)>,
}
impl Residues {
    pub(crate) fn contains(&self, id: &str) -> bool {
        self.owners.contains(id)
    }
    pub(crate) fn unchanged(&self) -> Result<()> {
        if let Some((root, head)) = &self.history {
            require(
                history_git(root, &["rev-parse", "--verify", "HEAD^{commit}"])? == *head,
                "changed historical Git source",
            )?;
        }
        for receipt in &self.receipts {
            receipt.unchanged()?;
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
}

pub(crate) fn residues(path: &Path) -> Result<Residues> {
    for dir in [path.to_path_buf(), path.join("nodes"), path.join("trees")] {
        crate::real_dir(&dir)?;
    }
    let mut result = Residues::default();
    for dir in crate::entries(&path.join("nodes"))? {
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
                result
                    .physical
                    .insert(p.clone(), (m.dev(), m.ino(), m.ctime(), m.ctime_nsec()));
            }
            let m = fs::symlink_metadata(&dir)?;
            result
                .physical
                .insert(dir, (m.dev(), m.ino(), m.ctime(), m.ctime_nsec()));
            result.owners.insert(id);
        }
    }
    if result.owners.is_empty() {
        return Ok(result);
    }
    require(
        fs::canonicalize(
            std::str::from_utf8(&history_git(path, &["rev-parse", "--show-toplevel"])?)?.trim(),
        )? == path,
        "residue store must be its Git root",
    )?;
    let head = history_git(path, &["rev-parse", "--verify", "HEAD^{commit}"])?;
    result.history = Some((path.to_owned(), head.clone()));
    let head = std::str::from_utf8(&head)?.trim();
    let parent = history_git(path, &["rev-parse", "--verify", &format!("{head}^1")])?;
    let parent = std::str::from_utf8(&parent)?.trim();
    // Only the immediate committed parent is considered; no arbitrary history
    // scan or editable-cache provenance. Partial current checkouts stay invalid.
    for id in &result.owners {
        require(
            history_git(path, &["ls-tree", head, "--", &format!("nodes/{id}")])?.is_empty(),
            "residue owner still belongs to current Git source",
        )?;
    }
    let historical = tempfile::tempdir()?;
    reconstruct(path, parent, historical.path())?;
    let policy = crate::Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR")))?;
    let view = crate::index::refresh(
        &policy,
        historical.path(),
        None,
        crate::index::Mode::Full,
        None,
    )?;
    require(view.status == "current", "invalid historical source")?;
    for id in &result.owners {
        let proof = crate::outlines::ResidueProof::capture(&policy, historical.path(), None, id)?;
        let source = historical.path().join("nodes").join(id).join("node.md");
        let measurement = crate::token_metadata::Refresher::default()
            .refresh(&source, None)?
            .measurement;
        for p in crate::entries(&path.join("nodes").join(id))? {
            let rel = p.strip_prefix(path)?;
            if crate::outlines::derived(rel) {
                result.receipts.push(proof.recognize(path, rel)?);
            } else {
                require(
                    crate::token_metadata::safe_derived(&p)?,
                    "unrecognized token residue",
                )?;
                if p.file_name().is_some_and(|n| n == "node.meta.json") {
                    require(
                        serde_json::from_slice::<crate::token_metadata::Measurement>(
                            &crate::read_regular(&p)?,
                        )? == measurement,
                        "forged historical token residue",
                    )?;
                }
            }
        }
    }
    require(
        history_git(path, &["rev-parse", "--verify", "HEAD^{commit}"])?
            == format!("{head}\n").as_bytes(),
        "changed historical Git source",
    )?;
    result.unchanged()?;
    Ok(result)
}

// Read-only object access. Never checkout, filters, hooks, replace objects,
// inherited Git routing, remote fetching, or writes to the user's index.
const HISTORY_CAP: usize = 64 * 1024 * 1024;
fn history_git(path: &Path, args: &[&str]) -> Result<Vec<u8>> {
    history_git_input(path, args, None, HISTORY_CAP)
}
fn history_git_input(
    path: &Path,
    args: &[&str],
    input: Option<&[u8]>,
    cap: usize,
) -> Result<Vec<u8>> {
    use std::{
        io::{Read, Seek, Write},
        process::{Command, Stdio},
        sync::mpsc,
        thread,
        time::{Duration, Instant},
    };
    // A regular input file avoids a pipe writer blocking on batched Git output.
    let stdin = if let Some(input) = input {
        let mut file = tempfile::tempfile()?;
        file.write_all(input)?;
        file.rewind()?;
        Stdio::from(file)
    } else {
        Stdio::null()
    };
    let mut command = Command::new("git");
    for (key, _) in std::env::vars_os() {
        if key.as_encoded_bytes().starts_with(b"GIT_") {
            command.env_remove(key);
        }
    }
    let mut child = command
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .arg("-C")
        .arg(path)
        .args([
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .stdin(stdin)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let pipes: [Box<dyn Read + Send>; 2] = [
        Box::new(child.stdout.take().ok_or("missing Git stdout")?),
        Box::new(child.stderr.take().ok_or("missing Git stderr")?),
    ];
    let (tx, rx) = mpsc::channel();
    for (tag, pipe) in pipes.into_iter().enumerate() {
        let tx = tx.clone();
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let value = pipe
                .take((cap + 1) as u64)
                .read_to_end(&mut bytes)
                .map(|_| bytes);
            let _ = tx.send((tag, value));
        });
    }
    drop(tx);
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() > Duration::from_secs(10) {
            child.kill()?;
            child.wait()?;
            return Err("historical Git resource timeout".into());
        }
        thread::sleep(Duration::from_millis(2));
    };
    let mut out = [Vec::new(), Vec::new()];
    for _ in 0..2 {
        let (tag, bytes) = rx.recv_timeout(Duration::from_secs(1))?;
        out[tag] = bytes?;
    }
    require(
        status.success() && out.iter().map(Vec::len).sum::<usize>() <= cap,
        "historical Git read rejected",
    )?;
    Ok(std::mem::take(&mut out[0]))
}
fn reconstruct(repo: &Path, commit: &str, destination: &Path) -> Result<()> {
    let listing = history_git(
        repo,
        &[
            "ls-tree",
            "-rz",
            "--full-tree",
            commit,
            "--",
            "nodes",
            "trees",
        ],
    )?;
    let mut entries = Vec::new();
    let mut request = Vec::new();
    for record in listing.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let text = std::str::from_utf8(record)?;
        let (header, name) = text.split_once('\t').ok_or("malformed historical entry")?;
        let fields: Vec<_> = header.split(' ').collect();
        require(
            fields.len() == 3
                && fields[1] == "blob"
                && matches!(fields[2].len(), 40 | 64)
                && fields[2].bytes().all(|b| b.is_ascii_hexdigit()),
            "unsafe historical object",
        )?;
        let rel = Path::new(name);
        require(
            rel.components()
                .all(|c| matches!(c, std::path::Component::Normal(_))),
            "unsafe historical path",
        )?;
        let parts: Vec<_> = name.split('/').collect();
        let link = match parts.as_slice() {
            ["trees", id] if crate::uuid(id) => Some(format!("../nodes/{id}")),
            ["nodes", id, "children", child] if crate::uuid(id) && crate::uuid(child) => {
                Some(format!("../../{child}"))
            }
            _ => None,
        };
        let regular = matches!(parts.as_slice(), ["trees", ".keep"])
            || matches!(parts.as_slice(), ["nodes", id, "node.md" | "children.order"] if crate::uuid(id))
            || matches!(parts.as_slice(), ["nodes", id, "children", ".keep"] if crate::uuid(id));
        require(
            (fields[0] == "120000" && link.is_some())
                || (matches!(fields[0], "100644" | "100755") && regular),
            "unsafe historical resource",
        )?;
        request.extend_from_slice(fields[2].as_bytes());
        request.push(b'\n');
        entries.push((rel, fields[0], fields[2], link));
    }
    // Each frame adds at most 128 bytes (SHA-256, type, decimal size, delimiters).
    // Payload limits are checked separately; framing must not shrink their budget.
    let cap = entries
        .len()
        .checked_mul(128)
        .and_then(|n| HISTORY_CAP.checked_add(n))
        .ok_or("historical resource overflow")?;
    let response = history_git_input(repo, &["cat-file", "--batch"], Some(&request), cap)?;
    let mut remaining = response.as_slice();
    let mut total = 0usize;
    for (rel, mode, id, link) in entries {
        let bytes = history_blob(&mut remaining, id, &mut total)?;
        let p = destination.join(rel);
        fs::create_dir_all(p.parent().ok_or("missing historical parent")?)?;
        match mode {
            "100644" | "100755" => fs::write(&p, bytes)?,
            "120000" => {
                require(
                    Some(std::str::from_utf8(bytes)?) == link.as_deref(),
                    "unsafe historical link",
                )?;
                std::os::unix::fs::symlink(std::str::from_utf8(bytes)?, &p)?;
            }
            _ => return Err("unsafe historical file mode".into()),
        }
    }
    require(remaining.is_empty(), "unexpected historical batch output")?;
    for dir in crate::entries(&destination.join("nodes"))? {
        crate::real_dir(&dir)?;
        fs::create_dir_all(dir.join("children"))?;
    }
    crate::real_dir(&destination.join("trees"))?;
    Ok(())
}

// Git batch bodies are binary and length-delimited, never line-delimited.
fn history_blob<'a>(remaining: &mut &'a [u8], id: &str, total: &mut usize) -> Result<&'a [u8]> {
    let end = remaining
        .iter()
        .position(|b| *b == b'\n')
        .ok_or("malformed historical batch header")?;
    require(end < 128, "oversized historical batch header")?;
    let header = std::str::from_utf8(&remaining[..end])?;
    let fields: Vec<_> = header.split(' ').collect();
    require(
        fields.len() == 3 && fields[0] == id && fields[1] == "blob",
        "unsafe historical batch object",
    )?;
    require(
        !fields[2].is_empty() && fields[2].bytes().all(|b| b.is_ascii_digit()),
        "malformed historical batch size",
    )?;
    let size: usize = fields[2].parse()?;
    *total = total
        .checked_add(size)
        .ok_or("historical resource overflow")?;
    require(
        size <= crate::LIMIT && *total <= HISTORY_CAP,
        "historical reconstruction resource limit",
    )?;
    let body = &remaining[end + 1..];
    require(
        body.get(size) == Some(&b'\n'),
        "truncated historical batch body",
    )?;
    let bytes = &body[..size];
    *remaining = &body[size + 1..];
    Ok(bytes)
}

#[cfg(test)]
mod history_batch_tests {
    use super::*;

    #[test]
    fn binary_frames_preserve_bytes_and_refuse_malformed_or_oversized_objects() {
        let id = "a".repeat(40);
        let frame = format!("{id} blob 4\n").into_bytes();
        let body = b"\0\n\xffx";
        let response = [frame.as_slice(), body, b"\n", frame.as_slice(), body, b"\n"].concat();
        let mut rest = response.as_slice();
        let mut total = 0;
        assert_eq!(history_blob(&mut rest, &id, &mut total).unwrap(), body);
        assert_eq!(history_blob(&mut rest, &id, &mut total).unwrap(), body);
        assert!(rest.is_empty());
        assert_eq!(total, 8);
        for response in [
            format!("{id} missing\n"),
            format!("{} blob 0\n\n", "b".repeat(40)),
            format!("{id} tree 0\n\n"),
            format!("{id} blob -1\n"),
            format!("{id} blob 9999999999999999999999999999999999999\n"),
            format!("{id} blob {}\n", crate::LIMIT + 1),
            format!("{id} blob 1\nx"),
            format!("{id} blob 1\nx!"),
            format!("{id} blob 2\nx\n"),
            format!("{id} blob  0\n\n"),
            "unterminated".into(),
        ] {
            assert!(
                history_blob(&mut response.as_bytes(), &id, &mut 0).is_err(),
                "{response:?}"
            );
        }
        let response = format!("{id} blob 1\nx\n");
        let mut total = HISTORY_CAP;
        assert!(history_blob(&mut response.as_bytes(), &id, &mut total).is_err());
        let response = format!("{id} blob {}\n", crate::LIMIT);
        // At the object boundary it must fail for truncation, not the size limit.
        assert_eq!(
            history_blob(&mut response.as_bytes(), &id, &mut 0)
                .unwrap_err()
                .to_string(),
            "truncated historical batch body"
        );
    }

    #[test]
    fn batched_git_reconstructs_exact_committed_bytes_and_canonical_links() {
        let repo = tempfile::tempdir().unwrap();
        let id = "71000000-0000-4000-8000-000000000001";
        let dir = repo.path().join("nodes").join(id);
        fs::create_dir_all(dir.join("children")).unwrap();
        fs::create_dir(repo.path().join("trees")).unwrap();
        for (file, bytes) in [
            ("node.md", b"raw\0\n\xff".as_slice()),
            ("children.order", b""),
            ("children/.keep", b""),
        ] {
            fs::write(dir.join(file), bytes).unwrap();
        }
        std::os::unix::fs::symlink(format!("../nodes/{id}"), repo.path().join("trees").join(id))
            .unwrap();
        let git = |args: &[&str]| history_git(repo.path(), args).unwrap();
        git(&["init", "-q"]);
        git(&["add", "."]);
        git(&[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "fixture: historical bytes",
        ]);
        let commit = git(&["rev-parse", "HEAD"]);
        let commit = std::str::from_utf8(&commit).unwrap().trim();
        let destination = tempfile::tempdir().unwrap();
        reconstruct(repo.path(), commit, destination.path()).unwrap();
        for file in ["node.md", "children.order", "children/.keep"] {
            assert_eq!(
                fs::read(destination.path().join("nodes").join(id).join(file)).unwrap(),
                fs::read(dir.join(file)).unwrap()
            );
        }
        assert_eq!(
            fs::read_link(destination.path().join("trees").join(id)).unwrap(),
            PathBuf::from(format!("../nodes/{id}"))
        );
        assert!(history_git_input(repo.path(), &["--version"], None, 1).is_err());
        assert!(history_git(repo.path(), &["cat-file", "blob", "unknown-object"]).is_err());
        // The committed source, not the mutable working bytes, selects content.
        fs::write(dir.join("node.md"), b"uncommitted").unwrap();
        let destination = tempfile::tempdir().unwrap();
        reconstruct(repo.path(), commit, destination.path()).unwrap();
        assert_eq!(
            fs::read(destination.path().join("nodes").join(id).join("node.md")).unwrap(),
            b"raw\0\n\xff"
        );
        fs::remove_file(repo.path().join("trees").join(id)).unwrap();
        std::os::unix::fs::symlink("/unsafe", repo.path().join("trees").join(id)).unwrap();
        git(&["add", "."]);
        git(&[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-qm",
            "fixture: unsafe link",
        ]);
        let destination = tempfile::tempdir().unwrap();
        assert!(reconstruct(repo.path(), "HEAD", destination.path()).is_err());
        assert!(!destination.path().join("trees").join(id).exists());
    }
}

type Identity = (u64, u64, i64, i64);
fn identities(path: &Path, residues: &Residues) -> Result<BTreeMap<PathBuf, Identity>> {
    let mut out = BTreeMap::new();
    for dir in [path.to_path_buf(), path.join("nodes"), path.join("trees")] {
        let m = fs::symlink_metadata(&dir)?;
        require(m.is_dir(), "unsafe store directory")?;
        // Root/node ctime includes generated output; root-shortcut directory
        // ctime is authored membership (also detects create/delete ABA).
        let times = if dir == path.join("trees") {
            (m.ctime(), m.ctime_nsec())
        } else {
            (0, 0)
        };
        out.insert(dir, (m.dev(), m.ino(), times.0, times.1));
    }
    fn walk(p: &Path, out: &mut BTreeMap<PathBuf, Identity>) -> Result<()> {
        for entry in crate::entries(p)? {
            let m = fs::symlink_metadata(&entry)?;
            out.insert(entry.clone(), (m.dev(), m.ino(), m.ctime(), m.ctime_nsec()));
            if m.is_dir() && entry.file_name().is_some_and(|n| n == "children") {
                walk(&entry, out)?;
            }
        }
        Ok(())
    }
    for dir in crate::entries(&path.join("nodes"))? {
        let m = fs::symlink_metadata(&dir)?;
        require(m.is_dir(), "unsafe node directory")?;
        if residues.contains(&crate::name(&dir)?) {
            continue;
        }
        out.insert(dir.clone(), (m.dev(), m.ino(), 0, 0));
        for file in ["node.md", "children.order", "children"] {
            let p = dir.join(file);
            let m = fs::symlink_metadata(&p)?;
            out.insert(p.clone(), (m.dev(), m.ino(), m.ctime(), m.ctime_nsec()));
            if file == "children" {
                walk(&p, &mut out)?;
            }
        }
    }
    walk(&path.join("trees"), &mut out)?;
    Ok(out)
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
        crate::real_dir(path)?;
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
            .map(|p| Store::capture_classified(p, None, &base_residues).map(|s| s.0))
            .transpose()?;
        let (store, metadata) = Store::capture_classified(&path, base.as_ref(), &residues)?;
        let capture = Self {
            store,
            metadata,
            base,
            physical,
            base_physical,
            residues,
            base_residues,
        };
        capture.unchanged()?;
        Ok(capture)
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
            let (now, _) = Store::inventory_classified(&base.path, None, &self.base_residues)?;
            require(
                logical_inventory(&now) == logical(base),
                "changed base bytes",
            )?;
            Some(now)
        } else {
            None
        };
        let (now, _) = Store::inventory_classified(
            &self.store.path,
            self.base
                .as_ref()
                .zip(base.as_ref())
                .map(|(b, files)| (b.path.as_path(), files)),
            &self.residues,
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
