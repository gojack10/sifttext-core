use crate::{
    Policy,
    checked_publication::{self, Request},
};
use serde_json::json;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicBool, Ordering},
    thread,
    time::{Duration, Instant},
};
fn git(root: &Path, args: &[&str]) -> String {
    let result = Command::new("git")
        .arg("-C")
        .arg(root)
        .args([
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    String::from_utf8(result.stdout).unwrap().trim().into()
}
#[test]
fn publication_empty_git_baseline_recovers_then_accepts_first_real_root() {
    let t = tempfile::tempdir().unwrap();
    let repo = t.path().join("repo");
    fs::create_dir(&repo).unwrap();
    fs::write(repo.join(".gitignore"), ".sifttext/\n*.lean\n*.meta.json\n").unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "empty baseline"]);
    let base = git(&repo, &["rev-parse", "HEAD"]);
    git(&repo, &["checkout", "--detach", "-q", &base]);
    let policy = Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    let empty = checked_publication::submit(&policy, &request(&repo));
    assert_eq!(empty.status, "current", "{empty:?}");
    assert_eq!(empty.candidate.as_ref(), Some(&base));
    assert!(
        crate::Store::read(empty.view.as_ref().unwrap())
            .unwrap()
            .nodes
            .is_empty()
    );
    assert!(!repo.join("nodes").exists());
    assert!(!repo.join("trees").exists());

    let draft = t.path().join("draft");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "first",
            draft.to_str().unwrap(),
            &base,
        ],
    );
    let id = "60000000-0000-4000-8000-000000000001";
    let node = draft.join(format!("nodes/{id}"));
    fs::create_dir_all(node.join("children")).unwrap();
    fs::write(node.join("children/.keep"), "").unwrap();
    fs::write(node.join("children.order"), "").unwrap();
    fs::create_dir(draft.join("trees")).unwrap();
    std::os::unix::fs::symlink(format!("../nodes/{id}"), draft.join("trees").join(id)).unwrap();
    let source = format!(
        "---\nid: \"{id}\"\nname: \"First\"\nmicro_summary: \"Summary\"\nstatus: \"in_progress\"\nsee_also: []\n---\nReal authored root"
    );
    fs::write(node.join("node.md"), &source).unwrap();
    git(&draft, &["add", "nodes", "trees"]);
    git(&draft, &["commit", "-qm", "first real root"]);
    let sha = git(&draft, &["rev-parse", "HEAD"]);
    let mut q = request(&repo);
    q.draft = Some(draft.clone());
    q.commit = Some(sha.clone());
    q.base = Some(base.clone());
    let rules = b"review rules v1";
    for (candidate, rules_digest, passed) in [
        (None, crate::sha256(rules), false),
        (
            Some(String::from("stale-candidate")),
            crate::sha256(rules),
            true,
        ),
        (None, crate::sha256(b"stale rules"), true),
    ] {
        let rejected = checked_publication::submit_reviewed(
            &policy,
            &q,
            checked_publication::ReviewMode::Hard,
            rules,
            |actual, diff, message, _rules| {
                assert_ne!(
                    actual, sha,
                    "review must bind the combined candidate, not draft SHA"
                );
                assert!(String::from_utf8_lossy(diff).contains("Real authored root"));
                assert!(String::from_utf8_lossy(message).contains("first real root"));
                Ok(checked_publication::SemanticVerdict {
                    candidate: candidate.as_deref().unwrap_or(actual).to_owned(),
                    rules: rules_digest.clone(),
                    passed,
                    findings: vec![],
                })
            },
        );
        assert_eq!(rejected.status, "rejected", "{rejected:?}");
        assert!(!rejected.published);
        assert_eq!(git(&repo, &["rev-parse", "main"]), base);
    }

    let published = checked_publication::submit_reviewed(
        &policy,
        &q,
        checked_publication::ReviewMode::Hard,
        rules,
        |candidate, diff, message, rules| {
            assert_ne!(
                candidate, sha,
                "review must bind the combined candidate, not draft SHA"
            );
            assert!(String::from_utf8_lossy(diff).contains("Real authored root"));
            assert!(String::from_utf8_lossy(message).contains("first real root"));
            Ok(checked_publication::SemanticVerdict {
                candidate: candidate.to_owned(),
                rules: crate::sha256(rules),
                passed: true,
                findings: vec![],
            })
        },
    );
    assert_eq!(published.status, "current", "{published:?}");
    assert!(published.published);
    assert_eq!(
        git(&repo, &["rev-parse", "main"]),
        *published.candidate.as_ref().unwrap()
    );
    assert_eq!(
        published.validation.as_ref().unwrap().before_version,
        empty.version
    );
    let view = published.view.as_ref().unwrap();
    assert_eq!(
        fs::read_to_string(view.join(format!("nodes/{id}/node.md"))).unwrap(),
        source
    );
    assert_eq!(
        crate::outlines::inspect(&policy, view, None)
            .unwrap()
            .version,
        published.version
    );
    assert_eq!(git(&draft, &["rev-parse", "HEAD"]), sha);
    assert_eq!(git(&draft, &["status", "--porcelain"]), "");
    assert!(!repo.join("nodes").exists());
    let retry = checked_publication::submit(&policy, &q);
    assert_eq!(retry.status, "current", "{retry:?}");
    assert_eq!(retry.candidate, published.candidate);
}

#[test]
fn invalid_candidate_never_invokes_semantic_reviewer() {
    let temp = tempfile::tempdir().unwrap();
    let (policy, repo, draft, q, base, sha, _) = review_fixture(temp.path(), "not-a-status");
    let called = AtomicBool::new(false);
    let rejected = checked_publication::submit_reviewed(
        &policy,
        &q,
        checked_publication::ReviewMode::Hard,
        b"rules v1",
        |candidate, _, _message, rules| {
            called.store(true, Ordering::SeqCst);
            Ok(checked_publication::SemanticVerdict {
                candidate: candidate.into(),
                rules: crate::sha256(rules),
                passed: true,
                findings: vec![],
            })
        },
    );
    assert_eq!(rejected.status, "rejected", "{rejected:?}");
    assert!(!rejected.published);
    assert!(!called.load(Ordering::SeqCst));
    assert_eq!(git(&repo, &["rev-parse", "main"]), base);
    assert_eq!(git(&draft, &["rev-parse", "HEAD"]), sha);
    assert_eq!(git(&draft, &["status", "--porcelain"]), "");
}

#[test]
fn candidate_mutated_after_passing_review_is_refused_and_draft_recovers() {
    let temp = tempfile::tempdir().unwrap();
    let (policy, repo, draft, mut q, base, sha, source) =
        review_fixture(temp.path(), "in_progress");
    let ready = temp.path().join("review-ready");
    let release = temp.path().join("review-release");
    q.pause_before_publish = Some(checked_publication::Barrier {
        ready: ready.clone(),
        release: release.clone(),
    });
    let reviewed = AtomicBool::new(false);
    let (rejected, view) = thread::scope(|scope| {
        let worker = scope.spawn(|| {
            checked_publication::submit_reviewed(
                &policy,
                &q,
                checked_publication::ReviewMode::Hard,
                b"rules v1",
                |candidate, _, _message, rules| {
                    reviewed.store(true, Ordering::SeqCst);
                    Ok(checked_publication::SemanticVerdict {
                        candidate: candidate.into(),
                        rules: crate::sha256(rules),
                        passed: true,
                        findings: vec![],
                    })
                },
            )
        });
        let start = Instant::now();
        while !ready.exists() {
            assert!(
                !worker.is_finished(),
                "publication exited before the final gate"
            );
            assert!(
                start.elapsed() < Duration::from_secs(30),
                "publication barrier timed out"
            );
            thread::sleep(Duration::from_millis(5));
        }
        let view = PathBuf::from(fs::read_to_string(&ready).unwrap());
        let candidate = view.join(format!("nodes/{REVIEW_NODE_ID}/node.md"));
        fs::write(&candidate, "mutated after semantic review").unwrap();
        fs::write(&release, "continue").unwrap();
        (worker.join().unwrap(), view)
    });
    assert!(reviewed.load(Ordering::SeqCst));
    assert_eq!(rejected.status, "rejected", "{rejected:?}");
    assert_eq!(rejected.check, "source_or_ref_changed", "{rejected:?}");
    assert!(!rejected.published);
    assert_eq!(git(&repo, &["rev-parse", "main"]), base);
    assert!(draft.exists());
    assert_eq!(git(&draft, &["rev-parse", "HEAD"]), sha);
    assert_eq!(git(&draft, &["status", "--porcelain"]), "");
    assert_eq!(
        fs::read_to_string(draft.join(format!("nodes/{REVIEW_NODE_ID}/node.md"))).unwrap(),
        source
    );
    assert!(view.exists(), "unexpected edited scratch must be preserved");
    assert!(
        rejected
            .lifecycle
            .iter()
            .any(|note| note.cleanup == "preserved_unsafe_or_incomplete")
    );

    q.pause_before_publish = None;
    let recovered = checked_publication::submit_reviewed(
        &policy,
        &q,
        checked_publication::ReviewMode::Hard,
        b"rules v1",
        |candidate, _, _message, rules| {
            Ok(checked_publication::SemanticVerdict {
                candidate: candidate.into(),
                rules: crate::sha256(rules),
                passed: true,
                findings: vec![],
            })
        },
    );
    assert_eq!(recovered.status, "current", "{recovered:?}");
    assert!(recovered.published);
    assert_eq!(
        git(&repo, &["rev-parse", "main"]),
        recovered.candidate.unwrap()
    );
}

const REVIEW_NODE_ID: &str = "60000000-0000-4000-8000-000000000001";

fn review_fixture(
    root: &Path,
    status: &str,
) -> (Policy, PathBuf, PathBuf, Request, String, String, String) {
    let repo = root.join("repo");
    fs::create_dir(&repo).unwrap();
    fs::write(repo.join(".gitignore"), ".sifttext/\n*.lean\n*.meta.json\n").unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "empty baseline"]);
    let base = git(&repo, &["rev-parse", "HEAD"]);
    git(&repo, &["checkout", "--detach", "-q", &base]);
    let draft = root.join("draft");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "review",
            draft.to_str().unwrap(),
            &base,
        ],
    );
    let node = draft.join(format!("nodes/{REVIEW_NODE_ID}"));
    fs::create_dir_all(node.join("children")).unwrap();
    fs::write(node.join("children/.keep"), "").unwrap();
    fs::write(node.join("children.order"), "").unwrap();
    fs::create_dir(draft.join("trees")).unwrap();
    std::os::unix::fs::symlink(
        format!("../nodes/{REVIEW_NODE_ID}"),
        draft.join("trees").join(REVIEW_NODE_ID),
    )
    .unwrap();
    let source = format!(
        "---\nid: \"{REVIEW_NODE_ID}\"\nname: \"First\"\nmicro_summary: \"Summary\"\nstatus: \"{status}\"\nsee_also: []\n---\nReal authored root"
    );
    fs::write(node.join("node.md"), &source).unwrap();
    git(&draft, &["add", "nodes", "trees"]);
    git(&draft, &["commit", "-qm", "first root"]);
    let sha = git(&draft, &["rev-parse", "HEAD"]);
    let mut q = request(&repo);
    q.draft = Some(draft.clone());
    q.commit = Some(sha.clone());
    q.base = Some(base.clone());
    (
        Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap(),
        repo,
        draft,
        q,
        base,
        sha,
        source,
    )
}

#[path = "single_save.rs"]
mod single_save;

fn request(repo: &Path) -> Request {
    Request {
        repo: repo.into(),
        accepted_ref: "refs/heads/main".into(),
        draft: None,
        commit: None,
        base: None,
        explicit: BTreeSet::new(),
        pause_before_publish: None,
        pause_after_publish: None,
    }
}
#[test]
fn publication_library_rejects_checked_out_main_and_recovers_a_detached_reader() {
    let t = tempfile::tempdir().unwrap();
    let p = t.path();
    let id = "60000000-0000-4000-8000-000000000001";
    fs::create_dir_all(p.join(format!("nodes/{id}/children"))).unwrap();
    fs::create_dir(p.join("trees")).unwrap();
    fs::write(p.join(format!("nodes/{id}/children/.keep")), "").unwrap();
    fs::write(p.join(format!("nodes/{id}/children.order")), "").unwrap();
    let fields = json!({"id":id,"name":"Node","micro_summary":"Summary","status":"in_progress","see_also":[]});
    let body = "---\n".to_owned()
        + &fields
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| format!("{k}: {v}\n"))
            .collect::<String>()
        + "---\nBody";
    fs::write(p.join(format!("nodes/{id}/node.md")), body).unwrap();
    fs::write(p.join(".gitignore"), ".sifttext/\n*.lean\n*.meta.json\n").unwrap();
    std::os::unix::fs::symlink(format!("../nodes/{id}"), p.join("trees").join(id)).unwrap();
    git(p, &["init", "-q", "-b", "main"]);
    git(p, &["add", "."]);
    git(p, &["commit", "-qm", "baseline"]);
    let sha = git(p, &["rev-parse", "HEAD"]);
    let policy = Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    let q = request(p);
    assert_eq!(checked_publication::submit(&policy, &q).status, "rejected");
    git(p, &["checkout", "--detach", "-q", &sha]);
    let r = checked_publication::submit(&policy, &q);
    assert_eq!(r.status, "current", "{r:?}");
    assert_eq!(r.candidate.as_ref(), Some(&sha));
    assert_eq!(git(p, &["rev-parse", "refs/heads/main"]), sha);
    assert_eq!(
        crate::outlines::inspect(&policy, r.view.as_ref().unwrap(), None)
            .unwrap()
            .version,
        r.version
    );
    let mut unsafe_request = request(p);
    unsafe_request.accepted_ref = "HEAD".into();
    assert_eq!(
        checked_publication::submit(&policy, &unsafe_request).status,
        "rejected"
    );
}
