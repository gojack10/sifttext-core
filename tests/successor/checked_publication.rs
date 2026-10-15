use crate::{
    Policy,
    checked_publication::{self, Request},
};
use serde_json::json;
use std::{collections::BTreeSet, fs, path::Path, process::Command};
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
    q.base = Some(base);
    let published = checked_publication::submit(&policy, &q);
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
