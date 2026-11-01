//! Exact recorded scratch ownership, never ownership inferred from Git's list.
use super::{clean, commit, git, lifetime::Custody, run};
use crate::{Result, require};
use serde::{Deserialize, Serialize};
use std::{
    cell::RefCell,
    fs,
    io::Write,
    os::unix::fs::{DirBuilderExt, MetadataExt},
    path::{Path, PathBuf},
    rc::Rc,
};

#[derive(Debug, Clone, Serialize)]
pub struct Note {
    pub phase: &'static str,
    pub kind: &'static str,
    pub record: PathBuf,
    pub cleanup: &'static str,
}
fn disposition(result: &Result<()>) -> &'static str {
    match result {
        Ok(()) => "removed",
        Err(e) if e.to_string().contains("save busy") => "busy",
        Err(e) if e.to_string().contains("termination unconfirmed") => "termination_unconfirmed",
        Err(_) => "preserved_unsafe_or_incomplete",
    }
}
pub(super) type Notes = Rc<RefCell<Vec<Note>>>;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Identity {
    dev: u64,
    ino: u64,
}
impl Identity {
    fn of(path: &Path) -> Result<Self> {
        let m = fs::symlink_metadata(path)?;
        require(m.is_dir(), "owned directory replaced")?;
        Ok(Self {
            dev: m.dev(),
            ino: m.ino(),
        })
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    version: u8,
    common: PathBuf,
    repository: Identity,
    parent: Identity,
    view: Identity,
    revision: String,
    disposition: String,
    initialized: bool,
    #[serde(default)]
    last_failure: Option<String>,
}
fn persist(parent: &Path, record: &Record) -> Result<()> {
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(&serde_json::to_vec(record)?)?;
    temp.as_file().sync_all()?;
    temp.persist(parent.join("owner.json"))?;
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}
fn common(repo: &Path) -> Result<PathBuf> {
    let p = PathBuf::from(git(
        repo,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?);
    require(fs::canonicalize(&p)? == p, "noncanonical Git metadata")?;
    crate::real_dir(&p)?;
    Ok(p)
}
fn load(repo: &Path, parent: &Path) -> Result<Record> {
    let common = common(repo)?;
    require(
        (parent.parent() == Some(common.join("sifttext-publications").as_path())
            || parent.parent() == Some(crate::scratch::publications(&common)?.as_path()))
            && parent
                .file_name()
                .is_some_and(|n| n.as_encoded_bytes().starts_with(b"submission-"))
            && fs::canonicalize(parent)? == parent,
        "unsafe ownership path",
    )?;
    require(
        fs::symlink_metadata(parent.join("owner.json"))?.len() <= 4096,
        "oversized ownership record",
    )?;
    let bytes = crate::read_regular(&parent.join("owner.json"))?;
    require(bytes.len() <= 4096, "oversized ownership record")?;
    let r: Record = serde_json::from_slice(&bytes)?;
    require(
        r.version == 1
            && r.common == common
            && r.repository == Identity::of(&common)?
            && r.parent == Identity::of(parent)?
            && super::oid(&r.revision)
            && ["disposable", "retained"].contains(&r.disposition.as_str())
            && (r.disposition != "retained" || r.initialized),
        "ownership mismatch",
    )?;
    if fs::symlink_metadata(parent.join("view")).is_ok() {
        require(
            r.view == Identity::of(&parent.join("view"))?
                && fs::canonicalize(parent.join("view"))? == parent.join("view"),
            "owned view reused",
        )?;
    }
    Ok(r)
}
fn note(
    notes: &Notes,
    parent: &Path,
    phase: &'static str,
    kind: &'static str,
    cleanup: &'static str,
) {
    // Bounded receipts; the on-disk record remains the per-view recovery source.
    if notes.borrow().len() < 16 {
        notes.borrow_mut().push(Note {
            phase,
            kind,
            record: parent.join("owner.json"),
            cleanup,
        });
    }
}
pub(super) fn error_kind(error: &dyn std::fmt::Display) -> &'static str {
    let s = error.to_string();
    if s.contains("save busy") {
        "busy"
    } else if s.contains("termination unconfirmed") {
        if s.contains("timeout") {
            "timeout_termination_unconfirmed"
        } else if s.contains("pipe") {
            "pipe_failure_termination_unconfirmed"
        } else if s.contains("resource limit") {
            "output_limit_termination_unconfirmed"
        } else {
            "termination_unconfirmed"
        }
    } else if s.contains("timeout") {
        "timeout"
    } else if s.contains("resource limit") {
        "output_limit"
    } else if s.contains("pipe") {
        "pipe_failure"
    } else if s.contains("Git operation rejected") {
        "git_nonzero"
    } else {
        "validation_or_io"
    }
}

pub(super) struct WorkingView {
    repo: PathBuf,
    parent: PathBuf,
    pub(super) path: PathBuf,
    custody: Option<Custody>,
    gate: Option<fs::File>,
    retained: bool,
    notes: Notes,
}
impl WorkingView {
    pub(super) fn new(repo: &Path, root: &Path, revision: &str, notes: &Notes) -> Result<Self> {
        Self::new_using(repo, root, revision, notes, |path| {
            git(
                repo,
                &[
                    "worktree",
                    "add",
                    "--quiet",
                    "--detach",
                    path.to_str().ok_or("non-UTF-8 view")?,
                    revision,
                ],
            )
            .map(|_| ())
        })
    }
    fn new_using(
        repo: &Path,
        root: &Path,
        revision: &str,
        notes: &Notes,
        add: impl FnOnce(&Path) -> Result<()>,
    ) -> Result<Self> {
        commit(repo, revision)?;
        let temp = tempfile::Builder::new()
            .prefix("submission-")
            .tempdir_in(root)?;
        let parent = fs::canonicalize(temp.path())?;
        let path = parent.join("view");
        fs::DirBuilder::new().mode(0o700).create(&path)?;
        let gate = crate::local::publication_gate(&path)?;
        let common = common(repo)?;
        let record = Record {
            version: 1,
            repository: Identity::of(&common)?,
            common,
            parent: Identity::of(&parent)?,
            view: Identity::of(&path)?,
            revision: revision.into(),
            disposition: "disposable".into(),
            initialized: false,
            last_failure: None,
        };
        persist(&parent, &record)?;
        // Commit the new operation/root directory entries too, not just the
        // record's contents and its immediate containing directory.
        fs::File::open(root)?.sync_all()?;
        fs::File::open(&record.common)?.sync_all()?;
        // From here, only coupled exact Git finish retires this directory.
        let _ = temp.keep();
        let mut owned = Self {
            repo: repo.into(),
            parent,
            path,
            custody: Some(Custody::new(&[&gate])?),
            gate: Some(gate),
            retained: false,
            notes: notes.clone(),
        };
        let result = add(&owned.path);
        match result {
            Ok(()) => {
                let mut r = load(repo, &owned.parent)?;
                r.initialized = true;
                persist(&owned.parent, &r)?;
                Ok(owned)
            }
            Err(error) => {
                if let Ok(mut r) = load(repo, &owned.parent) {
                    r.last_failure = Some(format!("setup:{}", error_kind(&error)));
                    let _ = persist(&owned.parent, &r);
                }
                let cleanup = owned.finish();
                note(
                    notes,
                    &owned.parent,
                    "setup",
                    error_kind(&error),
                    disposition(&cleanup),
                );
                owned.retained = true; // finish already attempted; never erase evidence via TempDir.
                Err(error)
            }
        }
    }
    pub(super) fn revision(&self, revision: &str) -> Result<()> {
        let mut record = load(&self.repo, &self.parent)?;
        record.revision = revision.into();
        persist(&self.parent, &record)
    }
    pub(super) fn retain(mut self, revision: &str) -> Result<PathBuf> {
        let mut r = load(&self.repo, &self.parent)?;
        clean(&self.path, revision)?;
        r.revision = revision.into();
        r.disposition = "retained".into();
        persist(&self.parent, &r)?;
        self.retained = true;
        Ok(self.path.clone())
    }
    fn finish(&mut self) -> Result<()> {
        // Handoff to a different open description: a surviving inherited helper
        // keeps this busy after our copies close. Never unlock a shared OFD.
        self.custody.take();
        self.gate.take();
        finish(&self.repo, &self.parent, false, true)
    }
}
impl Drop for WorkingView {
    fn drop(&mut self) {
        if !self.retained {
            let result = self.finish();
            note(
                &self.notes,
                &self.parent,
                "finish",
                if result.is_ok() {
                    "none"
                } else {
                    "cleanup_refused"
                },
                disposition(&result),
            );
        }
    }
}

// Revalidate registry backlink directly, without following an administrative
// symlink. An absent view still needs its exact registration removed by Git.
fn registration(repo: &Path, parent: &Path, r: &Record) -> Result<bool> {
    let view = parent.join("view");
    let listing = git(repo, &["worktree", "list", "--porcelain", "-z"])?;
    let wanted = format!("worktree {}", view.to_str().ok_or("non-UTF-8 view")?);
    let entries: Vec<_> = listing
        .split("\0\0")
        .filter(|e| e.split('\0').any(|line| line == wanted))
        .collect();
    require(entries.len() <= 1, "ambiguous registration")?;
    let mut backlinks = Vec::new();
    let adminroot = r.common.join("worktrees");
    if adminroot.try_exists()? {
        crate::real_dir(&adminroot)?;
        for entry in fs::read_dir(&adminroot)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let admin = entry.path();
            if crate::read_regular(&admin.join("gitdir"))
                .is_ok_and(|b| b == format!("{}\n", view.join(".git").display()).as_bytes())
            {
                backlinks.push(admin);
            }
        }
    }
    if entries.is_empty() {
        require(backlinks.is_empty(), "unclassifiable partial registration")?;
        return Ok(false);
    }
    require(backlinks.len() == 1, "registration backlink changed")?;
    let admin = &backlinks[0];
    require(
        crate::read_regular(&admin.join("HEAD"))? == format!("{}\n", r.revision).as_bytes()
            && !entries[0].split('\0').any(|s| s.starts_with("branch "))
            && entries[0]
                .split('\0')
                .any(|s| s == format!("HEAD {}", r.revision)),
        "owned registration acquired a branch or revision",
    )?;
    if fs::symlink_metadata(admin.join("locked")).is_ok() {
        require(
            !r.initialized && crate::read_regular(&admin.join("locked"))? == b"initializing\n",
            "unexpected worktree lock",
        )?;
    }
    if view.try_exists()? {
        require(
            crate::read_regular(&view.join(".git"))?
                == format!("gitdir: {}\n", admin.display()).as_bytes(),
            "view registration pointer changed",
        )?;
    }
    Ok(true)
}

fn finish(repo: &Path, parent: &Path, retained: bool, own_handoff: bool) -> Result<()> {
    let view = parent.join("view");
    if fs::symlink_metadata(parent).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound) {
        let common = common(repo)?;
        let legacy = common.join("sifttext-publications");
        let root = crate::scratch::publications(&common)?;
        let admins = common.join("worktrees");
        if admins.try_exists()? {
            crate::real_dir(&admins)?;
            for entry in fs::read_dir(&admins)? {
                let entry = entry?;
                if entry.file_type()?.is_dir() {
                    require(
                        !crate::read_regular(&entry.path().join("gitdir")).is_ok_and(|b| {
                            b == format!("{}\n", view.join(".git").display()).as_bytes()
                        }),
                        "unlisted registration remains without ownership record",
                    )?;
                }
            }
        }
        require(
            (parent.parent() == Some(root.as_path()) || parent.parent() == Some(legacy.as_path()))
                && fs::canonicalize(parent.parent().ok_or("missing operation root")?)? == parent.parent().unwrap()
                && parent
                    .file_name()
                    .is_some_and(|n| n.as_encoded_bytes().starts_with(b"submission-")),
            "unsafe completed operation path",
        )?;
        let listing = git(repo, &["worktree", "list", "--porcelain", "-z"])?;
        require(
            !listing
                .split('\0')
                .any(|line| line == format!("worktree {}", view.display())),
            "registration remains without ownership record",
        )?;
        return Ok(()); // idempotent no-op: neither filesystem nor Git piece remains
    }
    let start = std::time::Instant::now();
    let gate = loop {
        match crate::local::publication_gate(&view) {
            Ok(gate) => break gate,
            // Concurrent fork/exec can briefly inherit CLOEXEC handles from
            // another thread. Only our immediate finish tolerates that handoff;
            // operation-boundary recovery always skips busy paths immediately.
            Err(e)
                if own_handoff
                    && e.to_string().contains("save busy")
                    && start.elapsed() < std::time::Duration::from_millis(50) =>
            {
                std::thread::sleep(std::time::Duration::from_millis(2))
            }
            Err(e) => return Err(e),
        }
    };
    let r = load(repo, parent)?;
    require(
        r.disposition == if retained { "retained" } else { "disposable" },
        "view disposition changed",
    )?;
    for entry in fs::read_dir(parent)? {
        require(
            ["view", "owner.json"]
                .iter()
                .any(|n| entry.as_ref().is_ok_and(|e| e.file_name() == *n)),
            "unexpected operation file",
        )?;
    }
    let _custody = Custody::new(&[&gate])?;
    let registered = registration(repo, parent, &r)?;
    if registered {
        if view.try_exists()? {
            if r.initialized {
                clean(&view, &r.revision)?;
            } else {
                // Failed setup may have no usable index. Only authored paths
                // from the requested tree may exist; arbitrary user bytes outside
                // that set refuse stronger disposal. Never recurse through links.
                partial_paths(repo, &view, &r.revision)?;
            }
        }
        let mut args = vec!["worktree", "remove"];
        if !r.initialized {
            args.extend(["--force", "--force"]);
        }
        args.push(view.to_str().ok_or("non-UTF-8 view")?);
        git(repo, &args)?;
        require(!registration(repo, parent, &r)?, "Git registration remains")?;
    } else if view.try_exists()? {
        fs::remove_dir(&view)?; // only an empty reserved directory, not rm -rf
    }
    require(!view.try_exists()?, "view remains after removal")?;
    fs::remove_file(parent.join("owner.json"))?;
    fs::File::open(parent)?.sync_all()?;
    if let Err(error) = fs::remove_dir(parent) {
        let _ = persist(parent, &r);
        return Err(error.into());
    }
    fs::File::open(parent.parent().ok_or("missing operation root")?)?.sync_all()?;
    Ok(())
}
fn partial_paths(repo: &Path, view: &Path, revision: &str) -> Result<()> {
    let out = run(repo, &["ls-tree", "-rz", "--name-only", revision])?;
    require(out.status.success(), "partial tree unavailable")?;
    let allowed: std::collections::BTreeSet<_> = out
        .stdout
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
        .map(|s| PathBuf::from(std::str::from_utf8(s).unwrap_or("")))
        .collect();
    fn walk(root: &Path, dir: &Path, allowed: &std::collections::BTreeSet<PathBuf>) -> Result<()> {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let rel = path.strip_prefix(root)?;
            if rel == Path::new(".git") {
                require(entry.file_type()?.is_file(), "partial Git pointer replaced")?;
                continue;
            }
            if entry.file_type()?.is_dir() {
                require(
                    allowed.iter().any(|p| p.starts_with(rel)),
                    "unexpected partial directory",
                )?;
                walk(root, &path, allowed)?;
            } else {
                require(allowed.contains(rel), "unexpected partial user file")?;
            }
        }
        Ok(())
    }
    walk(view, view, &allowed)
}

/// Expiry discards inactive view files, never accepted Git history. The
/// existing ownership record and lifecycle gate still select the exact target.
pub(super) fn expire(root: &Path, now: std::time::SystemTime) -> Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() { continue; }
        let parent = entry.path();
        let result = (|| -> Result<()> {
            if !crate::scratch::inactive(&parent, now)? { return Ok(()); }
            require(fs::symlink_metadata(parent.join("owner.json"))?.len() <= 4096,
                "oversized ownership record")?;
            let bytes = crate::read_regular(&parent.join("owner.json"))?;
            require(bytes.len() <= 4096, "oversized ownership record")?;
            let identity: Record = serde_json::from_slice(&bytes)?;
            let repo = &identity.common;
            require(root == crate::scratch::publications(repo)?, "foreign scratch root")?;
            let view = parent.join("view");
            let gate = crate::local::publication_gate(&view)?;
            let _custody = Custody::new(&[&gate])?;
            let record = load(repo, &parent)?;
            if !crate::scratch::inactive(&parent, now)? { return Ok(()); }
            if registration(repo, &parent, &record)? {
                if view.join(".git").try_exists()? {
                    let admin = PathBuf::from(git(&view, &["rev-parse", "--absolute-git-dir"])?);
                    if admin.join("index.lock").try_exists()? || !crate::scratch::inactive(&admin, now)? {
                        return Ok(());
                    }
                }
                let mut args = vec!["worktree", "remove", "--force"];
                // Only the exact setup lock is eligible for stronger disposal.
                if !record.initialized { args.push("--force"); }
                args.push(view.to_str().ok_or("non-UTF-8 view")?);
                git(repo, &args)?;
            }
            fs::remove_dir_all(&parent)?;
            Ok(())
        })();
        if let Err(error) = result { eprintln!("scratch view retained at {}: {error}", parent.display()); }
    }
    Ok(())
}

pub(super) fn recover(repo: &Path, root: &Path, notes: &Notes) -> Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let parent = entry.path();
        if !parent.join("owner.json").try_exists()? {
            note(notes, &parent, "recovery", "ownership_unknown", "preserved");
            continue;
        }
        match load(repo, &parent) {
            Ok(r) if r.disposition == "retained" => {}
            Ok(_) => {
                let result = finish(repo, &parent, false, false);
                note(
                    notes,
                    &parent,
                    "recovery",
                    if result.is_ok() {
                        "none"
                    } else {
                        "cleanup_refused"
                    },
                    disposition(&result),
                );
            }
            Err(_) => note(
                notes,
                &parent,
                "recovery",
                "ownership_mismatch",
                "preserved",
            ),
        }
    }
    Ok(())
}
pub(super) fn remove_returned(
    repo: &Path,
    receipt: &super::Receipt,
    identity: &fs::Metadata,
) -> Result<()> {
    require(receipt.status == "current", "unverified cleanup view")?;
    let view = receipt.view.as_ref().ok_or("missing owned view")?;
    let parent = view.parent().ok_or("missing owned parent")?;
    if let Ok(m) = fs::symlink_metadata(view) {
        require(
            m.is_dir() && m.dev() == identity.dev() && m.ino() == identity.ino(),
            "owned view identity changed",
        )?;
    }
    require(
        view.file_name().is_some_and(|n| n == "view"),
        "unsafe owned view name",
    )?;
    if fs::symlink_metadata(parent).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound) {
        return finish(repo, parent, true, true);
    }
    let r = load(repo, parent)?;
    require(
        r.view
            == Identity {
                dev: identity.dev(),
                ino: identity.ino(),
            },
        "owned view physical identity changed",
    )?;
    require(
        Some(r.revision.as_str()) == receipt.candidate.as_deref(),
        "owned view revision changed",
    )?;
    finish(repo, parent, true, true)
}

#[cfg(test)]
#[path = "../../tests/successor/publication_owned.rs"]
mod tests;
