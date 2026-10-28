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

pub(super) fn lifecycle_path(path: &Path) -> Result<PathBuf> {
    let path = std::path::absolute(path)?;
    // Path components strip trailing slash/dot syntax. Resolve parent aliases,
    // never a final symlink: validation must use this same gate spelling.
    Ok(match path.file_name() {
        Some(name) => fs::canonicalize(path.parent().ok_or("missing root parent")?)?.join(name),
        None => fs::canonicalize(&path)?,
    })
}

pub(super) fn lifecycle_lock(path: &Path, wait: bool) -> Result<fs::File> {
    use std::os::unix::fs::MetadataExt;
    let path = lifecycle_path(path)?;
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

pub(super) fn publication_lock(home: &Path) -> Result<fs::File> {
    lock(&save_root(home)?.join("publication.lock"), true)
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
        Self::acquire_once(repo, draft)
    }

    fn acquire_once(repo: &Path, draft: &Path) -> Result<Self> {
        // Ordinary custody contention waits; scratch recovery remains nonwaiting.
        let mut lifecycle = lifecycle_lock(draft, true)?;
        let root = save_root(repo)?;
        let key = draft_key(draft)?;
        let file = lock(&root.join(format!("{key}.lock")), true)?;
        if fs::symlink_metadata(draft.join(".git")).is_ok() {
            mark_git_lifecycle(&mut lifecycle)?;
        }
        // The low-level API also accepts a branch-owned primary worktree as a
        // draft. Its session operations use home.lock, unlike linked drafts.
        let home_file = if fs::symlink_metadata(draft).is_ok_and(|m| m.is_dir())
            && fs::symlink_metadata(draft.join(".git")).is_ok_and(|m| m.is_dir())
            && Path::new(&reader_git(draft, &["rev-parse", "--absolute-git-dir"])?)
                == root.parent().ok_or("missing common Git directory")?
        {
            Some(lock(&root.join("home.lock"), true)?)
        } else {
            None
        };
        Ok(Self { _lifecycle: lifecycle, _file: file, _home_file: home_file, root, key })
    }

    pub(crate) fn custody(&self) -> Result<checked_publication::Custody> {
        let mut files = vec![&self._lifecycle, &self._file];
        if let Some(home) = &self._home_file { files.push(home); }
        checked_publication::Custody::new(&files)
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
    submit_inner(policy, request, || Ok(())).map_err(|error| {
        format!("submission completion unknown unless reported below; retain files and original submit arguments for retry: {error}").into()
    })
}

/// Receive an authenticated writer's already-accepted contribution. Its status
/// changes are explicit; the combined candidate still passes the ordinary gate.
/// A retained native request always keeps its original, frozen intents.
pub fn submit_received(policy: &Policy, request: &checked_publication::Request) -> Result<Value> {
    submit_with_status(policy, request, true, || Ok(())).map_err(|error| {
        format!("received submission completion unknown; retain files and original arguments for retry: {error}").into()
    })
}

fn submit_inner(
    policy: &Policy,
    request: &checked_publication::Request,
    before_return: impl FnOnce() -> Result<()>,
) -> Result<Value> {
    submit_with_status(policy, request, false, before_return)
}

fn submit_with_status(
    policy: &Policy,
    request: &checked_publication::Request,
    received: bool,
    before_return: impl FnOnce() -> Result<()>,
) -> Result<Value> {
    let mut request = request.clone();
    require(
        request.accepted_ref == "refs/heads/main",
        "unsupported home accepted ref",
    )?;
    let home = &request.repo;
    let draft = request.draft.as_ref().ok_or("missing draft")?;
    require(draft.is_absolute(), "draft must be absolute")?;
    // CLI completion removes an allocated linked worktree. Refuse primary drafts
    // before queueing: their low-level lease also owns home.lock. Missing completed
    // drafts still recover through their journal below.
    if let Ok(metadata) = fs::symlink_metadata(draft.join(".git")) {
        require(metadata.is_file(), "submit cleanup requires an allocated linked private worktree")?;
    }
    let commit = request.commit.as_ref().ok_or("missing submitted commit")?;
    let base = request.base.as_ref().ok_or("missing submitted base")?;
    let draft_lease = DraftLease::acquire(home, draft)?;
    let _custody = draft_lease.custody()?;
    let publication_file = publication_lock(home)?;
    let publication_custody = checked_publication::Custody::new(&[&publication_file])?;
    // Tuple fields drop in order: pop inner custody before closing its file.
    // Helpers inherit this description through the existing custody transport.
    let mut publication = Some((publication_custody, publication_file));
    // Accepted Git objects, not the mutable home reader, supply the baseline.
    // Check reader cleanliness only before advancing it during finishing.
    let root = save_root(home)?;
    let key = draft_key(draft)?;
    let journal = root.join(format!("{key}.json"));
    let retained: Option<Pending> = if journal.try_exists()? {
        Some(serde_json::from_slice(&crate::read_regular(&journal)?)?)
    } else {
        None
    };
    if received {
        require(request.explicit.is_empty(), "received status selection cannot include manual intents")?;
        request.explicit = if let Some(saved) = retained.as_ref().filter(|saved| saved.commit == *commit) {
            saved.explicit.clone()
        } else {
            checked_publication::commit(home, base)?;
            checked_publication::commit(home, commit)?;
            // Reuse Core's immutable Git reader and parsed-object cache, not a
            // second Markdown parser or mutable checkout scan.
            let mut reader = crate::git_snapshot::GitReader::new(policy, home)?;
            let before = reader.view(base)?;
            let after = reader.view(commit)?;
            after.nodes.iter().filter_map(|(id, node)| {
                before.nodes.get(id).filter(|old| old.state.status != node.state.status)
                    .map(|_| id.clone())
            }).collect()
        };
    }
    let mut pending: Pending = if let Some(saved) = retained {
        saved
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
        pending.home == *home && pending.draft == *draft && pending.base == *base,
        "different home, draft or base is pending; retain the journal and retry its original arguments",
    )?;
    require(
        pending.branch.starts_with("refs/heads/sifttext/"),
        "unsafe pending branch",
    )?;
    if pending.commit != *commit {
        // A new explicit COMMIT is a repair request, not permission to adopt
        // ambient HEAD. Never replace custody of an accepted/unfinished save.
        require(pending.completed_head.is_none(),
            "completed save is pending; retry its original arguments before another save")?;
        crate::real_dir(home)?;
        crate::real_dir(draft)?;
        require(fs::canonicalize(home)? == *home && fs::canonicalize(draft)? == *draft
            && reader_git(draft, &["rev-parse", "--show-toplevel"])? == draft.to_str().ok_or("non-UTF-8 draft")?
            && reader_git(draft, &["rev-parse", "--path-format=absolute", "--git-common-dir"])?
                == home.join(".git").to_str().ok_or("non-UTF-8 home")?
            && reader_git(draft, &["symbolic-ref", "--quiet", "HEAD"])? == pending.branch,
            "repair must use the same canonical home, allocated draft and branch; retain original journal")?;
        let ancestor = |from: &str, to: &str| -> Result<bool> {
            let output = checked_publication::run_command(super::reader_command(home,
                &["merge-base", "--is-ancestor", from, to]))?;
            match output.status.code() {
                Some(0) => Ok(true),
                Some(1) => Ok(false),
                _ => Err("cannot establish repair ancestry; retain original journal".into()),
            }
        };
        let accepted = reader_git(home, &["rev-parse", "refs/heads/main"])?;
        checked_publication::commit(home, &accepted)?;
        checked_publication::commit(home, &pending.commit)?;
        checked_publication::commit(home, base)?;
        checked_publication::commit(home, commit)?;
        require(!ancestor(&pending.commit, &accepted)?,
            "original request is already accepted or baseline; finish its original retry, do not replace its journal")?;
        let tip = if let Some((from, to)) = &pending.prepared_heads {
            checked_publication::commit(home, from)?;
            checked_publication::commit(home, to)?;
            require(!ancestor(from, &accepted)? && !ancestor(to, &accepted)?
                && ancestor(&pending.commit, from)? && ancestor(from, to)?,
                "prepared request may already be accepted; retain journal and retry original arguments")?;
            to
        } else {
            &pending.commit
        };
        require(ancestor(base, &pending.commit)? && ancestor(base, &accepted)?
            && ancestor(tip, commit)? && reader_git(draft, &["rev-parse", "HEAD"])? == *commit,
            "repair COMMIT must be the clean draft HEAD descended from the recorded preparation; retain original journal")?;
        checked_publication::clean(draft, commit)?;
        pending.commit = commit.clone();
        pending.explicit = request.explicit.clone();
        pending.prepared_heads = None;
        persist(&journal, &pending)?;
    }
    require(pending.explicit == request.explicit,
        "different status intents are pending; retry original arguments or commit an explicit repair first")?;

    let missing =
        fs::symlink_metadata(draft).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound);
    require(!missing || pending.completed_head.is_some(),
        "missing uncompleted draft; retain journal")?;
    // Removal may have stopped after unlinking .git but before removing the
    // directory. Completed custody recovers acceptance without reading that
    // partially deleted draft as a new publication input.
    let recovering_cleanup = pending.completed_head.is_some();
    let receipt = if recovering_cleanup {
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
        let receipt = checked_publication::save_prepared_leased(
            policy,
            &request,
            &draft_lease,
            previous_heads
                .as_ref()
                .map(|(from, to)| (from.as_str(), to.as_str())),
            record,
        );
        if receipt.status != "current" {
            // A refused retry can follow earlier acceptance. This invocation's
            // published=false says nothing about that prior attempt.
            let accepted = receipt.accepted_before.as_deref().filter(|accepted| {
                checked_publication::commit(home, commit).is_ok()
                    && reader_git(home, &["merge-base", "--is-ancestor", commit, accepted]).is_ok()
            });
            let retry_ref_race = pending.prepared_heads.is_some()
                && matches!(receipt.check, "conditional_ref_race" | "source_or_ref_changed")
                && receipt.accepted_before.as_ref().is_some_and(|before| {
                    reader_git(home, &["rev-parse", "refs/heads/main"])
                        .is_ok_and(|current| current != *before)
                });
            if !receipt.published
                && pending.completed_head.is_none()
                && pending.prepared_heads.is_none()
                && accepted.is_none()
                && !retry_ref_race
            {
                // Only a preflight refusal without prepared custody can discard
                // the request. Normalization/combination records survive rejection
                // so exact original arguments recheck the same prepared HEAD.
                fs::remove_file(&journal)?;
                fs::File::open(&root)?.sync_all()?;
            }
            let mut output = serde_json::to_value(&receipt)?;
            output["home"] = json!(home);
            output["draft"] = json!(draft);
            if retry_ref_race {
                // Normalization may have authored a new prepared HEAD. Retain
                // its exact journal pair so the original request can resume it.
                output["next_action"] = json!(
                    "accepted main advanced concurrently; retain the draft and retry the original submit arguments to recombine and recheck"
                );
            }
            output["publication"] = json!(if receipt.published || accepted.is_some() {
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
            } else if pending.prepared_heads.is_some() && !retry_ref_race {
                output["next_action"] = json!(
                    "retain the prepared draft; retry original arguments to recheck it, or commit a repair descended from its recorded HEAD and submit that new explicit COMMIT with the same home/draft/base"
                );
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
    let finish = || -> Result<Value> {
        let advanced = {
            let home_lock = home_lock(home, true)?;
            let _home_custody = checked_publication::Custody::new(&[&home_lock])?;
            let before = reader_state(home)?;
            let advanced = advance_checked(policy, home, &before, target, Some(version), Some(draft), || Ok(()))?;
            // advance_checked already verifies HEAD, accepted ref and the
            // refreshed source version while this home custody is held.
            advanced
        };
        // Home custody has already popped. Release publication custody then its
        // file before recursive cleanup; the draft lease still protects removal.
        drop(publication.take());
        if !recovering_cleanup {
            pending.completed_head = Some(target.to_owned());
            // Persist acceptance custody before deleting anything. A retry
            // finishes this exact draft even if .git or the root is already gone.
            persist(&journal, &pending)?;
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
        remove_completed_draft(home, draft, &pending.branch, cleaned_head)?;
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
        if recovering_cleanup {
            let identity = fs::symlink_metadata(receipt.view.as_ref().ok_or("missing recovery view")?)?;
            checked_publication::remove_owned_view(home, &receipt, &identity)?;
        }
        // ponytail: retain completed journals; GC needs an explicit retry-retention contract.
        // Retain completed custody as an idempotency tombstone. Losing stdout after
        // cleanup must not make the original request unrecoverable; retries still
        // validate current accepted history rather than trust this record as proof.
        let mut output = serde_json::to_value(&receipt)?;
        if recovering_cleanup {
            output["published"] = json!(false);
        }
        output["view"] = json!(home);
        output["home"] = json!(home);
        output["home_advance"] = advanced;
        output["worktree_removed"] = json!(true);
        before_return()?;
        Ok(output)
    };
    match finish() {
        Ok(output) => Ok(output),
        Err(error) => {
            eprintln!("accepted save finishing incomplete: {error}");
            let mut output = serde_json::to_value(&receipt)?;
            output["status"] = json!("unavailable");
            output["check"] = json!("accepted_finish_incomplete");
            output["publication"] = json!("accepted_incomplete");
            output["accepted"] = json!(target);
            output["home"] = json!(home);
            output["draft"] = json!(draft);
            output["view"] = Value::Null;
            output["next_action"] = json!(format!(
                "accepted {target}, version {version}; home or cleanup may already have changed; preserve new work and retry the original submit arguments: {error}"
            ));
            Ok(output)
        }
    }
}

// The completed editing worktree is disposable, including ignored/untracked
// files. Only its exact recorded registration may be retired; never prune peers.
fn remove_completed_draft(home: &Path, draft: &Path, branch: &str, head: &str) -> Result<()> {
    let registered = reader_git(home, &["worktree", "list", "--porcelain", "-z"])?;
    let wanted = format!("worktree {}", draft.to_str().ok_or("non-UTF-8 draft")?);
    let mut entries = registered.split("\0\0")
        .filter(|entry| entry.split('\0').any(|row| row == wanted));
    let entry = entries.next();
    require(entries.next().is_none(), "ambiguous completed draft registration")?;
    if let Some(entry) = entry {
        require(entry.split('\0').any(|row| row == format!("branch {branch}"))
            && entry.split('\0').any(|row| row == format!("HEAD {head}")),
            "completed draft registration changed; preserve it")?;
    }
    match fs::symlink_metadata(draft) {
        Ok(_) => {
            require(entry.is_some(), "unregistered completed draft path reused; preserve it")?;
            crate::real_dir(draft)?;
            require(fs::canonicalize(draft)? == draft, "completed draft path changed")?;
            // No status, ignored-file inventory or artifact/cleanliness gate.
            // Stdlib recursive deletion also resumes a partially removed .git.
            fs::remove_dir_all(draft)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.into()),
    }
    if entry.is_some() {
        reader_git(home, &["worktree", "remove", "--force",
            draft.to_str().ok_or("non-UTF-8 draft")?])?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/successor/local_submission.rs"]
mod tests;
