//! Cooperative local Git publication. Git merges authored files; caches never merge.
//! The accepted branch must not be checked out. `save` checks the submitted draft;
//! the low-level `submit` retains its detached-view recovery/observer contract.
mod lifetime;
mod owned;
use crate::{Policy, Result, candidate_validation as gate, index, require};
pub(crate) use lifetime::Custody;
use owned::WorkingView;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{BufRead, BufReader, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    process::{Command, Output},
    thread,
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Deserialize)]
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
#[derive(Debug, Clone, Deserialize)]
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
    pub semantic_check: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hosted_check: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lifecycle: Vec<owned::Note>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_kind: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_operation: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<crate::source_error::PublicationProblem>,
    pub elapsed_ms: u128,
}
impl Receipt {
    pub fn lean(&self) -> Result<String> {
        crate::lean_render::render(&serde_json::json!({
            "status":self.status,"check":self.check,"published":self.published,
            "accepted_before":self.accepted_before,"candidate":self.candidate,
            "tree":self.tree,"draft_commit":self.draft_commit,"draft_base":self.draft_base,
            "draft_branch":self.draft_branch,"version":self.version,
            "failure_kind":self.failure_kind,"failure_operation":self.failure_operation,
            "problem":self.problem,"semantic_check":self.semantic_check,
            "rule":gate::CANDIDATE_RULE,"rows":self.lifecycle,
        }))
        .map_err(|e| std::io::Error::other(e).into())
    }
}

/// Customer-facing transaction receipt; `Serialize` retains full internal diagnostics.
pub fn cli_json(receipt: &impl Serialize) -> Result<String> {
    use serde_json::json;
    let value = serde_json::to_value(receipt)?;
    let saved = value.get("home").is_some();
    let mut output = if value["status"] == "current" {
        if saved {
            json!({"status":if value["check"] == "published_checked_candidate" {"saved"} else {"recovered"},
                "accepted":value["candidate"], "version":value["version"],
                "home":value["home"], "worktree_removed":value["worktree_removed"]})
        } else {
            json!({"status":"ready", "accepted":value["candidate"],
                "version":value["version"], "view":value["view"], "home_advanced":false})
        }
    } else {
        // A failed conditional ref update is not evidence that nothing changed.
        let status = if value["published"] == true || value["publication"] == "accepted_incomplete"
        {
            "accepted_incomplete"
        } else if value["check"] == "conditional_ref_race" || value["publication"] == "unknown" {
            "unknown"
        } else {
            "not_accepted"
        };
        let mut failure = json!({"status":status, "check":value["check"]});
        if status == "accepted_incomplete" {
            failure["accepted"] = value.get("accepted").unwrap_or(&value["candidate"]).clone();
        }
        if status != "accepted_incomplete"
            || value["published"] == true
            || value["candidate"] == failure["accepted"]
        {
            if !value["version"].is_null() {
                failure["version"] = value["version"].clone();
            }
        }
        for key in ["home", "draft"] {
            if !value[key].is_null() {
                failure[key] = value[key].clone();
            }
        }
        if let Some(validation) = value
            .get("validation")
            .filter(|v| !v.is_null() && v["status"] != "current")
        {
            let mut validation = validation.clone();
            gate::customer_report(&mut validation);
            failure["validation"] = validation;
        }
        for key in ["failure_kind", "failure_operation", "failure_stage", "problem", "lifecycle", "next_action"] {
            if !value[key].is_null() {
                failure[key] = value[key].clone();
            }
        }
        failure
    };
    if let Some(notes) = value.get("lifecycle").and_then(|v| v.as_array()) {
        let incomplete: Vec<_> = notes.iter().filter(|n| n["cleanup"] != "removed").collect();
        if !incomplete.is_empty() {
            output["lifecycle"] = serde_json::to_value(incomplete)?;
        }
    }
    output["semantic_check"] = value.get("semantic_check").cloned().unwrap_or(json!("deferred"));
    if crate::hosted_gate::configured() {
        if let Some(check) = value.get("hosted_check") {
            // Keep native machine receipts parseable and content-free. The host
            // retrieves retained findings by request ID, never from a journal.
            output["hosted_check"] = crate::hosted_checks::publication_record(check);
        }
    }
    serde_json::to_string_pretty(&output).map_err(Into::into)
}

#[cfg(test)]
#[test]
fn cli_receipt_hides_graph_routing_but_keeps_actionable_validation() {
    let graph = gate::GraphCheck {
        mode: "preserved",
        reason: "base",
        base_version: Some("base-version".into()),
        candidate_version: Some("candidate-version".into()),
    };
    let receipt = Receipt {
        status: "rejected",
        check: "candidate_validation_rejected",
        accepted_before: None,
        candidate: None,
        tree: None,
        draft_commit: None,
        draft_base: None,
        draft_branch: None,
        published: false,
        version: None,
        view: None,
        validation: Some(gate::Report {
            status: "rejected",
            version: None,
            before_version: None,
            nodes: 0,
            rows: vec![gate::Row {
                id: None,
                check: "invalid_candidate",
                rule: gate::CANDIDATE_RULE,
                tokens: None,
                source: None,
                target: None,
                law: None,
            }],
            graph: graph.clone(),
            failure: None,
            problem: None,
        }),
        index_reuse: None,
        accepted_index_available: false,
        draft_index_available: false,
        graph_checks: vec![graph],
        semantic_check: "deferred",
        hosted_check: None,
        lifecycle: vec![],
        failure_kind: None,
        failure_operation: None,
        problem: None,
        elapsed_ms: 0,
    };
    let output: serde_json::Value = serde_json::from_str(&cli_json(&receipt).unwrap()).unwrap();
    assert_eq!(output["status"], "not_accepted");
    assert_eq!(output["check"], "candidate_validation_rejected");
    assert_eq!(
        output["validation"]["rows"][0]["check"],
        "invalid_candidate"
    );
    assert!(output.get("graph_checks").is_none());
    assert!(output["validation"].get("graph").is_none());
}

#[cfg(test)]
#[test]
fn cli_publication_projects_outcomes_without_success_diagnostics() {
    use serde_json::{Value, json};
    let render =
        |value: Value| -> Value { serde_json::from_str(&cli_json(&value).unwrap()).unwrap() };
    let mut value = json!({"status":"current", "check":"published_checked_candidate",
        "candidate":"commit", "version":"version", "view":"/view",
        "validation":{"status":"current","rows":[{"check":"successful"}]},
        "tree":"tree", "elapsed_ms":42});
    assert_eq!(
        render(value.clone()),
        json!({"status":"ready", "accepted":"commit",
        "version":"version", "view":"/view", "home_advanced":false})
    );
    value["home"] = json!("/home");
    value["worktree_removed"] = json!(true);
    assert_eq!(
        render(value.clone()),
        json!({"status":"saved", "accepted":"commit",
        "version":"version", "home":"/home", "worktree_removed":true})
    );
    value["check"] = json!("accepted_revision_recovered");
    assert_eq!(render(value.clone())["status"], "recovered");
    value["status"] = json!("unavailable");
    value["published"] = json!(true);
    let failure = render(value.clone());
    assert_eq!(failure["status"], "accepted_incomplete");
    assert_eq!(failure["accepted"], "commit");
    assert!(failure.get("validation").is_none());
    value["published"] = json!(false);
    value["check"] = json!("conditional_ref_race");
    assert_eq!(render(value.clone())["status"], "unknown");
    value["check"] = json!("unsafe_submission");
    assert_eq!(render(value.clone())["status"], "not_accepted");
    value["publication"] = json!("accepted_incomplete");
    value["accepted"] = json!("previously-saved");
    value["failure_kind"] = json!("timeout");
    value["lifecycle"] = json!([{"phase":"setup", "kind":"timeout", "record":"/scratch/owner.json", "cleanup":"busy"}]);
    let failure = render(value);
    assert_eq!(failure["failure_kind"], "timeout");
    assert_eq!(failure["lifecycle"][0]["cleanup"], "busy");
    assert!(failure.get("view").is_none());
    assert_eq!(failure["accepted"], "previously-saved");
    assert!(
        failure.get("version").is_none(),
        "a rejected candidate's version is not the accepted version"
    );
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
        .args(crate::GIT_CONFIG_FENCES)
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgSign=false",
            "-c",
            "merge.autoStash=false",
            "-c",
            "rerere.enabled=false",
            "-c",
            "gc.auto=0",
            "-c",
            "maintenance.auto=false",
        ])
        .args(args);
    cmd
}
// Bounded helper-inclusive execution: drain both pipes, stop oversized or hung Git.
fn run(repo: &Path, args: &[&str]) -> Result<Output> {
    run_command(command(repo, args))
}
pub(crate) fn run_command(cmd: Command) -> Result<Output> {
    lifetime::run(cmd)
}
pub(crate) fn failure_operation(error: &(dyn std::error::Error + 'static)) -> Option<&'static str> {
    lifetime::failure_operation(error)
        .or_else(|| error.source().and_then(failure_operation))
}
pub(crate) fn failure_kind(error: &dyn std::fmt::Display) -> &'static str {
    owned::error_kind(error)
}
pub(crate) fn require_git_success(operation: &str, status: std::process::ExitStatus) -> Result<()> {
    if status.success() { Ok(()) } else { Err(lifetime::nonzero(operation, status)) }
}
fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let out = run(repo, args)?;
    require_git_success(args.first().copied().unwrap_or(""), out.status)?;
    Ok(std::str::from_utf8(&out.stdout)?
        .trim_end_matches('\n')
        .to_owned())
}
fn oid(s: &str) -> bool {
    [40, 64].contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub(crate) fn commit(repo: &Path, s: &str) -> Result<String> {
    require(oid(s), "commit must be a full object ID")?;
    let got = git(repo, &["rev-parse", "--verify", &format!("{s}^{{commit}}")])?;
    require(got == s, "commit identity changed")?;
    Ok(got)
}

pub(crate) fn clean(draft: &Path, expected: &str) -> Result<()> {
    clean_draft(draft, expected)
}

fn clean_draft(draft: &Path, expected: &str) -> Result<()> {
    protect_local_artifacts(draft)?;
    require(
        git(draft, &["rev-parse", "HEAD"])? == expected,
        "draft HEAD changed",
    )?;
    native_index_guard(draft)?;
    let status = run(
        draft,
        &["status", "--porcelain=v1", "-z", "--untracked-files=all", "--ignore-submodules=none"],
    )?;
    require(
        status.status.success() && status.stdout.is_empty(),
        &format!("dirty draft at {}; preserve tracked, staged and untracked changes before submitting: {}",
            draft.display(), String::from_utf8_lossy(&status.stdout).replace('\0', " ")),
    )?;
    ignored_files(draft, false)?;
    // These operation markers belong to the worktree Git directory, including
    // linked worktrees. Resolve it once instead of spawning Git for each marker.
    let git_dir = PathBuf::from(git(draft, &["rev-parse", "--absolute-git-dir"])?);
    for name in [
        "MERGE_HEAD",
        "CHERRY_PICK_HEAD",
        "REVERT_HEAD",
        "rebase-merge",
        "rebase-apply",
    ] {
        require(
            !git_dir.join(name).try_exists()?,
            "unfinished Git operation",
        )?;
    }
    Ok(())
}

// Status omits ignored files. Unknown ignored work is not evidence of a clean
// reader either; only the durable home may retain node-owned artifacts/pointer.
pub(crate) fn ignored_files(draft: &Path, durable_home: bool) -> Result<()> {
    let ignored = run(
        draft,
        &[
            "ls-files",
            "--others",
            "--ignored",
            "--directory",
            "--exclude-standard",
            "-z",
        ],
    )?;
    require(
        ignored.status.success(),
        "ignored file inventory unavailable",
    )?;
    for file in ignored.stdout.split(|b| *b == 0).filter(|b| !b.is_empty()) {
        ignored_entry(draft, Path::new(std::str::from_utf8(file)?), durable_home)?;
    }
    Ok(())
}

fn ignored_entry(root: &Path, path: &Path, durable_home: bool) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    let parts: Vec<_> = path.iter().map(|s| s.to_str().unwrap_or_default()).collect();
    if durable_home && path == Path::new("AGENTS.md") {
        return Ok(());
    }
    if durable_home && parts.len() >= 3 && parts[0] == "nodes"
        && crate::uuid(parts[1]) && parts[2] == "artifacts" {
        // Artifact contents are opaque, including independent projects and aliases.
        for parent in [root.join("nodes"), root.join("nodes").join(parts[1])] {
            crate::real_dir(&parent)?;
        }
        return crate::real_dir(&root.join("nodes").join(parts[1]).join("artifacts"));
    }
    for parent in path.ancestors().skip(1).filter(|p| !p.as_os_str().is_empty()) {
        crate::real_dir(&root.join(parent))?;
    }
    let metadata = fs::symlink_metadata(root.join(path))?;
    if matches!(parts.as_slice(), ["nodes", id, "artifacts", "chats", _] if crate::uuid(id)) {
        return require(metadata.file_type().is_symlink(), "local chat artifact must be a symlink");
    }
    if metadata.is_dir() {
        let known = match parts.as_slice() {
            [".sifttext"] | [".sifttext", "outlines"] | [".sifttext", "outline-stage"] => true,
            [".sifttext", "outlines", id] => crate::uuid(id),
            // Git collapses a deleted owner's remaining ignored cache files into
            // this directory. Check every leaf; native indexing still requires
            // exact cached-byte ownership before retiring source-less outlines.
            ["nodes", id] => durable_home && crate::uuid(id),
            ["nodes", id, "artifacts"] | ["nodes", id, "artifacts", "chats"] => crate::uuid(id),
            _ => false,
        };
        require(known, &format!("unknown ignored directory {}; preserve it before submitting", path.display()))?;
        for entry in fs::read_dir(root.join(path))? {
            ignored_entry(root, &path.join(entry?.file_name()), durable_home)?;
        }
        return Ok(());
    }
    require(
        metadata.is_file() && metadata.nlink() == 1,
        "ignored entry is not a disposable regular file; preserve it before submitting",
    )?;
    require(
        crate::token_metadata::derived(path)
            || disposable_outline(path)
            || (durable_home && path == Path::new(".sifttext/config.json"))
            || [
                ".sifttext/index.sqlite",
                ".sifttext/index.sqlite-journal",
                ".sifttext/index.sqlite-wal",
                ".sifttext/index.sqlite-shm",
                ".sifttext/refresh.lock",
            ].iter().any(|cache| path == Path::new(cache)),
        &format!("ignored user file {}; preserve it before submitting", path.display()),
    )
}

/// Refuse user-owned material during draft preflight, before Git's recursive
/// ignored-file inventory. Chat links retain their separate checked contract.
/// Durable-home artifacts never enter the disposable editing worktree.
pub(crate) fn protect_local_artifacts(root: &Path) -> Result<()> {
    let nodes = root.join("nodes");
    match fs::symlink_metadata(&nodes) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
        Ok(_) => crate::real_dir(&nodes)?,
    }
    for node in fs::read_dir(nodes)? {
        let node = node?;
        if !crate::uuid(node.file_name().to_str().unwrap_or_default()) {
            continue;
        }
        crate::real_dir(&node.path())?;
        let artifacts = node.path().join("artifacts");
        match fs::symlink_metadata(&artifacts) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
            Ok(_) => crate::real_dir(&artifacts)?,
        }
        for entry in fs::read_dir(&artifacts)? {
            require(
                entry?.file_name() == "chats",
                "draft-local artifacts are not disposable; preserve them and deliberately move them to the owning node in the durable home before submitting",
            )?;
        }
    }
    Ok(())
}

/// Check only container/owner identities, never artifact contents. A node may
/// not disappear from accepted knowledge while its durable container remains.
pub(crate) fn protect_artifact_owners(home: &Path, target: &str) -> Result<()> {
    let nodes = home.join("nodes");
    if !nodes.try_exists()? {
        return Ok(());
    }
    crate::real_dir(&nodes)?;
    let mut owners = Vec::new();
    for node in fs::read_dir(nodes)? {
        let node = node?;
        let id = node.file_name();
        if !crate::uuid(id.to_str().unwrap_or_default()) {
            continue;
        }
        crate::real_dir(&node.path())?;
        let artifacts = node.path().join("artifacts");
        match fs::symlink_metadata(&artifacts) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
            Ok(_) => crate::real_dir(&artifacts)?,
        }
        owners.push(format!("nodes/{}/node.md", id.to_str().ok_or("invalid owner")?));
    }
    if !owners.is_empty() {
        let tree = run(home, &["ls-tree", "-rz", "--name-only", target])?;
        require(tree.status.success(), "artifact owner tree unavailable")?;
        let paths: BTreeSet<_> = tree.stdout.split(|b| *b == 0).collect();
        for owner in owners {
            require(paths.contains(owner.as_bytes()),
                "artifact owner removal refused; deliberately rehome durable artifacts and remove the old container, or retain the owner")?;
        }
    }
    Ok(())
}

#[cfg(test)]
#[test]
fn clean_checks_operation_markers_in_each_worktree_git_directory() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    git(
        temp.path(),
        &["init", "-q", "-b", "main", repo.to_str().unwrap()],
    )
    .unwrap();
    git(&repo, &["commit", "--allow-empty", "-qm", "fixture"]).unwrap();
    let head = git(&repo, &["rev-parse", "HEAD"]).unwrap();
    let linked = temp.path().join("linked");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "--detach",
            linked.to_str().unwrap(),
            &head,
        ],
    )
    .unwrap();
    for view in [&repo, &linked] {
        clean(view, &head).unwrap();
        let git_dir = PathBuf::from(git(view, &["rev-parse", "--absolute-git-dir"]).unwrap());
        for name in [
            "MERGE_HEAD",
            "CHERRY_PICK_HEAD",
            "REVERT_HEAD",
            "rebase-merge",
            "rebase-apply",
        ] {
            let marker = git_dir.join(name);
            assert_eq!(
                marker,
                PathBuf::from(
                    git(
                        view,
                        &["rev-parse", "--path-format=absolute", "--git-path", name]
                    )
                    .unwrap()
                )
            );
            if name.starts_with("rebase-") {
                fs::create_dir(&marker).unwrap();
            } else {
                fs::write(&marker, &head).unwrap();
            }
            assert!(clean(view, &head).is_err(), "{name} accepted in {view:?}");
            if marker.is_dir() {
                fs::remove_dir(&marker).unwrap();
            } else {
                fs::remove_file(&marker).unwrap();
            }
            clean(view, &head).unwrap();
        }
    }
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

/// Reject hidden edits and malformed indexes, while allowing Git-owned sparse
/// omission in linked editing worktrees. Immutable Git trees remain the complete
/// acceptance input; skip-worktree is not evidence of a deletion.
pub(crate) fn native_index_guard(repo: &Path) -> Result<()> {
    let sparse = crate::local::sparse_checkout(repo)?;
    if sparse {
        require(fs::symlink_metadata(repo.join(".git"))?.is_file(),
            "sparse checkout is supported only in linked editing worktrees; keep the normal home complete")?;
    }
    let config = run(repo, &["config", "--null", "--get-regexp", "^core\\.splitindex$"])?;
    require(matches!(config.status.code(), Some(0 | 1)), "Git configuration unavailable")?;
    for entry in config.stdout.split(|b| *b == 0).filter(|b| !b.is_empty()) {
        let entry = std::str::from_utf8(entry)?;
        let (key, value) = entry.split_once('\n').ok_or("invalid Git configuration")?;
        require(matches!(value.to_ascii_lowercase().as_str(), "false" | "no" | "off" | "0"),
            &format!("unsupported Git configuration {key}; preserve files and disable split index before retrying"))?;
    }
    require(git(repo, &["rev-parse", "--shared-index-path"])?.is_empty(),
        "split Git index unsupported; preserve files and disable split index before retrying")?;
    let git_dir = PathBuf::from(git(repo, &["rev-parse", "--absolute-git-dir"])?);
    for name in ["config.worktree", "info/sparse-checkout"] {
        let path = git_dir.join(name);
        match fs::symlink_metadata(&path) {
            Ok(meta) => {
                use std::os::unix::fs::MetadataExt;
                require(meta.is_file() && meta.nlink() == 1,
                    &format!("unsafe Git configuration at {}; preserve files before retrying", path.display()))?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    let attributes = PathBuf::from(git(repo, &["rev-parse", "--path-format=absolute", "--git-path", "info/attributes"])?);
    let metadata = fs::symlink_metadata(&attributes);
    require(metadata.as_ref().is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
        || metadata.is_ok_and(|m| m.is_file() && m.len() == 0),
        &format!("unsupported repository attributes at {}; preserve files before retrying", attributes.display()))?;
    let check_attributes = |parent: &Path| -> Result<()> {
        let path = repo.join(parent).join(".gitattributes");
        require(fs::symlink_metadata(&path).is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound),
            &format!("unsupported working attributes at {}; preserve files before retrying", path.display()))
    };
    check_attributes(Path::new(""))?;
    let mut files = tempfile::tempfile()?;
    let output = lifetime::run_stdout_file(command(repo, &["ls-files", "-v", "-z"]), &files)?;
    require(output.status.success(), "Git index inventory unavailable")?;
    if files.metadata()?.len() != 0 {
        files.seek(SeekFrom::End(-1))?;
        let mut end = [0];
        files.read_exact(&mut end)?;
        require(end == [0], "truncated Git index inventory; preserve files before retrying")?;
    }
    files.rewind()?;
    let mut previous_parent = PathBuf::new();
    for entry in BufReader::new(files).split(0) {
        let entry = entry?;
        require(entry.starts_with(b"H ") || (sparse && entry.starts_with(b"S ")),
            &format!("hidden, unmerged or malformed Git index entry {}; preserve files and clear assume-unchanged or resolve the index before retrying", String::from_utf8_lossy(&entry)))?;
        let text = std::str::from_utf8(&entry[2..])?;
        require(!text.is_empty() && !text.starts_with('/')
            && text.split('/').all(|part| !part.is_empty() && part != "." && part != ".."),
            "unsafe Git index path; preserve files before retrying")?;
        let parent = Path::new(text).parent().ok_or("missing tracked parent")?;
        for ancestor in parent.ancestors() {
            if !previous_parent.starts_with(ancestor) { check_attributes(ancestor)?; }
        }
        previous_parent = parent.to_path_buf();
    }
    drop(previous_parent);
    crate::release_unused_memory();
    Ok(())
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
    // Pin both the raw ref and its commit peel in one process. Equality still
    // rejects tag/blob/tree refs rather than silently accepting a peeled target.
    let resolved = git(
        repo,
        &["rev-parse", reference, &format!("{reference}^{{commit}}")],
    )?;
    let (tip, peeled) = resolved.split_once('\n').ok_or("missing commit identity")?;
    require(oid(tip) && tip == peeled, "commit identity changed")?;
    // Moving a checked-out branch would silently invalidate a user's index/draft.
    let worktrees = git(repo, &["worktree", "list", "--porcelain"])?;
    require(
        !worktrees
            .lines()
            .any(|l| l == format!("branch {reference}")),
        "accepted ref is checked out; detach its reader first",
    )?;
    Ok(tip.to_owned())
}
#[cfg(test)]
#[test]
fn accepted_ref_requires_exact_commit_and_detached_readers() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path();
    git(repo, &["init", "-q", "-b", "main"]).unwrap();
    fs::write(repo.join("node.md"), "committed content").unwrap();
    git(repo, &["add", "node.md"]).unwrap();
    git(repo, &["commit", "-qm", "knowledge: ref identity control"]).unwrap();
    let head = git(repo, &["rev-parse", "HEAD"]).unwrap();
    let reference = "refs/heads/main";
    assert!(accepted(repo, reference).is_err()); // Checked-out branch.
    git(repo, &["checkout", "--detach", "-q"]).unwrap();
    assert_eq!(accepted(repo, reference).unwrap(), head);
    for invalid in ["main", "refs/heads/missing", "refs/heads/bad..name"] {
        assert!(accepted(repo, invalid).is_err());
    }
    git(repo, &["symbolic-ref", "refs/heads/alias", reference]).unwrap();
    assert!(accepted(repo, "refs/heads/alias").is_err());
    git(repo, &["tag", "-a", "control", "-m", "tag identity"]).unwrap();
    let tag = git(repo, &["rev-parse", "refs/tags/control"]).unwrap();
    let tree = git(repo, &["rev-parse", "HEAD^{tree}"]).unwrap();
    let blob = git(repo, &["rev-parse", "HEAD:node.md"]).unwrap();
    // Bypass update-ref's own branch type guard to exercise corrupt raw refs.
    for object in [&tag, &tree, &blob] {
        fs::write(repo.join(".git/refs/heads/raw"), format!("{object}\n")).unwrap();
        assert!(accepted(repo, "refs/heads/raw").is_err());
    }
    let linked = temp.path().join("reader");
    git(
        repo,
        &["worktree", "add", "-q", linked.to_str().unwrap(), "main"],
    )
    .unwrap();
    assert!(accepted(repo, reference).is_err());
    git(&linked, &["checkout", "--detach", "-q"]).unwrap();
    assert_eq!(accepted(repo, reference).unwrap(), head);
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
            ["trees", ".keep"] | ["nodes", ".keep"] => regular,
            ["trees", id] => link && crate::uuid(id),
            ["nodes", id, "node.md" | "children.order" | "session-sources.json"] => {
                regular && crate::uuid(id)
            }
            ["nodes", id, "children", ".keep"] => regular && crate::uuid(id),
            ["nodes", id, "children", child] => link && crate::uuid(id) && crate::uuid(child),
            _ => false,
        };
        require(allowed, "unsafe or tracked non-authored Git entry")?;
    }
    Ok(())
}
/// Retire a matching verified retained reader through the same exact finish
/// used for disposable views. This never adopts an unrecorded legacy view.
pub(crate) fn remove_owned_view(
    repo: &Path,
    receipt: &Receipt,
    identity: &fs::Metadata,
) -> Result<()> {
    owned::remove_returned(repo, receipt, identity)
}

#[cfg(test)]
#[test]
fn authored_tree_refuses_force_tracked_artifacts_including_chat_links() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path();
    git(repo, &["init", "-q", "-b", "main"]).unwrap();
    let artifacts = repo.join("nodes/00000000-0000-4000-8000-000000000001/artifacts");
    fs::create_dir_all(&artifacts).unwrap();
    fs::write(artifacts.join("budget.csv"), b"item,quantity\nwidget,10\n").unwrap();
    git(repo, &["add", "."]).unwrap();
    git(repo, &["commit", "-qm", "fixture: force-tracked task file"]).unwrap();
    let head = git(repo, &["rev-parse", "HEAD"]).unwrap();
    assert!(authored_tree(repo, &head).is_err());
    fs::create_dir(artifacts.join("chats")).unwrap();
    std::os::unix::fs::symlink("/unavailable-session", artifacts.join("chats/session.jsonl")).unwrap();
    git(repo, &["rm", "--cached", "-r", "nodes"]).unwrap();
    git(repo, &["add", "nodes/00000000-0000-4000-8000-000000000001/artifacts/chats"]).unwrap();
    git(repo, &["commit", "-qm", "fixture: force-tracked chat link"]).unwrap();
    let head = git(repo, &["rev-parse", "HEAD"]).unwrap();
    assert!(authored_tree(repo, &head).is_err());
    assert_eq!(fs::read(artifacts.join("budget.csv")).unwrap(), b"item,quantity\nwidget,10\n");
}

/// Submit once, or recover the currently accepted ref. No caller pull or checkout
/// modification. Errors fail closed; a post-CAS error is explicitly unavailable.
pub fn submit(policy: &Policy, request: &Request) -> Receipt {
    submit_inner(policy, request, false, None, &mut |_, _| Ok(()), None, None)
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
    submit_inner(policy, request, true, previous_heads, &mut record, None, None)
}

pub(crate) fn save_prepared_leased(
    policy: &Policy,
    request: &Request,
    lease: &crate::local::DraftLease,
    reader: Option<&mut crate::git_snapshot::GitReader>,
    previous_heads: Option<(&str, &str)>,
    mut record: impl FnMut(&str, &str) -> Result<()>,
) -> Receipt {
    submit_inner(
        policy, request, true, previous_heads, &mut record, Some(lease), reader,
    )
}

fn submit_inner(
    policy: &Policy,
    request: &Request,
    in_draft: bool,
    previous_heads: Option<(&str, &str)>,
    record: &mut dyn FnMut(&str, &str) -> Result<()>,
    lease: Option<&crate::local::DraftLease>,
    reader: Option<&mut crate::git_snapshot::GitReader>,
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
        semantic_check: "deferred",
        hosted_check: None,
        lifecycle: vec![],
        failure_kind: None,
        failure_operation: None,
        problem: None,
        elapsed_ms: 0,
    };
    let notes = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let result = (|| -> Result<()> {
        // Every library entry owns custody too. The completion layer passes its
        // existing lease so the lock stays held through normal-home finishing.
        let owned;
        let lease = if let Some(lease) = lease {
            Some(lease)
        } else if let Some(draft) = &request.draft {
            owned = crate::local::DraftLease::acquire(&request.repo, draft)?;
            Some(&owned)
        } else {
            None
        };
        if let Some(lease) = lease {
            lease.check(&request.repo, request.draft.as_deref())?;
        }
        let _custody = lease.map(|l| l.custody()).transpose()?;
        execute(
            policy,
            request,
            &mut receipt,
            in_draft,
            previous_heads,
            record,
            &notes,
            reader,
        )
    })();
    receipt.lifecycle = notes.take();
    receipt.problem = result.as_ref().err().map(|e|
        crate::source_error::PublicationProblem::from_error(e.as_ref(), owned::error_kind(e)));
    receipt.failure_kind = receipt.problem.as_ref().map(|p| p.kind);
    receipt.failure_operation = result.as_ref().err()
        .and_then(|error| failure_operation(error.as_ref()));
    if let Err(error) = &result {
        eprintln!("submission {}: {}", receipt.check,
            error.to_string().chars().take(4096).collect::<String>());
    }
    if result.is_err() && receipt.published {
        receipt.status = "unavailable";
        receipt.check = "published_view_requires_recovery";
        receipt.view = None;
    }
    receipt.elapsed_ms = start.elapsed().as_millis();
    if std::env::var_os("SIFTTEXT_RETRIEVAL_TRACE").is_some() {
        eprintln!("publication-phase phase=candidate_acceptance elapsed_ms={} outcome={}",
            receipt.elapsed_ms, if result.is_ok() { "complete" } else { "incomplete" });
    }
    receipt
}
fn apply_git_normalization(
    reader: &mut crate::git_snapshot::GitReader,
    candidate: &crate::git_snapshot::GitView,
    view: &Path,
    normalized: &crate::status::Acceptance,
    patches: &BTreeMap<String, Vec<u8>>,
) -> Result<BTreeMap<String, String>> {
    // A status change may reopen an omitted ancestor. Expand exactly the nodes
    // carrying planned patches at this candidate revision, never the whole draft.
    let ids: Vec<_> = normalized.normalized_ids.iter().cloned().collect();
    crate::local::materialize_nodes(view, &ids)?;
    crate::real_dir(&view.join("nodes"))?;
    let mut staged = BTreeMap::new();
    let mut blobs = BTreeMap::new();
    // Stage complete Markdown before any replacement. Hash those exact bytes,
    // not a mutable destination later read by `git add`.
    for id in &normalized.normalized_ids {
        let directory = view.join("nodes").join(id);
        crate::real_dir(&directory)?;
        let destination = directory.join("node.md");
        let mut file = tempfile::NamedTempFile::new_in(&directory)?;
        file.write_all(&patches[id])?;
        file.as_file().set_permissions(fs::symlink_metadata(&destination)?.permissions())?;
        file.as_file().sync_all()?;
        let blob = git(view, &["hash-object", "-w", "--no-filters", "--", file.path().to_str().ok_or("non-UTF-8 patch path")?])?;
        require(oid(&blob), "invalid normalization blob")?;
        staged.insert(id.clone(), file);
        blobs.insert(id.clone(), blob);
    }
    crate::status::apply_changes(&normalized.changes, |id| {
        let directory = view.join("nodes").join(id);
        crate::real_dir(&view.join("nodes"))?;
        crate::real_dir(&directory)?;
        let destination = directory.join("node.md");
        require(
            crate::read_regular(&destination)? == reader.bytes(candidate, id)?,
            &format!("node {id}: candidate bytes changed before normalization; retain the draft"),
        )?;
        staged.remove(id).ok_or("missing staged normalization")?.persist(&destination)?;
        fs::File::open(directory)?.sync_all()?;
        Ok(())
    })?;
    Ok(blobs)
}

fn execute(
    policy: &Policy,
    q: &Request,
    r: &mut Receipt,
    in_draft: bool,
    previous_heads: Option<(&str, &str)>,
    record: &mut dyn FnMut(&str, &str) -> Result<()>,
    notes: &owned::Notes,
    reader: Option<&mut crate::git_snapshot::GitReader>,
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
        clean_draft(&path, if in_draft { &head } else { &sha })?;
        let branch = git(&path, &["symbolic-ref", "--quiet", "HEAD"])?;
        require(branch != q.accepted_ref, "draft is accepted branch")?;
        r.draft_branch = Some(branch);
        if sha != main {
            authored_tree(&repo, &sha)?;
        }
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
    // Normal publication reads immutable Git objects, not retained view donors.
    // Recovering unrelated historical views is not part of saving this draft.
    if !in_draft {
        owned::recover(&repo, &root, notes)?;
    }
    r.check = "owned_view_setup";
    let mut owned_reader = if reader.is_none() {
        Some(crate::git_snapshot::GitReader::new(policy, &repo)?)
    } else { None };
    let mut reader = reader.or(owned_reader.as_mut()).ok_or("missing Git reader")?;
    require(reader.repo == repo && reader.policy_identity == policy.validation_identity()?,
        "prepared Git reader does not match publication")?;
    let before = reader.view(&main)?;
    // The lifecycle refreshes the real home's existing derived state after
    // acceptance. Do not hydrate donors or build a second full temporary index.
    let candidate = if in_draft {
        None
    } else {
        Some(WorkingView::new(&repo, &root, &main, notes)?)
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
    let mut sha = if in_draft {
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
    if let Some(candidate) = &candidate {
        candidate.revision(&sha)?;
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
        // Reset only the exact successful native merge. Recheck mutable files
        // here, at the destructive boundary, not as acceptance of Git objects.
        require(
            git(view, &["write-tree"])? == *r.tree.as_ref().ok_or("missing candidate tree")?,
            "merge index changed; retain the draft",
        )?;
        native_index_guard(view)?;
        let changes = run(view, &["diff-files", "--no-ext-diff", "--name-only", "-z", "--ignore-submodules=none"])?;
        require(changes.status.success() && changes.stdout.is_empty(),
            "working files changed after merge; preserve the draft before retrying")?;
        git(view, &["reset", "--hard", &sha])?;
    }
    // Main and the submitted commit were already checked above. Only a new
    // combination needs another authored-path inventory.
    if sha != main && q.commit.as_deref() != Some(sha.as_str()) {
        authored_tree(&repo, &sha)?;
    }
    let candidate_view = reader.view(&sha)?;
    require(Some(&candidate_view.tree) == r.tree.as_ref(), "candidate tree identity changed")?;
    r.check = "candidate_normalization_rejected";
    let (normalized, patches) = crate::status::normalize_git(&mut reader, &before, &candidate_view, &q.explicit)?;
    normalized.report_reopenings();
    let final_view = if !patches.is_empty() {
        // Build from exact patches in a private index: never scoop up unrelated
        // staged edits or let a concurrent body write change the checked tree.
        policy.verify_stamp()?;
        let blobs = apply_git_normalization(&mut reader, &candidate_view, view, &normalized, &patches)?;
        drop(normalized);
        drop(patches);
        let old_tree = git(view, &["rev-parse", &format!("{sha}^{{tree}}")])?;
        require(
            git(view, &["write-tree"])? == old_tree,
            "candidate index changed",
        )?;
        let temporary = tempfile::tempdir()?;
        let index_path = temporary.path().join("index");
        let indexed = |args: &[&str]| -> Result<String> {
            let mut cmd = command(view, args);
            cmd.env("GIT_INDEX_FILE", &index_path);
            let out = run_command(cmd)?;
            require(out.status.success(), "normalization index unavailable")?;
            Ok(std::str::from_utf8(&out.stdout)?
                .trim_end_matches('\n')
                .to_owned())
        };
        indexed(&["read-tree", &sha])?;
        for (id, blob) in &blobs {
            indexed(&["update-index", "--add", "--cacheinfo", "100644", blob, &format!("nodes/{id}/node.md")])?;
        }
        let tree = indexed(&["write-tree"])?;
        let metadata = git(
            view,
            &[
                "log",
                "-1",
                "--format=%an%x00%ae%x00%(trailers:key=Co-authored-by)",
                submitted,
            ],
        )?;
        let mut cmd = command(view, &["commit-tree", &tree, "-p", &sha]);
        crate::checked_publication_attribution::integration_commit(&mut cmd, &metadata)?;
        let out = run_command(cmd)?;
        require(out.status.success(), "normalization commit rejected")?;
        let next = std::str::from_utf8(&out.stdout)?.trim().to_owned();
        commit(&repo, &next)?;
        require(
            git(view, &["write-tree"])? == old_tree,
            "candidate index changed",
        )?;
        // The tree contains only exact patches over the immutable combination.
        // New working edits are not acceptance input; completed drafts are disposable.
        require(
            git(view, &["rev-parse", "HEAD"])? == sha,
            "candidate HEAD changed",
        )?;
        if in_draft {
            record(&sha, &next)?;
        }
        if let Some(candidate) = &candidate {
            candidate.revision(&next)?;
        }
        git(view, &["update-ref", "HEAD", &next, &sha])?;
        // Synchronize only our paths. Never replace the entire real index: a
        // concurrently staged unrelated edit must remain available for repair.
        for (id, blob) in &blobs {
            git(view, &["update-index", "--add", "--cacheinfo", "100644", blob, &format!("nodes/{id}/node.md")])?;
        }
        sha = next;
        r.candidate = Some(sha.clone());
        r.tree = Some(tree);
        // The checked authored tree plus exact node.md patches cannot introduce
        // another path or mode. Do not enumerate that whole tree again.
        reader.view(&sha)?
    } else {
        drop(normalized);
        drop(patches);
        candidate_view
    };
    require(Some(&final_view.tree) == r.tree.as_ref(), "final candidate tree identity changed")?;
    r.check = "candidate_validation_rejected";
    // Execute the full law once on exact Git input. The baseline supplies
    // history, not a second full proof or a mutable filesystem recapture.
    let checked = gate::validate_git(policy, &mut reader, &before, &final_view, &q.explicit);
    r.graph_checks.push(checked.graph.clone());
    r.version = checked.version.clone();
    let valid = checked.status == "current";
    r.validation = Some(checked);
    require(valid, "combined candidate rejected")?;
    require(r.version.as_deref() == Some(final_view.version.as_str()), "validated candidate version mismatch")?;
    // The binary owns this gate: caller instructions and caller-supplied receipts
    // cannot authorize paid publication. Check only the exact combined candidate.
    if crate::hosted_gate::configured() {
        r.check = "hosted_parent_unavailable";
        // Initial immutable acceptance has no accepted baseline. A new submit
        // must use the final combined view; a recovery never checks a backlog.
        let result = crate::hosted_gate::check(&mut reader,
            if draft.is_some() { Some(&before) } else { None },
            &final_view, &sha, draft.is_some() && already)?;
        r.semantic_check = match result["status"].as_str() {
            Some("passed") => "passed",
            Some("findings") => "findings",
            Some("not_required") => "not_required",
            _ => "unchecked",
        };
        r.hosted_check = Some(result);
        if r.semantic_check == "findings" {
            r.check = "hosted_epistemic_findings";
            return Err("hosted epistemic findings; draft preserved".into());
        }
    } else if draft.is_some() || common.join("sifttext-hosted-checks").try_exists()? {
        let records = common.join("sifttext-hosted-checks");
        if !records.try_exists()? { fs::create_dir(&records)?; }
        crate::real_dir(&records)?;
        let path = records.join(format!("{}.json", final_view.tree));
        if draft.is_some() && !already {
            r.check = "hosted_epistemic_unavailable";
            let result = crate::hosted_checks::publication(&mut reader, &before, &final_view);
            let mut file = tempfile::NamedTempFile::new_in(&records)?;
            serde_json::to_writer(&mut file, &crate::hosted_checks::publication_record(&result))?;
            file.as_file().sync_all()?;
            file.persist(&path)?;
            fs::File::open(&records)?.sync_all()?;
            r.semantic_check = match result["status"].as_str() {
                Some("passed") => "passed",
                Some("findings") => "findings",
                Some("not_required") => "not_required",
                _ => "unchecked",
            };
            r.hosted_check = Some(result);
            if r.semantic_check == "findings" {
                r.check = "hosted_epistemic_findings";
                return Err("hosted epistemic findings; draft preserved".into());
            }
        } else if path.try_exists()? {
            let result: serde_json::Value = serde_json::from_slice(&crate::read_regular(&path)?)?;
            // Historical diagnostics only: never use this file to authorize a new candidate.
            r.semantic_check = match result["status"].as_str() {
                Some("passed") => "passed",
                Some("not_required") => "not_required",
                _ => "unchecked",
            };
            r.hosted_check = Some(result);
        }
    }
    drop(final_view);
    drop(before);
    reader.release_views();
    reader.suspend_objects();
    drop(owned_reader);
    crate::token_count::release();
    crate::release_unused_memory();
    r.check = "derived_view_unavailable";
    // Full checked refresh remains independent of graph preservation. Only
    // verified assembled records/search bodies cross views, not acceptance proofs;
    // Git never merges SQLite or generated files.
    if draft.is_none() {
        // The explicit accepted-view operation still exposes a ready standalone
        // view. Git omits empty directories; this bounded legacy adapter is not
        // used by normal publication or immutable acceptance.
        for name in ["nodes", "trees", ".sifttext"] {
            let path = view.join(name);
            if !path.try_exists()? { fs::create_dir(&path)?; }
            crate::real_dir(&path)?;
        }
        let (version, ()) = index::with_current(policy, view, |_, _| Ok(()))?;
        require(Some(version) == r.version, "derived candidate mismatch")?;
    }
    if let Some(barrier) = &q.pause_before_publish {
        barrier.wait(&[&repo, &common], view)?;
    }
    r.check = "source_or_ref_changed";
    if let Some(draft) = &draft {
        require(
            git(draft, &["rev-parse", "HEAD"])? == if in_draft { &sha } else { submitted },
            "draft HEAD changed; retain the draft",
        )?;
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
    // Reuse the successful exact-candidate gate in this invocation. Keep final
    // source/policy barriers, not another graph/outline
    // recomputation over the same input or a caller-forgeable receipt.
    r.check = "candidate_recheck_rejected";
    policy.verify_stamp()?;
    r.check = "artifact_owner_removed";
    protect_artifact_owners(&repo, &sha)?;
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
    // Acceptance is over immutable Git input, not post-publication cleanliness.
    // Editing drafts are disposable; retained observer views are checked once
    // by WorkingView::retain below.
    policy.verify_stamp()?;
    r.view = Some(if in_draft {
        view.clone()
    } else {
        candidate.ok_or("missing candidate")?.retain(&sha)?
    });
    r.status = "current";
    r.check = if already {
        "accepted_revision_recovered"
    } else {
        "published_checked_candidate"
    };
    Ok(())
}
