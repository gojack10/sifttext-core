//! CLI completion over the checked same-draft publisher. Journals are custody,
//! never acceptance evidence: every retry rechecks the accepted Git result.
use super::{advance_checked, reader_git, reader_state};
use crate::{Policy, Result, checked_publication, require};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::Write,
    os::unix::fs::OpenOptionsExt,
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct Pending {
    home: PathBuf,
    draft: PathBuf,
    commit: String,
    base: String,
    branch: String,
    explicit: BTreeSet<String>,
    // Exact Core-created transition; never permission for arbitrary draft edits.
    #[serde(default)]
    prepared_heads: Option<(String, String)>,
    // Persist before removal so an interrupted branch deletion is recoverable.
    completed_head: Option<String>,
}

fn lock(path: &Path, wait: bool) -> Result<fs::File> {
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .mode(0o600)
        .open(path)?;
    use std::os::unix::fs::MetadataExt;
    require(
        file.metadata()?.is_file() && file.metadata()?.nlink() == 1,
        "unsafe save lock",
    )?;
    if wait {
        file.lock()?;
    } else {
        file.try_lock()
            .map_err(|e| format!("save busy; retain draft and retry: {e}"))?;
    }
    Ok(file)
}

// Common Git metadata survives worktree removal. Never unlink these lock files:
// a waiter must acquire the same inode even after the submitted draft disappears.
fn save_root(repo: &Path) -> Result<PathBuf> {
    let repo = fs::canonicalize(repo)?;
    let common = PathBuf::from(reader_git(
        &repo,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?);
    crate::real_dir(&common)?;
    require(fs::canonicalize(&common)? == common, "noncanonical Git metadata")?;
    let root = common.join("sifttext-saves");
    if let Err(error) = fs::create_dir(&root)
        && error.kind() != std::io::ErrorKind::AlreadyExists
    {
        return Err(error.into());
    }
    crate::real_dir(&root)?;
    Ok(root)
}

fn draft_key(draft: &Path) -> Result<String> {
    use std::path::Component;
    require(
        draft.is_absolute()
            && draft.components().all(|c| matches!(c, Component::RootDir | Component::Normal(_))),
        "draft must be an absolute canonical path",
    )?;
    // Resolve parent aliases, not the last component: a completed draft path
    // reused as a symlink must retain the original journal/lock identity.
    let draft = fs::canonicalize(draft.parent().ok_or("missing draft parent")?)?
        .join(draft.file_name().ok_or("missing draft name")?);
    Ok(format!("{:x}", Sha256::digest(draft.as_os_str().as_encoded_bytes())))
}

// Discoverable without .git or the Store: recursive Git removal can unlink
// .git before removing the root. This UID-private, per-path gate is always
// acquired first, including by genuine standalone stores. Never unlink gates
// or their directory (even after cleanup); waiters retain the same inode.
// /tmp is intentional, not caller-specific TMPDIR/HOME. Clearing this internal
// namespace requires quiescing all native mutators for this UID first.
fn lifecycle_root() -> Result<PathBuf> {
    use std::os::unix::fs::{DirBuilderExt, MetadataExt};
    let parent = fs::canonicalize("/tmp")?;
    let metadata = fs::symlink_metadata(&parent)?;
    require(metadata.is_dir() && metadata.uid() == 0 && metadata.mode() & 0o1000 != 0,
        "unsafe lifecycle gate parent")?;
    let root = parent.join(format!("sifttext-lifecycle-{}", unsafe { libc::geteuid() }));
    if let Err(error) = fs::DirBuilder::new().mode(0o700).create(&root)
        && error.kind() != std::io::ErrorKind::AlreadyExists
    {
        return Err(error.into());
    }
    check_lifecycle_directory(&root)?;
    Ok(root)
}

fn check_lifecycle_directory(root: &Path) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    let metadata = fs::symlink_metadata(root)?;
    require(metadata.is_dir() && metadata.uid() == unsafe { libc::geteuid() }
        && metadata.mode() & 0o777 == 0o700 && fs::canonicalize(root)? == root,
        "unsafe lifecycle gate directory")
}

pub(super) fn lifecycle_lock(path: &Path, wait: bool) -> Result<fs::File> {
    use std::os::unix::fs::MetadataExt;
    let path = std::path::absolute(path)?;
    // Canonicalize parent aliases, never a reused final symlink. Session APIs
    // also accept relative roots; publication retains its stricter path checks.
    let path = match path.file_name() {
        Some(name) => fs::canonicalize(path.parent().ok_or("missing root parent")?)?.join(name),
        None => fs::canonicalize(&path)?,
    };
    let key = format!("{:x}", Sha256::digest(path.as_os_str().as_encoded_bytes()));
    let root = lifecycle_root()?;
    let lock_path = root.join(format!("{key}.lock"));
    let file = lock(&lock_path, wait)?;
    let metadata = file.metadata()?;
    let named = fs::symlink_metadata(&lock_path)?;
    require(metadata.uid() == unsafe { libc::geteuid() }
        && metadata.mode() & 0o777 == 0o600
        && (metadata.dev(), metadata.ino()) == (named.dev(), named.ino()),
        "unsafe lifecycle gate file")?;
    Ok(file)
}

// A retained bit also fails closed after interrupted partial deletion. A path
// once used by Git is not silently reclassified as a fresh standalone store.
pub(super) fn mark_git_lifecycle(file: &mut fs::File) -> Result<()> {
    if file.metadata()?.len() == 0 {
        file.write_all(b"G")?;
        file.sync_all()?;
    }
    Ok(())
}

pub(super) fn draft_lock(repo: &Path, draft: &Path, wait: bool) -> Result<fs::File> {
    lock(&save_root(repo)?.join(format!("{}.lock", draft_key(draft)?)), wait)
}

pub(super) fn home_lock(home: &Path, wait: bool) -> Result<fs::File> {
    lock(&save_root(home)?.join("home.lock"), wait)
}

// Only this module can construct a lease. Checked publication borrows it rather
// than recursively acquiring the same lock; completion retains it through removal.
pub(crate) struct DraftLease {
    _lifecycle: fs::File,
    _file: fs::File,
    _home_file: Option<fs::File>,
    root: PathBuf,
    key: String,
}
impl DraftLease {
    pub(crate) fn acquire(repo: &Path, draft: &Path) -> Result<Self> {
        let mut lifecycle = lifecycle_lock(draft, false)?;
        let root = save_root(repo)?;
        let key = draft_key(draft)?;
        let file = lock(&root.join(format!("{key}.lock")), false)?;
        if fs::symlink_metadata(draft.join(".git")).is_ok() {
            mark_git_lifecycle(&mut lifecycle)?;
        }
        // The low-level API also accepts a branch-owned primary worktree as a
        // draft. Its session operations use home.lock, unlike linked drafts.
        let home_file = if fs::symlink_metadata(draft).is_ok_and(|m| m.is_dir())
            && fs::symlink_metadata(draft.join(".git")).is_ok()
            && Path::new(&reader_git(draft, &["rev-parse", "--absolute-git-dir"])?)
                == root.parent().ok_or("missing common Git directory")?
        {
            Some(lock(&root.join("home.lock"), false)?)
        } else {
            None
        };
        Ok(Self { _lifecycle: lifecycle, _file: file, _home_file: home_file, root, key })
    }

    pub(crate) fn check(&self, repo: &Path, draft: Option<&Path>) -> Result<()> {
        require(
            draft.map(draft_key).transpose()?.as_deref() == Some(self.key.as_str())
                && save_root(repo)? == self.root,
            "publication lease does not match submitted draft",
        )
    }
}

fn persist(path: &Path, pending: &Pending) -> Result<()> {
    let parent = path.parent().ok_or("missing journal parent")?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(&serde_json::to_vec(pending)?)?;
    temp.as_file().sync_all()?;
    temp.persist(path)?;
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

pub fn submit(policy: &Policy, request: &checked_publication::Request) -> Result<Value> {
    submit_inner(policy, request, None, || Ok(())).map_err(|error| {
        format!("submission completion unknown unless reported below; retain files and original submit arguments for retry: {error}").into()
    })
}

pub fn submit_reviewed(
    policy: &Policy,
    request: &checked_publication::Request,
    mode: checked_publication::ReviewMode,
    rules: &[u8],
    mut review: impl FnMut(&str, &[u8], &[u8], &[u8]) -> Result<checked_publication::SemanticVerdict>,
) -> Result<Value> {
    require(request.draft.is_some(), "review requires a submitted draft")?;
    submit_inner(policy, request, Some((mode, rules, &mut review)), || Ok(()))
}

fn submit_inner(
    policy: &Policy,
    request: &checked_publication::Request,
    mut review: Option<(
        checked_publication::ReviewMode,
        &[u8],
        &mut dyn FnMut(&str, &[u8], &[u8], &[u8]) -> Result<checked_publication::SemanticVerdict>,
    )>,
    before_return: impl FnOnce() -> Result<()>,
) -> Result<Value> {
    require(
        request.accepted_ref == "refs/heads/main",
        "unsupported home accepted ref",
    )?;
    let home = &request.repo;
    let draft = request.draft.as_ref().ok_or("missing draft")?;
    require(draft.is_absolute(), "draft must be absolute")?;
    let commit = request.commit.as_ref().ok_or("missing submitted commit")?;
    let base = request.base.as_ref().ok_or("missing submitted base")?;
    let draft_lease = DraftLease::acquire(home, draft)?;
    reader_state(home)?; // Recheck after acquiring custody, before publication.
    let root = save_root(home)?;
    let key = draft_key(draft)?;
    let journal = root.join(format!("{key}.json"));
    let mut pending: Pending = if journal.try_exists()? {
        serde_json::from_slice(&crate::read_regular(&journal)?)?
    } else {
        crate::real_dir(draft)?;
        require(
            fs::canonicalize(draft)? == *draft,
            "draft must be canonical",
        )?;
        require(
            reader_git(draft, &["rev-parse", "--show-toplevel"])?
                == draft.to_str().ok_or("non-UTF-8 draft")?,
            "not a draft root",
        )?;
        require(
            reader_git(
                draft,
                &["rev-parse", "--path-format=absolute", "--git-common-dir"],
            )? == home.join(".git").to_str().ok_or("non-UTF-8 home")?,
            "cross-repository draft",
        )?;
        let branch = reader_git(draft, &["symbolic-ref", "--quiet", "HEAD"])?;
        require(
            branch.starts_with("refs/heads/sifttext/"),
            "submit cleanup requires an allocated temporary branch",
        )?;
        let record = Pending {
            home: home.clone(),
            draft: draft.clone(),
            commit: commit.clone(),
            base: base.clone(),
            branch,
            explicit: request.explicit.clone(),
            prepared_heads: None,
            completed_head: None,
        };
        persist(&journal, &record)?;
        record
    };
    require(
        pending.home == *home
            && pending.draft == *draft
            && pending.commit == *commit
            && pending.base == *base
            && pending.explicit == request.explicit,
        "different submission is pending; retain files and resume its original request",
    )?;
    require(
        pending.branch.starts_with("refs/heads/sifttext/"),
        "unsafe pending branch",
    )?;

    let missing =
        fs::symlink_metadata(draft).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound);
    let receipt = if missing {
        require(
            pending.completed_head.is_some(),
            "missing uncompleted draft; retain journal",
        )?;
        checked_publication::submit(
            policy,
            &checked_publication::Request {
                repo: home.clone(),
                accepted_ref: request.accepted_ref.clone(),
                draft: None,
                commit: None,
                base: None,
                explicit: BTreeSet::new(),
                pause_before_publish: None,
                pause_after_publish: None,
            },
        )
    } else {
        let previous_heads = pending.prepared_heads.clone();
        let record = |from: &str, to: &str| {
            pending.prepared_heads = Some((from.to_owned(), to.to_owned()));
            persist(&journal, &pending)
        };
        let receipt = if let Some((mode, rules, reviewer)) = review.as_mut() {
            checked_publication::save_prepared_reviewed_leased(
                policy,
                request,
                &draft_lease,
                *mode,
                *rules,
                previous_heads
                    .as_ref()
                    .map(|(from, to)| (from.as_str(), to.as_str())),
                &mut **reviewer,
                record,
            )
        } else {
            checked_publication::save_prepared_leased(
                policy,
                request,
                &draft_lease,
                previous_heads
                    .as_ref()
                    .map(|(from, to)| (from.as_str(), to.as_str())),
                record,
            )
        };
        if receipt.status != "current" {
            // A refused retry can follow earlier acceptance. This invocation's
            // published=false says nothing about that prior attempt.
            let accepted = receipt.accepted_before.as_deref().filter(|accepted| {
                checked_publication::commit(home, commit).is_ok()
                    && reader_git(home, &["merge-base", "--is-ancestor", commit, accepted]).is_ok()
            });
            if !receipt.published && pending.completed_head.is_none() && accepted.is_none() {
                // An uncompleted refusal returns to explicit repair/resubmission.
                // Completed custody must survive a reused path's refusal so the
                // original request remains recoverable after that path clears.
                fs::remove_file(&journal)?;
                fs::File::open(&root)?.sync_all()?;
            }
            let mut output = serde_json::to_value(&receipt)?;
            output["home"] = json!(home);
            output["draft"] = json!(draft);
            output["publication"] = json!(if accepted.is_some() {
                "accepted_incomplete"
            } else if receipt.accepted_before.is_some()
                && receipt.candidate.is_some()
                && receipt.check != "conditional_ref_race"
            {
                "not_accepted"
            } else {
                "unknown"
            });
            if let Some(accepted) = accepted {
                output["accepted"] = json!(accepted);
            }
            return Ok(output);
        }
        receipt
    };
    require(
        receipt.status == "current",
        "checked save recovery unavailable; retain journal",
    )?;
    let target = receipt
        .candidate
        .as_deref()
        .ok_or("missing checked candidate")?;
    let version = receipt
        .version
        .as_deref()
        .ok_or("missing checked version")?;
    // Save preparation stays concurrent; serialize only normal-home finishing.
    let finish = || -> Result<Value> {
        let _home_lock = home_lock(home, false)?;
        let before = reader_state(home)?;
        let advanced = advance_checked(policy, home, &before, target, version, None, || Ok(()))?;
        require(
            reader_state(home)? == target
                && reader_git(home, &["rev-parse", "refs/heads/main"])? == target,
            "home changed before cleanup; retry",
        )?;
        if !missing {
            checked_publication::clean(draft, target)?;
            require(
                reader_git(draft, &["symbolic-ref", "--quiet", "HEAD"])? == pending.branch,
                "completed draft changed; preserve it",
            )?;
            pending.completed_head = Some(target.to_owned());
            persist(&journal, &pending)?;
            reader_git(
                home,
                &[
                    "worktree",
                    "remove",
                    draft.to_str().ok_or("non-UTF-8 draft")?,
                ],
            )?;
        }
        let cleaned_head = pending
            .completed_head
            .as_deref()
            .ok_or("missing cleanup identity")?;
        reader_git(
            home,
            &["merge-base", "--is-ancestor", &pending.commit, cleaned_head],
        )?;
        reader_git(home, &["merge-base", "--is-ancestor", cleaned_head, target])?;
        if missing {
            // Git removes the directory before its registration. Resume only this
            // exact completed registration, never prune another agent's worktree.
            let registered = reader_git(home, &["worktree", "list", "--porcelain", "-z"])?;
            let wanted = format!("worktree {}", draft.to_str().ok_or("non-UTF-8 draft")?);
            if let Some(entry) = registered
                .split("\0\0")
                .find(|entry| entry.split('\0').any(|row| row == wanted))
            {
                require(
                    entry
                        .split('\0')
                        .any(|row| row == format!("branch {}", pending.branch))
                        && entry
                            .split('\0')
                            .any(|row| row == format!("HEAD {cleaned_head}")),
                    "missing draft registration changed; preserve it",
                )?;
                reader_git(
                    home,
                    &[
                        "worktree",
                        "remove",
                        draft.to_str().ok_or("non-UTF-8 draft")?,
                    ],
                )?;
            }
        }
        let refs = reader_git(
            home,
            &["for-each-ref", "--format=%(refname)", &pending.branch],
        )?;
        if !refs.is_empty() {
            require(refs == pending.branch, "ambiguous cleanup branch")?;
            require(
                reader_git(
                    home,
                    &["for-each-ref", "--format=%(symref)", &pending.branch],
                )?
                .is_empty(),
                "symbolic cleanup branch",
            )?;
            require(
                !reader_git(home, &["worktree", "list", "--porcelain"])?
                    .lines()
                    .any(|line| line == format!("branch {}", pending.branch)),
                "cleanup branch acquired by another worktree",
            )?;
            reader_git(
                home,
                &[
                    "update-ref",
                    "--no-deref",
                    "-d",
                    &pending.branch,
                    cleaned_head,
                ],
            )?;
        }
        if missing {
            // This recovery's temporary checked view is ours, never another draft.
            reader_git(
                home,
                &[
                    "worktree",
                    "remove",
                    receipt
                        .view
                        .as_ref()
                        .ok_or("missing recovery view")?
                        .to_str()
                        .ok_or("non-UTF-8 recovery view")?,
                ],
            )?;
        }
        // ponytail: retain completed journals; GC needs an explicit retry-retention contract.
        // Retain completed custody as an idempotency tombstone. Losing stdout after
        // cleanup must not make the original request unrecoverable; retries still
        // validate current accepted history rather than trust this record as proof.
        let mut output = serde_json::to_value(&receipt)?;
        if missing {
            output["published"] = json!(false);
        }
        output["view"] = json!(home);
        output["home"] = json!(home);
        output["home_advance"] = advanced;
        output["worktree_removed"] = json!(true);
        before_return()?;
        Ok(output)
    };
    finish().map_err(|error| format!(
        "accepted save finishing incomplete (accepted {target}, version {version}, home {}, draft {}); home or cleanup may already have changed; retain files and retry the original submit request: {error}",
        home.display(), draft.display()
    ).into())
}

#[cfg(test)]
#[path = "../tests/successor/local_submission.rs"]
mod tests;
