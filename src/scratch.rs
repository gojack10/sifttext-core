//! Disposable operation storage in the platform temp directory. Git history and
//! submission identity remain in the repository, never in this namespace.
use crate::{Result, require};
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

const EXPIRY: Duration = Duration::from_secs(48 * 60 * 60);

fn private_directory(path: &Path) -> Result<()> {
    if let Err(error) = fs::DirBuilder::new().mode(0o700).create(path)
        && error.kind() != std::io::ErrorKind::AlreadyExists
    {
        return Err(error.into());
    }
    let metadata = fs::symlink_metadata(path)?;
    require(metadata.is_dir() && metadata.uid() == unsafe { libc::geteuid() }
        && metadata.mode() & 0o777 == 0o700 && fs::canonicalize(path)? == path,
        "unsafe operation directory")
}

fn root() -> Result<PathBuf> {
    let root = fs::canonicalize(std::env::temp_dir())?
        .join(format!("sifttext-operations-{}", unsafe { libc::geteuid() }));
    private_directory(&root)?;
    Ok(root)
}

pub(crate) fn drafts() -> Result<PathBuf> {
    let path = root()?.join("drafts");
    private_directory(&path)?;
    Ok(path)
}

pub(crate) fn publications(common: &Path) -> Result<PathBuf> {
    let path = root()?.join(format!("publication-{}", crate::sha256(common.as_os_str().as_encoded_bytes())));
    private_directory(&path)?;
    Ok(path)
}

fn open_gate(path: &Path) -> Result<fs::File> {
    let file = fs::OpenOptions::new().read(true).write(true).create(true).truncate(false)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK).mode(0o600).open(path)?;
    let metadata = file.metadata()?;
    require(metadata.is_file() && metadata.nlink() == 1
        && metadata.uid() == unsafe { libc::geteuid() } && metadata.mode() & 0o777 == 0o600,
        "unsafe operation lease")?;
    Ok(file)
}

/// A live lease protects long-running operations even when their files are old.
/// Git helpers inherit custody. Normal completion removes the entire directory;
/// an interrupted process leaves rebuildable files for startup expiry.
pub(crate) struct Directory {
    custody: Option<crate::checked_publication::Custody>,
    gate: Option<fs::File>,
    directory: Option<tempfile::TempDir>,
    files: PathBuf,
}
impl Directory {
    pub(crate) fn new() -> Result<Self> {
        let directory = tempfile::Builder::new().prefix("binary-").tempdir_in(root()?)?;
        let gate = open_gate(&directory.path().join("lease"))?;
        gate.lock()?;
        let custody = crate::checked_publication::Custody::new(&[&gate])?;
        let files = directory.path().join("files");
        private_directory(&files)?;
        Ok(Self { custody: Some(custody), gate: Some(gate), directory: Some(directory), files })
    }
    pub(crate) fn path(&self) -> &Path {
        &self.files
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        self.custody.take();
        self.gate.take();
        if let Some(directory) = self.directory.take() {
            // A surviving child still owns the inherited lease. Do not unlink
            // its working files just because the original caller went away.
            match open_gate(&directory.path().join("lease")) {
                Ok(gate) if gate.try_lock().is_ok() => { drop(directory); }
                _ => { let _ = directory.keep(); }
            }
        }
    }
}

pub(crate) fn inactive(path: &Path, now: SystemTime) -> Result<bool> {
    let metadata = fs::symlink_metadata(path)?;
    if now.duration_since(metadata.modified()?).unwrap_or_default() < EXPIRY {
        return Ok(false);
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            // Never follow links into a knowledge home or artifact repository.
            if !inactive(&entry.path(), now)? { return Ok(false); }
        }
    }
    Ok(true)
}

/// Reader upgrades refresh generated files even in abandoned legacy drafts.
/// Use the working files and Git activity, ignoring only known generated paths.
pub(crate) fn inactive_legacy_draft(root: &Path, path: &Path, now: SystemTime) -> Result<bool> {
    let relative = path.strip_prefix(root)?;
    let parts: Vec<_> = relative.iter().map(|part| part.to_str().unwrap_or_default()).collect();
    if matches!(parts.as_slice(), [".sifttext"])
        || matches!(parts.as_slice(), ["nodes", id,
            "outline.lean" | "outline.full.lean" | "outline.lean.meta.json"
                | "outline.full.lean.meta.json" | "links-to" | "linked-from" | "parent"]
            if crate::uuid(id))
    {
        return Ok(true);
    }
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir() {
        return Ok(now.duration_since(metadata.modified()?).unwrap_or_default() >= EXPIRY);
    }
    for entry in fs::read_dir(path)? {
        if !inactive_legacy_draft(root, &entry?.path(), now)? { return Ok(false); }
    }
    Ok(true)
}

fn cleanup_gate(path: &Path, now: SystemTime) -> Result<Option<fs::File>> {
    let gate = open_gate(path)?;
    if gate.try_lock().is_err()
        || (gate.metadata()?.len() != 0
            && now.duration_since(gate.metadata()?.modified()?).unwrap_or_default()
                < Duration::from_secs(3600))
    {
        return Ok(None);
    }
    Ok(Some(gate))
}

/// Repository discovery already belongs to publication. Reuse it rather than
/// scanning homes or adding a second command-line dispatcher for old views.
pub(crate) fn cleanup_legacy_publications(common: &Path, immediate: bool) {
    let result = (|| -> Result<()> {
        let legacy = common.join("sifttext-publications");
        if !legacy.try_exists()? { return Ok(()); }
        crate::real_dir(&legacy)?;
        let now = SystemTime::now();
        let path = publications(common)?.join("legacy-cleanup");
        let gate = if immediate {
            let gate = open_gate(&path)?;
            if gate.try_lock().is_err() { return Ok(()); }
            gate
        } else {
            let Some(gate) = cleanup_gate(&path, now)? else { return Ok(()); };
            gate
        };
        crate::checked_publication::expire_scratch_views(&legacy, now)?;
        if fs::read_dir(&legacy)?.next().is_none() {
            fs::remove_dir(&legacy)?;
        }
        gate.set_len(1)?;
        gate.set_modified(now)?;
        Ok(())
    })();
    if let Err(error) = result {
        eprintln!("legacy publication cleanup deferred: {error}");
    }
}

/// Retire the former persistent memo during reader preparation or sync.
pub fn retire_legacy_facts(repo: &Path) {
    let result = (|| -> Result<()> {
        let common = crate::local::reader_git(repo,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"])?;
        let common = Path::new(&common);
        crate::real_dir(common)?;
        for name in [
            "sifttext-git-facts.sqlite-journal",
            "sifttext-git-facts.sqlite-wal",
            "sifttext-git-facts.sqlite-shm",
            "sifttext-git-facts.sqlite",
        ] {
            let file = common.join(name);
            match std::fs::symlink_metadata(&file) {
                Ok(metadata) => {
                    use std::os::unix::fs::MetadataExt;
                    require(metadata.is_file() && metadata.nlink() == 1,
                        "unrecognized legacy Git facts file")?;
                    std::fs::remove_file(file)?;
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    })();
    if let Err(error) = result {
        eprintln!("Legacy Git facts retained in {}: {error}", repo.display());
    }
}

/// Retire stale registrations belonging to this independent knowledge home.
pub fn retire_repository_worktrees(home: &Path) -> Result<()> {
    let home = fs::canonicalize(home)?;
    let common = home.join(".git");
    crate::real_dir(&common)?;
    cleanup_legacy_publications(&common, true);
    crate::local::expire_repository_drafts(&common, SystemTime::now())
}

/// Opportunistic startup maintenance, not a daemon or a publication preflight.
/// One fixed bookkeeping file throttles scans to hourly; no per-run ledger grows.
/// Failures retain the affected directory and are reported without blocking use.
pub fn cleanup() {
    if let Err(error) = cleanup_inner() {
        eprintln!("scratch cleanup deferred: {error}");
    }
}
fn cleanup_inner() -> Result<()> {
    let root = root()?;
    let now = SystemTime::now();
    let Some(gate) = cleanup_gate(&root.join("cleanup"), now)? else {
        return Ok(());
    };
    for entry in fs::read_dir(&root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() { continue; }
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let result = (|| -> Result<()> {
            private_directory(&path)?;
            if name.starts_with("binary-") && inactive(&path, now)? {
                let lease = open_gate(&path.join("lease"))?;
                if lease.try_lock().is_ok() && inactive(&path, now)? {
                    fs::remove_dir_all(&path)?;
                }
            } else if name == "drafts" {
                for draft in fs::read_dir(&path)? {
                    let draft = draft?;
                    if draft.file_type()?.is_dir()
                        && draft.file_name().to_string_lossy().starts_with("sifttext-worktree-")
                        && inactive(&draft.path(), now)?
                    {
                        if let Err(error) = crate::local::expire_draft(&draft.path(), now) {
                            eprintln!("scratch draft retained at {}: {error}", draft.path().display());
                        }
                    }
                }
            } else if name.starts_with("publication-") {
                crate::checked_publication::expire_scratch_views(&path, now)?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            eprintln!("scratch retained at {}: {error}", path.display());
        }
    }
    gate.set_len(1)?;
    gate.set_modified(now)?;
    Ok(())
}
