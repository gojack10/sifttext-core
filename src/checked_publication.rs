//! Cooperative local Git publication. Git merges authored files; caches never merge.
//! The accepted branch must not be checked out. `save` checks the submitted draft;
//! the low-level `submit` retains its detached-view recovery/observer contract.
use crate::{Policy, Result, candidate_validation as gate, index, require};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub repo: PathBuf,
    pub accepted_ref: String,
    /// Registered, clean worktree with HEAD at the retained commit. None recovers accepted main.
    pub draft: Option<PathBuf>,
    pub commit: Option<String>,
    pub base: Option<String>,
    #[serde(default)]
    pub explicit: BTreeSet<String>,
    #[serde(default)]
    pub pause_before_publish: Option<Barrier>,
    #[serde(default)]
    pub pause_after_publish: Option<Barrier>,
}

/// Test-only scheduling barrier; never an alternative validation path.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Barrier {
    pub ready: PathBuf,
    pub release: PathBuf,
}
impl Barrier {
    fn wait(&self, forbidden: &[&Path], view: &Path) -> Result<()> {
        require(self.ready != self.release, "barrier paths must differ")?;
        for p in [&self.ready, &self.release] {
            let parent = fs::canonicalize(p.parent().ok_or("missing barrier parent")?)?;
            require(
                forbidden.iter().all(|d| !parent.starts_with(d)),
                "unsafe barrier path",
            )?;
        }
        let mut ready = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&self.ready)?;
        ready.write_all(view.as_os_str().as_encoded_bytes())?;
        let start = Instant::now();
        while !self.release.try_exists()? {
            require(start.elapsed() < Duration::from_secs(10), "barrier expired")?;
            thread::sleep(Duration::from_millis(5));
        }
        Ok(())
    }
}

#[derive(Debug, Serialize)]
pub struct Receipt {
    pub status: &'static str,
    pub check: &'static str,
    pub accepted_before: Option<String>,
    pub candidate: Option<String>,
    pub tree: Option<String>,
    pub draft_commit: Option<String>,
    pub draft_base: Option<String>,
    pub draft_branch: Option<String>,
    pub published: bool,
    pub version: Option<String>,
    /// Only present for a verified, matching view. Never return an orphan as current.
    pub view: Option<PathBuf>,
    pub validation: Option<gate::Report>,
    /// Verified record computation reuse; fresh detached caches still write all rows.
    pub index_reuse: Option<index::Reuse>,
    pub accepted_index_available: bool,
    pub draft_index_available: bool,
    /// Both computed and final exact-candidate graph checks, not proof tokens.
    pub graph_checks: Vec<gate::GraphCheck>,
    pub elapsed_ms: u128,
}
impl Receipt {
    pub fn lean(&self) -> Result<String> {
        crate::lean_render::render(&serde_json::json!({
            "status":self.status,"check":self.check,"published":self.published,
            "accepted_before":self.accepted_before,"candidate":self.candidate,
            "tree":self.tree,"draft_commit":self.draft_commit,"draft_base":self.draft_base,
            "draft_branch":self.draft_branch,"version":self.version,
            "graph_mode":self.graph_checks.last().map(|g| g.mode),
            "graph_reason":self.graph_checks.last().map(|g| g.reason),
            "rule":gate::CANDIDATE_RULE,"rows":[],
        }))
        .map_err(|e| std::io::Error::other(e).into())
    }
}

// Disable inherited Git routing/config and execution hooks. Repository-local
// attributes are refused below rather than trusting arbitrary merge/filter drivers.
fn command(repo: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new("git");
    for (key, _) in std::env::vars_os() {
        if key.as_encoded_bytes().starts_with(b"GIT_") {
            cmd.env_remove(key);
        }
    }
    cmd.env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_NO_REPLACE_OBJECTS", "1")
        .env("GIT_ATTR_NOSYSTEM", "1")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_AUTHOR_NAME", "SiftText Core")
        .env("GIT_AUTHOR_EMAIL", "core@example.invalid")
        .env("GIT_COMMITTER_NAME", "SiftText Core")
        .env("GIT_COMMITTER_EMAIL", "core@example.invalid")
        .args(["-C"])
        .arg(repo)
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.attributesFile=/dev/null",
            "-c",
            "commit.gpgSign=false",
            "-c",
            "merge.autoStash=false",
            "-c",
            "rerere.enabled=false",
        ])
        .args(args);
    cmd
}
// Bounded helper-inclusive execution: drain both pipes, stop oversized or hung Git.
fn run(repo: &Path, args: &[&str]) -> Result<Output> {
    run_command(command(repo, args))
}
fn run_command(mut cmd: Command) -> Result<Output> {
    const CAP: usize = 16 * 1024 * 1024;
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let pipes: [Box<dyn Read + Send>; 2] = [
        Box::new(child.stdout.take().ok_or("missing stdout")?),
        Box::new(child.stderr.take().ok_or("missing stderr")?),
    ];
    let (tx, rx) = mpsc::channel();
    for (tag, pipe) in pipes.into_iter().enumerate() {
        let tx = tx.clone();
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = pipe
                .take((CAP + 1) as u64)
                .read_to_end(&mut bytes)
                .map(|_| bytes);
            let _ = tx.send((tag, result));
        });
    }
    drop(tx);
    let start = Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if start.elapsed() >= Duration::from_secs(10) {
            child.kill()?;
            child.wait()?;
            return Err("Git helper timeout".into());
        }
        thread::sleep(Duration::from_millis(2));
    };
    let mut bytes = [Vec::new(), Vec::new()];
    for _ in 0..2 {
        let (tag, result) = rx.recv_timeout(Duration::from_secs(1))?;
        bytes[tag] = result?;
    }
    require(
        bytes.iter().map(Vec::len).sum::<usize>() <= CAP,
        "Git output resource limit",
    )?;
    let [stdout, stderr] = bytes;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}
fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let out = run(repo, args)?;
    require(out.status.success(), "Git operation rejected")?;
    Ok(std::str::from_utf8(&out.stdout)?
        .trim_end_matches('\n')
        .to_owned())
}
fn oid(s: &str) -> bool {
    [40, 64].contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
fn commit(repo: &Path, s: &str) -> Result<String> {
    require(oid(s), "commit must be a full object ID")?;
    let got = git(repo, &["rev-parse", "--verify", &format!("{s}^{{commit}}")])?;
    require(got == s, "commit identity changed")?;
    Ok(got)
}
#[cfg(test)]
#[test]
fn clean_refuses_hidden_working_edits() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path();
    git(repo, &["init", "-q", "-b", "main"]).unwrap();
    fs::write(repo.join("node.md"), "committed content").unwrap();
    git(repo, &["add", "node.md"]).unwrap();
    git(repo, &["commit", "-qm", "knowledge: hidden-index control"]).unwrap();
    let head = git(repo, &["rev-parse", "HEAD"]).unwrap();
    let tree = git(repo, &["write-tree"]).unwrap();
    clean(repo, &head).unwrap();
    for (flag, clear) in [
        ("--assume-unchanged", "--no-assume-unchanged"),
        ("--skip-worktree", "--no-skip-worktree"),
    ] {
        git(repo, &["update-index", flag, "node.md"]).unwrap();
        fs::write(repo.join("node.md"), "hidden uncommitted content").unwrap();
        assert_eq!(git(repo, &["status", "--porcelain"]).unwrap(), "");
        assert_eq!(git(repo, &["write-tree"]).unwrap(), tree);
        assert!(clean(repo, &head).is_err(), "hidden edit accepted: {flag}");
        assert_eq!(
            fs::read_to_string(repo.join("node.md")).unwrap(),
            "hidden uncommitted content"
        );
        assert_eq!(git(repo, &["rev-parse", "HEAD"]).unwrap(), head);
        git(repo, &["update-index", clear, "node.md"]).unwrap();
        assert!(clean(repo, &head).is_err());
        fs::write(repo.join("node.md"), "committed content").unwrap();
        clean(repo, &head).unwrap();
    }
    git(repo, &["update-index", "--skip-worktree", "node.md"]).unwrap();
    // Even unchanged present files are not a safe sparse omission.
    assert!(clean_draft(repo, &head, true).is_err());
    fs::remove_file(repo.join("node.md")).unwrap();
    clean_draft(repo, &head, true).unwrap();
    assert!(clean(repo, &head).is_err()); // Destructive save still refuses.
    std::os::unix::fs::symlink("missing-target", repo.join("node.md")).unwrap();
    assert!(clean_draft(repo, &head, true).is_err());
    fs::remove_file(repo.join("node.md")).unwrap();
    git(repo, &["update-index", "--assume-unchanged", "node.md"]).unwrap();
    assert!(clean_draft(repo, &head, true).is_err());
}

pub(crate) fn clean(draft: &Path, expected: &str) -> Result<()> {
    clean_draft(draft, expected, false)
}

// Detached submission reads the commit and never resets/removes the sparse draft.
// Only absent skip-worktree paths may be hydrated from that commit; present hidden
// files must still refuse, since Git status/write-tree do not inspect their bytes.
fn clean_draft(draft: &Path, expected: &str, sparse: bool) -> Result<()> {
    require(
        git(draft, &["rev-parse", "HEAD"])? == expected,
        "draft HEAD changed",
    )?;
    let files = run(draft, &["ls-files", "-v", "-z"])?;
    require(files.status.success(), "draft index unavailable")?;
    let mut omissions = BTreeSet::new();
    for entry in files.stdout.split(|b| *b == 0).filter(|b| !b.is_empty()) {
        let absent = sparse
            && entry.starts_with(b"S ")
            && fs::symlink_metadata(draft.join(std::str::from_utf8(&entry[2..])?))
                .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound);
        if absent {
            omissions.insert(entry[2..].to_vec());
        }
        require(
            entry.starts_with(b"H ") || absent,
            "draft index has hidden/sparse edits; preserve files before submitting",
        )?;
    }
    tracked_matches_commit(draft, expected, &omissions)?;
    let status = run(
        draft,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
    )?;
    require(
        status.status.success() && status.stdout.is_empty(),
        "dirty draft",
    )?;
    let ignored = run(
        draft,
        &[
            "ls-files",
            "--others",
            "--ignored",
            "--exclude-standard",
            "-z",
        ],
    )?;
    require(
        ignored.status.success(),
        "ignored file inventory unavailable",
    )?;
    for file in ignored.stdout.split(|b| *b == 0).filter(|b| !b.is_empty()) {
        let path = Path::new(std::str::from_utf8(file)?);
        use std::os::unix::fs::MetadataExt;
        let metadata = fs::symlink_metadata(draft.join(path))?;
        require(
            metadata.is_file() && metadata.nlink() == 1,
            "ignored entry is not a disposable regular file; preserve it before submitting",
        )?;
        require(
            crate::token_metadata::derived(path)
                || disposable_outline(path)
                || [
                    ".sifttext/index.sqlite",
                    ".sifttext/index.sqlite-journal",
                    ".sifttext/index.sqlite-wal",
                    ".sifttext/index.sqlite-shm",
                ]
                .iter()
                .any(|cache| path == Path::new(cache)),
            "ignored user file; preserve it before submitting",
        )?;
    }
    for name in [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "rebase-merge",
        "rebase-apply",
    ] {
        let path = git(
            draft,
            &["rev-parse", "--path-format=absolute", "--git-path", name],
        )?;
        require(!Path::new(&path).try_exists()?, "unfinished Git operation")?;
    }
    Ok(())
}
#[cfg(test)]
#[test]
fn ignored_outline_inventory_protects_unknown_files() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path();
    git(repo, &["init", "-q", "-b", "main"]).unwrap();
    fs::write(repo.join(".gitignore"), "/.sifttext/\n").unwrap();
    git(repo, &["add", ".gitignore"]).unwrap();
    git(
        repo,
        &["commit", "-qm", "knowledge: output custody control"],
    )
    .unwrap();
    let head = git(repo, &["rev-parse", "HEAD"]).unwrap();
    let output = repo.join(".sifttext/outlines/00000000-0000-0000-0000-000000000001");
    fs::create_dir_all(&output).unwrap();
    for name in [
        "outline.lean",
        "outline.full.lean",
        "outline.lean.meta.json",
        "outline.full.lean.meta.json",
    ] {
        fs::write(output.join(name), "reserved output").unwrap();
    }
    let stage = repo.join(".sifttext/outline-stage");
    fs::create_dir(&stage).unwrap();
    let _staged = tempfile::NamedTempFile::new_in(&stage).unwrap();
    clean(repo, &head).unwrap();
    for directory in [&output, &stage] {
        let note = directory.join("private-note.txt");
        fs::write(&note, "uncommitted user data").unwrap();
        assert!(clean(repo, &head).is_err());
        assert_eq!(fs::read_to_string(&note).unwrap(), "uncommitted user data");
        assert_eq!(git(repo, &["rev-parse", "HEAD"]).unwrap(), head);
        fs::remove_file(note).unwrap();
        clean(repo, &head).unwrap();
    }
    // Git collapses embedded repositories to one ignored directory entry.
    // A reserved output name does not make their contents disposable.
    let embedded = stage.join(".tmpABC123");
    fs::create_dir(&embedded).unwrap();
    git(&embedded, &["init", "-q", "-b", "main"]).unwrap();
    let note = embedded.join("private-note.txt");
    fs::write(&note, "uncommitted nested repository data").unwrap();
    assert!(clean(repo, &head).is_err());
    assert_eq!(
        fs::read_to_string(&note).unwrap(),
        "uncommitted nested repository data"
    );
    assert_eq!(git(repo, &["rev-parse", "HEAD"]).unwrap(), head);
    fs::remove_dir_all(embedded).unwrap();
    let link = stage.join(".tmpABC124");
    std::os::unix::fs::symlink("missing-target", &link).unwrap();
    assert!(clean(repo, &head).is_err());
    assert_eq!(fs::read_link(&link).unwrap(), Path::new("missing-target"));
    fs::remove_file(&link).unwrap();
    fs::hard_link(_staged.path(), &link).unwrap();
    assert!(clean(repo, &head).is_err());
    fs::remove_file(link).unwrap();
    clean(repo, &head).unwrap();
    // Ignored reserved output can still carry a staged, uncommitted index entry.
    let indexed = output.join("outline.lean");
    let relative = indexed.strip_prefix(repo).unwrap().to_str().unwrap();
    git(repo, &["add", "-f", "--", relative]).unwrap();
    assert!(
        !git(repo, &["diff", "--cached", "--name-only"])
            .unwrap()
            .is_empty()
    );
    assert!(clean(repo, &head).is_err());
    fs::remove_file(&indexed).unwrap(); // The staged bytes now exist only in Git.
    assert!(clean(repo, &head).is_err());
    assert_eq!(
        git(repo, &["show", &format!(":{relative}")]).unwrap(),
        "reserved output"
    );
    assert_eq!(git(repo, &["rev-parse", "HEAD"]).unwrap(), head);
    git(repo, &["reset", "-q", "HEAD", "--", relative]).unwrap();
    fs::write(indexed, "reserved output").unwrap();
    clean(repo, &head).unwrap();
}

/// Compare working bytes with the commit, not the caller index's cached stat data.
/// A fresh private index has no trusted stat entries; neither refresh nor diff
/// modifies the real index. Detached submission permits only verified omissions.
pub(crate) fn tracked_matches_commit(
    repo: &Path,
    expected: &str,
    omissions: &BTreeSet<Vec<u8>>,
) -> Result<()> {
    let temp = tempfile::tempdir()?;
    let index = temp.path().join("index");
    let checked = |args: &[&str]| {
        let mut cmd = command(
            repo,
            &[
                "-c",
                "core.sparseCheckout=false",
                "-c",
                "core.autocrlf=false",
                "-c",
                "core.ignoreStat=false",
                "-c",
                "core.trustctime=true",
                "-c",
                "core.checkStat=default",
                "-c",
                "core.filemode=true",
                "-c",
                "core.symlinks=true",
            ],
        );
        cmd.env("GIT_INDEX_FILE", &index).args(args);
        run_command(cmd)
    };
    require(
        checked(&["read-tree", expected])?.status.success(),
        "commit index unavailable",
    )?;
    // Git clean filters/EOL attributes can hide raw-byte differences or execute
    // repository commands. Core authored trees cannot contain .gitattributes.
    let attributes = PathBuf::from(git(
        repo,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "info/attributes",
        ],
    )?);
    let metadata = fs::symlink_metadata(&attributes);
    require(
        metadata
            .as_ref()
            .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
            || metadata.is_ok_and(|m| m.is_file() && m.len() == 0),
        "unsupported repository attributes; preserve files before submitting",
    )?;
    let files = checked(&["ls-files", "-z"])?;
    require(files.status.success(), "commit file inventory unavailable")?;
    let mut parents = BTreeSet::new();
    for file in files.stdout.split(|b| *b == 0).filter(|p| !p.is_empty()) {
        let path = Path::new(std::str::from_utf8(file)?);
        for parent in path.parent().ok_or("missing tracked parent")?.ancestors() {
            parents.insert(parent.to_path_buf());
        }
    }
    for parent in parents {
        require(
            fs::symlink_metadata(repo.join(parent).join(".gitattributes"))
                .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
            "unsupported working attributes; preserve files before submitting",
        )?;
    }
    // Refresh hashes every stat-invalid entry; code 1 means changed/missing files,
    // which the following NUL-delimited inventory checks (including sparse omissions).
    let refresh = checked(&["update-index", "--really-refresh"])?;
    require(
        matches!(refresh.status.code(), Some(0 | 1)),
        "commit index refresh unavailable",
    )?;
    let diff = checked(&[
        "diff-files",
        "--no-ext-diff",
        "--name-only",
        "-z",
        "--ignore-submodules=none",
    ])?;
    require(
        diff.status.success()
            && diff
                .stdout
                .split(|b| *b == 0)
                .filter(|p| !p.is_empty())
                .all(|p| omissions.contains(p)),
        "working files differ from committed knowledge; preserve them before submitting",
    )
}

// Namespace membership alone does not make an ignored user file disposable.
fn disposable_outline(path: &Path) -> bool {
    let parts: Vec<_> = path.iter().filter_map(|s| s.to_str()).collect();
    match parts.as_slice() {
        ["nodes", id, name] | [".sifttext", "outlines", id, name] => {
            crate::uuid(id)
                && [
                    "outline.lean",
                    "outline.full.lean",
                    "outline.lean.meta.json",
                    "outline.full.lean.meta.json",
                ]
                .contains(name)
        }
        // NamedTempFile's pinned default: .tmp followed by six random characters.
        [".sifttext", "outline-stage", name] => {
            name.len() == 10
                && name.starts_with(".tmp")
                && name[4..].bytes().all(|b| b.is_ascii_alphanumeric())
        }
        _ => false,
    }
}
type FileIdentity = (PathBuf, u64, u64, i64, i64);
fn tracked_identity(view: &Path) -> Result<Vec<FileIdentity>> {
    use std::os::unix::fs::MetadataExt;
    let files = run(view, &["ls-files", "-z"])?;
    require(files.status.success(), "missing view index")?;
    files
        .stdout
        .split(|b| *b == 0)
        .filter(|b| !b.is_empty())
        .map(|name| {
            let path = PathBuf::from(std::str::from_utf8(name)?);
            let m = fs::symlink_metadata(view.join(&path))?;
            Ok((path, m.dev(), m.ino(), m.ctime(), m.ctime_nsec()))
        })
        .collect()
}
fn accepted(repo: &Path, reference: &str) -> Result<String> {
    require(
        reference.starts_with("refs/heads/"),
        "accepted ref must be a full branch ref",
    )?;
    git(repo, &["check-ref-format", reference])?;
    require(
        !run(repo, &["symbolic-ref", "-q", reference])?
            .status
            .success(),
        "symbolic accepted ref",
    )?;
    let tip = git(repo, &["rev-parse", "--verify", reference])?;
    commit(repo, &tip)?;
    // Moving a checked-out branch would silently invalidate a user's index/draft.
    let worktrees = git(repo, &["worktree", "list", "--porcelain"])?;
    require(
        !worktrees
            .lines()
            .any(|l| l == format!("branch {reference}")),
        "accepted ref is checked out; detach its reader first",
    )?;
    Ok(tip)
}
fn authored_tree(repo: &Path, revision: &str) -> Result<()> {
    let tree = run(repo, &["ls-tree", "-rz", revision])?;
    require(tree.status.success(), "unknown tree")?;
    for record in tree.stdout.split(|b| *b == 0).filter(|b| !b.is_empty()) {
        let record = std::str::from_utf8(record)?;
        let (meta, path) = record.split_once('\t').ok_or("invalid Git tree entry")?;
        let parts: Vec<_> = path.split('/').collect();
        let regular = meta.starts_with("100644 blob ");
        let link = meta.starts_with("120000 blob ");
        let allowed = match parts.as_slice() {
            [".gitignore"] => regular,
            ["trees", ".keep"] => regular,
            ["trees", id] => link && crate::uuid(id),
            ["nodes", id, "node.md" | "children.order"] => regular && crate::uuid(id),
            ["nodes", id, "children", ".keep"] => regular && crate::uuid(id),
            ["nodes", id, "children", child] => link && crate::uuid(id) && crate::uuid(child),
            _ => false,
        };
        require(allowed, "unsafe or tracked non-authored Git entry")?;
    }
    Ok(())
}
struct WorkingView {
    repo: PathBuf,
    dir: tempfile::TempDir,
    path: PathBuf,
    retained: bool,
}
impl WorkingView {
    fn new(repo: &Path, root: &Path, revision: &str) -> Result<Self> {
        let dir = tempfile::Builder::new()
            .prefix("submission-")
            .tempdir_in(root)?;
        let path = dir.path().join("view");
        git(
            repo,
            &[
                "worktree",
                "add",
                "--quiet",
                "--detach",
                path.to_str().ok_or("non-UTF-8 path")?,
                revision,
            ],
        )?;
        Ok(Self {
            repo: repo.into(),
            dir,
            path,
            retained: false,
        })
    }
    fn retain(mut self) -> PathBuf {
        self.retained = true;
        let path = self.path.clone();
        // Drop uses the retained bit; keep directory without moving out of Drop.
        self.dir.disable_cleanup(true);
        path
    }
}
impl Drop for WorkingView {
    fn drop(&mut self) {
        if !self.retained {
            let _ = run(
                &self.repo,
                &[
                    "worktree",
                    "remove",
                    "--force",
                    self.path.to_str().unwrap_or(""),
                ],
            );
        }
    }
}

fn accepted_index(
    policy: &Policy,
    repo: &Path,
    root: &Path,
    main: &str,
) -> Result<Option<index::PreparedIndex>> {
    let mut donors = vec![repo.to_path_buf()];
    // ponytail: scan retained views; an explicit retirement/index policy is
    // needed if retained publication history makes this acquisition expensive.
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            donors.push(entry.path().join("view"));
        }
    }
    donors.sort();
    for path in donors {
        let verify = || -> Result<Option<index::PreparedIndex>> {
            crate::real_dir(&path)?;
            if !path.join(".sifttext/index.sqlite").try_exists()? {
                return Ok(None);
            }
            clean(&path, main)?;
            let source = tracked_identity(&path)?;
            let prepared = index::prepare_reuse(policy, &path, None)?;
            clean(&path, main)?;
            require(tracked_identity(&path)? == source, "donor source changed")?;
            Ok(prepared)
        };
        if let Ok(Some(prepared)) = verify() {
            return Ok(Some(prepared));
        }
    }
    Ok(None)
}

/// Submit once, or recover the currently accepted ref. No caller pull or checkout
/// modification. Errors fail closed; a post-CAS error is explicitly unavailable.
pub fn submit(policy: &Policy, request: &Request) -> Receipt {
    submit_inner(policy, request, false, None, &mut |_, _| Ok(()))
}

/// Check and accept the combination in the submitted worktree. The caller owns
/// durable home advancement and cleanup; refusal never discards the draft.
pub fn save(policy: &Policy, request: &Request) -> Receipt {
    save_prepared(policy, request, None, |_, _| Ok(()))
}

/// The caller's durable journal records exact HEAD transitions, not acceptance.
/// Retry still validates the complete candidate and conditionally updates main.
pub(crate) fn save_prepared(
    policy: &Policy,
    request: &Request,
    previous_heads: Option<(&str, &str)>,
    mut record: impl FnMut(&str, &str) -> Result<()>,
) -> Receipt {
    submit_inner(policy, request, true, previous_heads, &mut record)
}

fn submit_inner(
    policy: &Policy,
    request: &Request,
    in_draft: bool,
    previous_heads: Option<(&str, &str)>,
    record: &mut dyn FnMut(&str, &str) -> Result<()>,
) -> Receipt {
    let start = Instant::now();
    let mut receipt = Receipt {
        status: "rejected",
        check: "unsafe_submission",
        accepted_before: None,
        candidate: None,
        tree: None,
        draft_commit: request.commit.clone(),
        draft_base: request.base.clone(),
        draft_branch: None,
        published: false,
        version: None,
        view: None,
        validation: None,
        index_reuse: None,
        accepted_index_available: false,
        draft_index_available: false,
        graph_checks: vec![],
        elapsed_ms: 0,
    };
    if execute(
        policy,
        request,
        &mut receipt,
        in_draft,
        previous_heads,
        record,
    )
    .is_err()
        && receipt.published
    {
        receipt.status = "unavailable";
        receipt.check = "published_view_requires_recovery";
        receipt.view = None;
    }
    receipt.elapsed_ms = start.elapsed().as_millis();
    receipt
}
fn execute(
    policy: &Policy,
    q: &Request,
    r: &mut Receipt,
    in_draft: bool,
    previous_heads: Option<(&str, &str)>,
    record: &mut dyn FnMut(&str, &str) -> Result<()>,
) -> Result<()> {
    require(!in_draft || q.draft.is_some(), "save requires a draft")?;
    crate::real_dir(&q.repo)?;
    let repo = fs::canonicalize(&q.repo)?;
    let common = PathBuf::from(git(
        &repo,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?);
    crate::real_dir(&common)?;
    for name in ["info/attributes", "objects/info/alternates", "shallow"] {
        let p = common.join(name);
        require(
            !p.try_exists()? || (fs::symlink_metadata(&p)?.is_file() && fs::read(&p)?.is_empty()),
            "unsupported repository indirection",
        )?;
    }
    let main = accepted(&repo, &q.accepted_ref)?;
    authored_tree(&repo, &main)?;
    r.accepted_before = Some(main.clone());
    let draft = if let Some(path) = &q.draft {
        crate::real_dir(path)?;
        let path = fs::canonicalize(path)?;
        require(
            git(
                &path,
                &["rev-parse", "--path-format=absolute", "--show-toplevel"],
            )? == path.to_str().ok_or("non-UTF-8 worktree path")?,
            "draft must be a worktree root",
        )?;
        let own = PathBuf::from(git(
            &path,
            &["rev-parse", "--path-format=absolute", "--git-common-dir"],
        )?);
        require(
            fs::canonicalize(own)? == common,
            "cross-repository submission",
        )?;
        let registered = run(&repo, &["worktree", "list", "--porcelain", "-z"])?;
        let entry = format!("worktree {}", path.to_str().ok_or("non-UTF-8 path")?);
        require(
            registered.status.success()
                && registered
                    .stdout
                    .split(|b| *b == 0)
                    .any(|field| field == entry.as_bytes()),
            "unregistered draft worktree",
        )?;
        let sha = commit(&repo, q.commit.as_deref().ok_or("missing commit")?)?;
        let base = commit(&repo, q.base.as_deref().ok_or("missing base")?)?;
        require(
            run(&repo, &["merge-base", "--is-ancestor", &base, &sha])?
                .status
                .success()
                && run(&repo, &["merge-base", "--is-ancestor", &base, &main])?
                    .status
                    .success(),
            "unrelated base",
        )?;
        let head = git(&path, &["rev-parse", "HEAD"])?;
        if in_draft && head != sha {
            // Resume only an accepted descendant or the caller's exact journaled
            // preparation. An arbitrary user-created descendant still refuses.
            require(
                run(&repo, &["merge-base", "--is-ancestor", &sha, &head])?
                    .status
                    .success()
                    && (previous_heads.is_some_and(|(from, to)| head == from || head == to)
                        || run(&repo, &["merge-base", "--is-ancestor", &head, &main])?
                            .status
                            .success()),
                "unaccepted draft HEAD changed; repair and resubmit explicitly",
            )?;
        }
        clean_draft(&path, if in_draft { &head } else { &sha }, !in_draft)?;
        let branch = git(&path, &["symbolic-ref", "--quiet", "HEAD"])?;
        require(branch != q.accepted_ref, "draft is accepted branch")?;
        r.draft_branch = Some(branch);
        authored_tree(&repo, &sha)?;
        Some(path)
    } else {
        require(
            q.commit.is_none() && q.base.is_none() && q.explicit.is_empty(),
            "invalid recovery request",
        )?;
        None
    };
    let root = common.join("sifttext-publications");
    if !root.try_exists()? {
        fs::create_dir(&root)?;
    }
    crate::real_dir(&root)?;
    // Only the detached repository reader and Core's retained publication views
    // are donors, never unrelated registered drafts. Git and the index verifier
    // both check them; missing/corrupt data is just an optimization miss.
    let main_index = accepted_index(policy, &repo, &root, &main)?;
    let before = WorkingView::new(&repo, &root, &main)?;
    let draft_index = if let Some(path) = &draft {
        if path.join(".sifttext/index.sqlite").try_exists()? {
            // Sparse preparation must use the submitted draft's pinned base,
            // not the newer accepted main (sparse omissions are not deletions).
            let base = WorkingView::new(&repo, &root, q.base.as_deref().ok_or("missing base")?)?;
            index::prepare_reuse(policy, path, Some(&base.path))?
        } else {
            None
        }
    } else {
        None
    };
    r.accepted_index_available = main_index.is_some();
    r.draft_index_available = draft_index.is_some();
    let candidate = if in_draft {
        None
    } else {
        Some(WorkingView::new(&repo, &root, &main)?)
    };
    let view = if in_draft {
        draft.as_ref().ok_or("missing draft")?
    } else {
        &candidate.as_ref().ok_or("missing candidate")?.path
    };
    let resumed = if in_draft {
        Some(git(view, &["rev-parse", "HEAD"])?)
    } else {
        None
    };
    let submitted = resumed.as_deref().or(q.commit.as_deref()).unwrap_or(&main);
    let already = run(&repo, &["merge-base", "--is-ancestor", submitted, &main])?
        .status
        .success();
    let sha = if in_draft {
        if already {
            main.clone()
        } else if run(&repo, &["merge-base", "--is-ancestor", &main, submitted])?
            .status
            .success()
        {
            submitted.to_owned()
        } else {
            r.check = "git_file_conflict";
            require(
                run(
                    view,
                    &[
                        "merge",
                        "--no-commit",
                        "--no-ff",
                        "--no-edit",
                        "--no-overwrite-ignore",
                        &main,
                    ],
                )?
                .status
                .success(),
                "Git conflict",
            )?;
            let tree = git(view, &["write-tree"])?;
            let metadata = git(
                view,
                &[
                    "log",
                    "-1",
                    "--format=%an%x00%ae%x00%(trailers:key=Co-authored-by)",
                    submitted,
                ],
            )?;
            let mut cmd = command(view, &["commit-tree", &tree, "-p", submitted, "-p", &main]);
            crate::checked_publication_attribution::integration_commit(&mut cmd, &metadata)?;
            let out = run_command(cmd)?;
            require(out.status.success(), "integration commit rejected")?;
            let sha = std::str::from_utf8(&out.stdout)?.trim().to_owned();
            commit(&repo, &sha)?;
            sha
        }
    } else if already {
        main.clone()
    } else {
        r.check = "git_file_conflict";
        require(
            run(
                view,
                &["merge", "--no-commit", "--no-ff", "--no-edit", submitted],
            )?
            .status
            .success(),
            "Git conflict",
        )?;
        let tree = git(view, &["write-tree"])?;
        git(
            view,
            &[
                "commit-tree",
                &tree,
                "-p",
                &main,
                "-p",
                submitted,
                "-m",
                "checked-publication: combine retained draft",
            ],
        )?
    };
    r.candidate = Some(sha.clone());
    r.tree = Some(git(&repo, &["rev-parse", &format!("{sha}^{{tree}}")])?);
    // The merge succeeded and the draft was clean at entry. Reset completes only
    // that exact merge (never a conflicted one); fast-forward keeps its commit.
    if in_draft {
        // Persist both sides before moving HEAD: a crash before or after the Git
        // operation must not orphan the preceding recorded preparation.
        record(submitted, &sha)?;
    }
    if in_draft && already {
        git(
            view,
            &[
                "merge",
                "--ff-only",
                "--no-edit",
                "--no-overwrite-ignore",
                &sha,
            ],
        )?;
    } else if !in_draft || git(view, &["rev-parse", "HEAD"])? != sha {
        git(view, &["reset", "--hard", &sha])?;
    }
    authored_tree(&repo, &sha)?;
    // Git omits empty directories. Hydrate the candidate after combination
    // (which may remove the last node) and the isolated accepted baseline.
    for owned in [&before.path, view] {
        // The root cache directory is part of the conservative Store inventory
        // (its generated contents are excluded). Hydrate it symmetrically before
        // proving the base so refresh cannot itself change the graph edit class.
        for name in ["nodes", "trees", ".sifttext"] {
            let path = owned.join(name);
            if !path.try_exists()? {
                fs::create_dir(&path)?;
            }
            crate::real_dir(&path)?;
        }
    }
    // A branch name or prior receipt file is not a proof. Bootstrap from the
    // exact accepted detached view in this process, with complete authored
    // candidate checks before obtaining the opaque graph capability.
    let (base_check, base_capture) = gate::validate_with_capture(
        policy,
        &gate::Request {
            store: before.path.clone(),
            before: None,
            explicit: BTreeSet::new(),
            expected_version: None,
            pause_before_accept: None,
        },
        None,
    );
    let proven = if draft.is_some() && base_check.status == "current" {
        r.check = "graph_transition_unavailable";
        let transition = crate::graph_transition::Transition::checked(policy)?;
        let base = transition.prove_validated(base_capture.ok_or("missing validated base")?)?;
        require(
            Some(base.version()) == base_check.version.as_deref(),
            "base changed after validation",
        )?;
        Some((transition, base))
    } else {
        None
    };
    r.check = "candidate_validation_rejected";
    let validation = gate::Request {
        store: view.clone(),
        before: Some(before.path.clone()),
        explicit: q.explicit.clone(),
        expected_version: None,
        pause_before_accept: None,
    };
    let proof = proven.as_ref().map(|(transition, base)| (transition, base));
    let (checked, final_capture) = gate::validate_with_capture(policy, &validation, proof);
    r.graph_checks.push(checked.graph.clone());
    r.version = checked.version.clone();
    let valid = checked.status == "current";
    r.validation = Some(checked);
    require(valid, "combined candidate rejected")?;
    let final_capture = final_capture.ok_or("missing validated candidate")?;
    let source_identity = tracked_identity(view)?;
    let before_identity = tracked_identity(&before.path)?;
    r.check = "derived_view_unavailable";
    // Full checked refresh remains independent of graph preservation. Only
    // verified assembled records/search bodies cross views, not acceptance proofs;
    // Git never merges SQLite or generated files.
    let (indexed, reuse) = index::refresh_for_publication(
        policy,
        view,
        None,
        main_index.as_ref(),
        draft_index.as_ref(),
        None,
    )?;
    r.index_reuse = Some(reuse);
    require(
        indexed.status == "current" && indexed.version == r.version,
        "derived candidate mismatch",
    )?;
    if let Some(barrier) = &q.pause_before_publish {
        barrier.wait(&[&repo, &common], view)?;
    }
    r.check = "source_or_ref_changed";
    if let Some(draft) = &draft {
        clean_draft(draft, if in_draft { &sha } else { submitted }, !in_draft)?;
        require(
            git(draft, &["symbolic-ref", "--quiet", "HEAD"])?
                == *r.draft_branch.as_ref().ok_or("missing draft identity")?,
            "draft branch changed",
        )?;
    }
    require(
        accepted(&repo, &q.accepted_ref)? == main,
        "accepted ref changed",
    )?;
    require(
        tracked_identity(view)? == source_identity
            && tracked_identity(&before.path)? == before_identity,
        "captured source identity changed",
    )?;
    clean(&before.path, &main)?;
    clean(view, &sha)?;
    require(
        git(view, &["write-tree"])? == *r.tree.as_ref().ok_or("missing tree")?,
        "candidate tree changed",
    )?;
    // Same invocation, request and immutable inputs: reuse every successful
    // candidate check, never a caller-supplied receipt or logical version alone.
    // Refresh may change derived files only. Keep complete before/candidate byte
    // and physical identity barriers and independently pin the transition helper.
    r.check = "candidate_recheck_rejected";
    if let Some((transition, _)) = proof {
        transition.verify()?;
    }
    final_capture.checked(policy)?;
    if let Some((transition, _)) = proof {
        transition.verify()?;
    }
    let inspection = crate::outlines::verify_validated(policy, view, &final_capture)?;
    require(
        inspection.status == "current" && inspection.version == r.version,
        "derived candidate changed",
    )?;
    // Git's conditional update is the serialization point; never retry a lost race.
    r.check = "conditional_ref_race";
    if !in_draft || sha != main {
        git(
            &repo,
            &["update-ref", "--no-deref", &q.accepted_ref, &sha, &main],
        )?;
    }
    r.published = !in_draft || sha != main;
    r.status = "unavailable";
    r.check = "published_view_requires_recovery";
    if let Some(barrier) = &q.pause_after_publish {
        barrier.wait(&[&repo, &common], view)?;
    }
    require(
        accepted(&repo, &q.accepted_ref)? == sha,
        "accepted advanced again",
    )?;
    clean(view, &sha)?;
    let inspection = crate::outlines::verify_validated(policy, view, &final_capture)?;
    require(
        inspection.status == "current" && inspection.version == r.version,
        "post-publication view changed",
    )?;
    r.view = Some(if in_draft {
        view.clone()
    } else {
        candidate.ok_or("missing candidate")?.retain()
    });
    r.status = "current";
    r.check = if already {
        "accepted_revision_recovered"
    } else {
        "published_checked_candidate"
    };
    Ok(())
}
