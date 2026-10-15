//! Core same-draft checks. CLI tests own normal-home completion and cleanup.
use super::{git, request};
use crate::{Policy, checked_publication};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn seed(repo: &Path) {
    fs::create_dir(repo).unwrap();
    // Match the initialized-home contract used by the normal reader advancement.
    fs::write(repo.join(".gitignore"), "/.sifttext/\n/nodes/*/node.meta.json\n/nodes/*/outline*.lean\n/nodes/*/outline*.lean.meta.json\n").unwrap();
    fs::create_dir(repo.join("trees")).unwrap();
    for n in 1..=2 {
        let id = format!("60000000-0000-4000-8000-{n:012}");
        let node = repo.join("nodes").join(&id);
        fs::create_dir_all(node.join("children")).unwrap();
        fs::write(node.join("children/.keep"), "").unwrap();
        fs::write(node.join("children.order"), "").unwrap();
        fs::write(node.join("node.md"), format!("---\nid: \"{id}\"\nname: \"Node {n}\"\nmicro_summary: \"Summary {n}\"\nstatus: \"in_progress\"\nsee_also: []\n---\nOriginal {n}\n")).unwrap();
        std::os::unix::fs::symlink(format!("../nodes/{id}"), repo.join("trees").join(id)).unwrap();
    }
    git(repo, &["init", "-q", "-b", "main"]);
    git(repo, &["add", "."]);
    git(repo, &["commit", "-qm", "knowledge: baseline"]);
    git(repo, &["checkout", "--detach", "-q"]);
}
fn draft(repo: &Path, parent: &Path, branch: &str, base: &str) -> PathBuf {
    let path = parent.join(branch);
    git(
        repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            branch,
            path.to_str().unwrap(),
            base,
        ],
    );
    fs::canonicalize(path).unwrap()
}
fn edit(path: &Path, n: u64, text: &str) -> String {
    let file = path.join(format!("nodes/60000000-0000-4000-8000-{n:012}/node.md"));
    let old = fs::read_to_string(&file).unwrap();
    fs::write(&file, old.replace(&format!("Original {n}"), text)).unwrap();
    git(path, &["add", "."]);
    git(path, &["commit", "-qm", "knowledge: update"]);
    git(path, &["rev-parse", "HEAD"])
}
fn save(
    policy: &Policy,
    repo: &Path,
    path: &Path,
    sha: &str,
    base: &str,
) -> checked_publication::Receipt {
    let mut q = request(repo);
    q.draft = Some(path.into());
    q.commit = Some(sha.into());
    q.base = Some(base.into());
    checked_publication::save(policy, &q)
}
fn navigation(p: &Path) -> std::collections::BTreeMap<PathBuf, (Vec<u8>, u64, i64, i64)> {
    use std::os::unix::fs::MetadataExt;
    let mut files = std::collections::BTreeMap::new();
    let mut dirs = vec![p.join("nodes"), p.join(".sifttext/outlines")];
    while let Some(dir) = dirs.pop() {
        if !dir.exists() {
            continue;
        }
        for entry in fs::read_dir(dir).unwrap() {
            let entry = entry.unwrap();
            let kind = entry.file_type().unwrap();
            if kind.is_dir() && entry.file_name() != "children" {
                dirs.push(entry.path());
            } else if kind.is_file()
                && entry
                    .file_name()
                    .as_encoded_bytes()
                    .starts_with(b"outline.")
            {
                let path = entry.path();
                let m = entry.metadata().unwrap();
                files.insert(
                    path.strip_prefix(p).unwrap().into(),
                    (fs::read(path).unwrap(), m.ino(), m.mtime(), m.mtime_nsec()),
                );
            }
        }
    }
    assert!(!files.is_empty(), "save did not materialize navigation");
    files
}

fn navigation_bytes(
    files: &std::collections::BTreeMap<PathBuf, (Vec<u8>, u64, i64, i64)>,
) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    files
        .iter()
        .map(|(p, v)| (p.clone(), v.0.clone()))
        .collect()
}

#[test]
fn navigation_snapshot_detects_bytes_sidecars_and_replacement_without_following_links() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    let node = root.join("nodes/node");
    fs::create_dir_all(node.join("children")).unwrap();
    let file = node.join("outline.lean");
    fs::write(&file, "rows").unwrap();
    fs::write(node.join("outline.lean.meta.json"), "cost").unwrap();
    std::os::unix::fs::symlink(&node, node.join("children/loop")).unwrap();
    let before = navigation(root);
    assert_eq!(navigation(root), before);
    assert_eq!(before.len(), 2);
    fs::write(&file, "changed rows").unwrap();
    assert_ne!(navigation(root), before);
    fs::write(&file, "rows").unwrap();
    let restored = navigation(root);
    fs::write(node.join("outline.lean.meta.json"), "changed cost").unwrap();
    assert_ne!(navigation(root), restored);
    let prior = navigation(root);
    fs::rename(&file, node.join("retired")).unwrap();
    fs::write(&file, "rows").unwrap();
    let replaced = navigation(root);
    assert_eq!(
        prior[Path::new("nodes/node/outline.lean")].0,
        replaced[Path::new("nodes/node/outline.lean")].0
    );
    assert_ne!(prior, replaced);
    assert_eq!(navigation_bytes(&prior), navigation_bytes(&replaced));
}

#[test]
fn save_preserves_fast_forward_and_checks_divergence_in_the_original_draft() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().canonicalize().unwrap();
    let repo = dir.join("home");
    seed(&repo);
    let base = git(&repo, &["rev-parse", "main"]);
    let a = draft(&repo, &dir, "a", &base);
    let b = draft(&repo, &dir, "b", &base);
    let a_sha = edit(&a, 1, "First");
    let b_sha = edit(&b, 2, "Second");
    let policy = Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    for path in [&a, &b] {
        crate::index::refresh(&policy, path, None, crate::index::Mode::Full, None).unwrap();
    }
    let first = save(&policy, &repo, &a, &a_sha, &base);
    assert_eq!(first.status, "current", "{first:?}");
    assert_eq!(first.candidate.as_deref(), Some(a_sha.as_str()));
    assert_eq!(first.view.as_deref(), Some(a.as_path()));
    assert_eq!(git(&b, &["rev-parse", "HEAD"]), b_sha);
    // The normal wrapper advances home after each save, making it the next
    // accepted-index donor. Keep completion/cleanup in its existing CLI tests.
    crate::local::advance(&policy, &repo).unwrap();
    let home_after_first = git(&repo, &["rev-parse", "HEAD"]);
    assert_eq!(home_after_first, a_sha);
    let donor_navigation = navigation(&a);
    let home_navigation = navigation(&repo);
    let second = save(&policy, &repo, &b, &b_sha, &base);
    assert_eq!(second.status, "current", "{second:?}");
    assert_ne!(second.version, first.version);
    assert_eq!(second.validation.as_ref().unwrap().status, "current");
    assert_eq!(second.graph_checks.len(), 1); // Recheck retains the same successful validation.
    for check in &second.graph_checks {
        assert_eq!(check.mode, "preserved");
        assert_eq!(check.base_version, first.version);
        assert_eq!(check.candidate_version, second.version);
    }
    assert!(second.accepted_index_available && second.draft_index_available);
    let reuse = second.index_reuse.as_ref().unwrap();
    assert_eq!(
        (reuse.accepted_main, reuse.prepared_worktree, reuse.computed),
        (1, 1, 0)
    );
    let combined_navigation = navigation(&b);
    assert_eq!(
        navigation_bytes(&combined_navigation),
        navigation_bytes(&donor_navigation)
    );
    assert_eq!(navigation(&a), donor_navigation);
    assert_eq!(second.view.as_deref(), Some(b.as_path()));
    let sha = second.candidate.as_deref().unwrap();
    assert_eq!(git(&b, &["rev-parse", "HEAD"]), sha);
    assert_eq!(git(&repo, &["rev-parse", "main"]), sha);
    assert_eq!(
        git(&b, &["rev-list", "--parents", "-n", "1", sha]),
        format!("{sha} {b_sha} {a_sha}")
    );
    assert!(
        fs::read_to_string(b.join("nodes/60000000-0000-4000-8000-000000000001/node.md"))
            .unwrap()
            .contains("First")
    );
    assert!(
        fs::read_to_string(b.join("nodes/60000000-0000-4000-8000-000000000002/node.md"))
            .unwrap()
            .contains("Second")
    );
    assert_eq!(
        crate::outlines::inspect(&policy, &b, None).unwrap().version,
        second.version
    );
    crate::index::refresh(&policy, &b, None, crate::index::Mode::Incremental, None).unwrap();
    assert_eq!(navigation(&b), combined_navigation);
    assert_eq!(navigation(&a), donor_navigation);
    assert_eq!(navigation(&repo), home_navigation);
    assert_eq!(git(&a, &["rev-parse", "HEAD"]), a_sha);
    // Original request still recovers an accepted combined HEAD after a lost receipt.
    let retry = save(&policy, &repo, &b, &b_sha, &base);
    assert_eq!(retry.status, "current", "{retry:?}");
    assert!(!retry.published);
    assert_eq!(retry.candidate, second.candidate);
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), home_after_first); // wrapper owns home advancement
}
#[test]
fn normal_home_submission_preserves_body_only_navigation_and_retained_draft() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path().canonicalize().unwrap();
    let home = dir.join("home");
    seed(&home);
    let policy = Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    let allocate = || {
        let r = crate::local::worktree(&policy, &home).unwrap();
        PathBuf::from(r["worktree"].as_str().unwrap())
    };
    let a = allocate();
    let b = allocate();
    let retained = allocate();
    let base = git(&home, &["rev-parse", "main"]);
    let retained_head = git(&retained, &["rev-parse", "HEAD"]);
    let a_sha = edit(&a, 1, "First");
    let b_sha = edit(&b, 2, "Second");
    for p in [&a, &b, &retained] {
        crate::index::refresh(&policy, p, None, crate::index::Mode::Full, None).unwrap();
    }
    let retained_navigation = navigation(&retained);
    let submit = |path: &Path, sha: &str| {
        let mut q = request(&home);
        q.draft = Some(path.into());
        q.commit = Some(sha.into());
        q.base = Some(base.clone());
        let r = crate::local::submit(&policy, &q).unwrap();
        assert_eq!(r["status"], "current", "{r}");
        r
    };
    let first = submit(&a, &a_sha);
    assert!(!a.exists());
    let before = navigation(&home);
    let second = submit(&b, &b_sha);
    assert!(!b.exists());
    assert_ne!(second["version"], first["version"]);
    assert_eq!(second["validation"]["status"], "current");
    let checks = second["graph_checks"].as_array().unwrap();
    assert_eq!(checks.len(), 1); // One actual validation, retained through the final recheck.
    for check in checks {
        assert_eq!(check["mode"], "preserved");
        assert_eq!(check["base_version"], first["version"]);
        assert_eq!(check["candidate_version"], second["version"]);
    }
    assert_eq!(second["accepted_index_available"], true);
    assert_eq!(second["draft_index_available"], true);
    for key in ["accepted_main", "prepared_worktree"] {
        assert_eq!(second["index_reuse"][key], 1);
    }
    assert_eq!(second["index_reuse"]["computed"], 0);
    let after = navigation(&home);
    // Cross-publication bytes/costs must agree; cross-view inode reuse is not required.
    assert_eq!(navigation_bytes(&after), navigation_bytes(&before));
    let sha = second["candidate"].as_str().unwrap();
    assert_eq!(git(&home, &["rev-parse", "HEAD"]), sha);
    assert_eq!(git(&home, &["rev-parse", "main"]), sha);
    for (n, body) in [(1, "First"), (2, "Second")] {
        assert!(
            fs::read_to_string(home.join(format!("nodes/60000000-0000-4000-8000-{n:012}/node.md")))
                .unwrap()
                .contains(body)
        );
    }
    assert_eq!(
        crate::outlines::inspect(&policy, &home, None)
            .unwrap()
            .version
            .as_deref(),
        second["version"].as_str()
    );
    crate::index::refresh(&policy, &home, None, crate::index::Mode::Incremental, None).unwrap();
    assert_eq!(navigation(&home), after);
    assert_eq!(navigation(&retained), retained_navigation);
    assert_eq!(git(&retained, &["rev-parse", "HEAD"]), retained_head);
    assert_eq!(git(&retained, &["status", "--porcelain"]), "");
}

#[test]
fn journaled_preparation_resumes_but_unrecorded_changed_head_refuses() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("home");
    seed(&repo);
    let base = git(&repo, &["rev-parse", "main"]);
    let a = draft(&repo, tmp.path(), "a", &base);
    let b = draft(&repo, tmp.path(), "b", &base);
    let a_sha = edit(&a, 1, "First");
    let b_sha = edit(&b, 2, "Second");
    let policy = Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    assert_eq!(save(&policy, &repo, &a, &a_sha, &base).status, "current");
    let mut q = request(&repo);
    q.draft = Some(b.clone());
    q.commit = Some(b_sha.clone());
    q.base = Some(base);
    let ready = tmp.path().join("occupied-ready");
    fs::write(&ready, "occupied").unwrap(); // deterministic refusal after preparation, before CAS
    q.pause_before_publish = Some(checked_publication::Barrier {
        ready,
        release: tmp.path().join("release"),
    });
    let mut heads = None;
    let refused = checked_publication::save_prepared(&policy, &q, None, |from, to| {
        assert_eq!(
            git(&b, &["rev-parse", "HEAD"]),
            from,
            "journal must precede HEAD movement"
        );
        heads = Some((from.to_owned(), to.to_owned()));
        Ok(())
    });
    assert_eq!(refused.status, "rejected", "{refused:?}");
    assert!(!refused.published);
    assert_eq!(git(&repo, &["rev-parse", "main"]), a_sha);
    let (from, to) = heads.unwrap();
    assert_eq!(from, b_sha);
    assert_eq!(git(&b, &["rev-parse", "HEAD"]), to);
    q.pause_before_publish = None;
    assert_eq!(checked_publication::save(&policy, &q).status, "rejected");
    let resumed =
        checked_publication::save_prepared(&policy, &q, Some((&from, &to)), |_, _| Ok(()));
    assert_eq!(resumed.status, "current", "{resumed:?}");
    assert_eq!(resumed.candidate.as_deref(), Some(to.as_str()));
    assert_eq!(git(&repo, &["rev-parse", "main"]), to);
}
#[test]
fn save_retains_actual_conflict_and_rejects_unaccepted_changed_head() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("home");
    seed(&repo);
    let base = git(&repo, &["rev-parse", "main"]);
    let a = draft(&repo, tmp.path(), "a", &base);
    let b = draft(&repo, tmp.path(), "b", &base);
    let a_sha = edit(&a, 1, "First");
    let b_sha = edit(&b, 1, "Second");
    let policy = Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    assert_eq!(save(&policy, &repo, &a, &a_sha, &base).status, "current");
    let refused = save(&policy, &repo, &b, &b_sha, &base);
    assert_eq!(refused.status, "rejected");
    assert_eq!(refused.check, "git_file_conflict");
    assert!(!refused.published);
    assert_eq!(git(&repo, &["rev-parse", "main"]), a_sha);
    assert!(!git(&b, &["diff", "--name-only", "--diff-filter=U"]).is_empty());
    let file = b.join("nodes/60000000-0000-4000-8000-000000000001/node.md");
    fs::write(
        &file,
        fs::read(a.join("nodes/60000000-0000-4000-8000-000000000001/node.md")).unwrap(),
    )
    .unwrap();
    git(&b, &["add", "."]);
    git(&b, &["commit", "-qm", "knowledge: resolve conflict"]);
    assert_eq!(save(&policy, &repo, &b, &b_sha, &base).status, "rejected");
    let repaired = git(&b, &["rev-parse", "HEAD"]);
    let accepted = save(&policy, &repo, &b, &repaired, &base);
    assert_eq!(accepted.status, "current", "{accepted:?}");
    assert_eq!(accepted.candidate.as_deref(), Some(repaired.as_str()));
}
