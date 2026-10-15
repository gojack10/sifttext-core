//! Core same-draft checks. CLI tests own normal-home completion and cleanup.
use super::{git, request};
use crate::{Policy, checked_publication};
use std::{
    fs,
    path::{Path, PathBuf},
};

fn seed(repo: &Path) {
    fs::create_dir(repo).unwrap();
    fs::write(repo.join(".gitignore"), ".sifttext/\n*.lean\n*.meta.json\n").unwrap();
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
#[test]
fn save_preserves_fast_forward_and_checks_divergence_in_the_original_draft() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("home");
    seed(&repo);
    let base = git(&repo, &["rev-parse", "main"]);
    let a = draft(&repo, tmp.path(), "a", &base);
    let b = draft(&repo, tmp.path(), "b", &base);
    let a_sha = edit(&a, 1, "First");
    let b_sha = edit(&b, 2, "Second");
    let policy = Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    let first = save(&policy, &repo, &a, &a_sha, &base);
    assert_eq!(first.status, "current", "{first:?}");
    assert_eq!(first.candidate.as_deref(), Some(a_sha.as_str()));
    assert_eq!(first.view.as_deref(), Some(a.as_path()));
    assert_eq!(git(&b, &["rev-parse", "HEAD"]), b_sha);
    let second = save(&policy, &repo, &b, &b_sha, &base);
    assert_eq!(second.status, "current", "{second:?}");
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
    assert_eq!(git(&a, &["rev-parse", "HEAD"]), a_sha);
    // Original request still recovers an accepted combined HEAD after a lost receipt.
    let retry = save(&policy, &repo, &b, &b_sha, &base);
    assert_eq!(retry.status, "current", "{retry:?}");
    assert!(!retry.published);
    assert_eq!(retry.candidate, second.candidate);
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]), base); // wrapper owns home advancement
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
