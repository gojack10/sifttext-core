use super::*;
use std::{
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf, String, Notes) {
    let t = tempfile::tempdir().unwrap();
    let repo = fs::canonicalize(t.path()).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]).unwrap();
    fs::write(repo.join(".gitignore"), ".sifttext/\n*.lean\n*.meta.json\n").unwrap();
    git(&repo, &["add", "."]).unwrap();
    git(&repo, &["commit", "-qm", "owned fixture"]).unwrap();
    let head = git(&repo, &["rev-parse", "HEAD"]).unwrap();
    git(&repo, &["checkout", "--detach", "-q"]).unwrap();
    let root = repo.join(".git/sifttext-publications");
    fs::create_dir(&root).unwrap();
    (t, repo, root, head, Rc::new(RefCell::new(Vec::new())))
}
fn wait(path: &Path) {
    let start = Instant::now();
    while !path.exists() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "missing {}",
            path.display()
        );
        thread::sleep(Duration::from_millis(5));
    }
}
#[test]
fn success_and_locked_partial_initialization_use_coupled_native_finish() {
    let (_t, repo, root, head, notes) = fixture();
    let owned = WorkingView::new(&repo, &root, &head, &notes).unwrap();
    let parent = owned.parent.clone();
    drop(owned);
    assert!(!parent.exists());
    finish(&repo, &parent, false, false).unwrap();
    assert!(
        !git(&repo, &["worktree", "list", "--porcelain"])
            .unwrap()
            .contains("submission-")
    );
    for absent in [false, true] {
        let error = WorkingView::new_using(&repo, &root, &head, &notes, |view| {
            git(
                &repo,
                &[
                    "worktree",
                    "add",
                    "--detach",
                    "--no-checkout",
                    view.to_str().unwrap(),
                    &head,
                ],
            )?;
            git(
                &repo,
                &[
                    "worktree",
                    "lock",
                    "--reason",
                    "initializing",
                    view.to_str().unwrap(),
                ],
            )?;
            if absent {
                fs::remove_dir_all(view)?;
            }
            git(
                &repo,
                &[
                    "rev-parse",
                    "--verify",
                    "refs/heads/nonexistent-owned-fixture",
                ],
            )
            .map(|_| ())
        })
        .err()
        .unwrap();
        assert!(error.to_string().contains("Git operation rejected"));
        assert_eq!(notes.borrow().last().unwrap().kind, "git_nonzero");
        assert_eq!(notes.borrow().last().unwrap().cleanup, "removed");
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
    }
}
#[test]
fn retained_unknown_mismatch_reused_branch_and_user_file_are_preserved() {
    let (_t, repo, root, head, notes) = fixture();
    let retained = WorkingView::new(&repo, &root, &head, &notes)
        .unwrap()
        .retain(&head)
        .unwrap();
    let unknown = root.join("submission-unknown");
    fs::create_dir(&unknown).unwrap();
    fs::write(unknown.join("private"), "keep").unwrap();
    for mode in [
        "identity",
        "repository",
        "view-identity",
        "reused",
        "branch",
        "user",
        "lock",
        "format",
    ] {
        let mut owned = WorkingView::new(&repo, &root, &head, &notes).unwrap();
        let parent = owned.parent.clone();
        let view = owned.path.clone();
        owned.retained = true;
        drop(owned); // simulate interruption after releasing original custody
        match mode {
            "identity" => {
                let mut r = load(&repo, &parent).unwrap();
                r.parent.ino += 1;
                persist(&parent, &r).unwrap();
            }
            "repository" => {
                let mut r = load(&repo, &parent).unwrap();
                r.repository.ino += 1;
                persist(&parent, &r).unwrap();
            }
            "view-identity" => {
                let mut r = load(&repo, &parent).unwrap();
                r.view.ino += 1;
                persist(&parent, &r).unwrap();
            }
            "format" => {
                fs::write(parent.join("owner.json"), "{\"version\":999}").unwrap();
            }
            "reused" => {
                fs::rename(&view, parent.join("old-view")).unwrap();
                fs::create_dir(&view).unwrap();
                fs::write(view.join("private"), "keep").unwrap();
            }
            "branch" => {
                git(&view, &["checkout", "-b", "acquired-branch"]).unwrap();
            }
            "lock" => {
                git(
                    &repo,
                    &[
                        "worktree",
                        "lock",
                        "--reason",
                        "user protection",
                        view.to_str().unwrap(),
                    ],
                )
                .unwrap();
            }
            "user" => {
                fs::write(view.join("private"), "keep").unwrap();
            }
            _ => unreachable!(),
        }
        recover(&repo, &root, &notes).unwrap();
        assert!(view.exists(), "{mode}");
        assert!(parent.join("owner.json").exists());
    }
    recover(&repo, &root, &notes).unwrap();
    assert!(retained.exists());
    assert_eq!(fs::read(unknown.join("private")).unwrap(), b"keep");
}
#[test]
fn removal_failure_does_not_fall_through_to_recursive_tempdir_deletion() {
    let (_t, repo, root, head, notes) = fixture();
    let owned = WorkingView::new(&repo, &root, &head, &notes).unwrap();
    let path = owned.path.clone();
    git(
        &repo,
        &[
            "worktree",
            "lock",
            "--reason",
            "user",
            path.to_str().unwrap(),
        ],
    )
    .unwrap();
    drop(owned);
    assert!(path.exists());
    assert!(path.parent().unwrap().join("owner.json").exists());
    assert_eq!(
        notes.borrow().last().unwrap().cleanup,
        "preserved_unsafe_or_incomplete"
    );
}
#[test]
fn owned_crash_worker() {
    let Some(repo) = std::env::var_os("SIFTTEXT_OWNED_TEST_REPO") else {
        return;
    };
    let repo = PathBuf::from(repo);
    let root = repo.join(".git/sifttext-publications");
    let head = git(&repo, &["rev-parse", "HEAD"]).unwrap();
    let draft = repo.join("submitted-draft");
    let lease = crate::local::DraftLease::acquire(&repo, &draft).unwrap();
    let _custody = lease.custody().unwrap();
    let notes = Rc::new(RefCell::new(Vec::new()));
    let _ = WorkingView::new_using(&repo, &root, &head, &notes, |view| {
        git(
            &repo,
            &[
                "worktree",
                "add",
                "--detach",
                "--no-checkout",
                view.to_str().unwrap(),
                &head,
            ],
        )?;
        fs::write(repo.join("owned-view"), view.as_os_str().as_encoded_bytes())?;
        let script = format!(
            "sh -c 'echo ready > {0}/owned-ready; n=0; while [ ! -f {0}/owned-release ] && [ $n -lt 500 ]; do sleep 0.01; n=$((n+1)); done; git -C {0}/submitted-draft reset --hard HEAD >/dev/null; printf \".sifttext/\\n*.lean\\n*.meta.json\\n\" > {1}/.gitignore; echo done > {0}/owned-done' & wait",
            repo.display(),
            view.display()
        );
        // Real Git's shell-alias dispatcher supplies an actual Git child plus
        // ordinary child/grandchild for FD inheritance proof (test fixture only).
        let mut cmd = Command::new("git");
        cmd.arg("-C").arg(&repo).args([
            "-c",
            &format!("alias.lifetime-proof=!{script}"),
            "lifetime-proof",
        ]);
        super::super::run_command(cmd)?;
        Ok(())
    });
    panic!("worker should be killed");
}
#[test]
fn core_sigkill_busy_skips_independent_publication_then_recovers_after_real_grandchild() {
    let (_t, repo, root, head, notes) = fixture();
    let source = fs::read(repo.join(".gitignore")).unwrap();
    let draft = repo.join("submitted-draft");
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-b",
            "sifttext/lifetime-fixture",
            draft.to_str().unwrap(),
            &head,
        ],
    )
    .unwrap();
    let mut worker = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "checked_publication::owned::tests::owned_crash_worker",
            "--nocapture",
        ])
        .env("SIFTTEXT_OWNED_TEST_REPO", &repo)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    wait(&repo.join("owned-ready"));
    let view = PathBuf::from(fs::read_to_string(repo.join("owned-view")).unwrap());
    worker.kill().unwrap();
    assert!(!worker.wait().unwrap().success());
    assert!(
        crate::local::DraftLease::acquire(&repo, &draft).is_err(),
        "orphan Git descendants must retain submitted-draft custody too"
    );
    recover(&repo, &root, &notes).unwrap();
    assert!(view.exists());
    assert_eq!(notes.borrow().last().unwrap().cleanup, "busy");
    // No global gate: a different view can be fully set up and retired while
    // the orphan grandchild retains the first path's inherited lifecycle gate.
    let policy = crate::Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    let request: super::super::Request =
        serde_json::from_value(serde_json::json!({"repo":repo,"accepted_ref":"refs/heads/main"}))
            .unwrap();
    let start = Instant::now();
    let independent = super::super::submit(&policy, &request);
    assert_eq!(independent.status, "current", "{independent:?}");
    assert!(start.elapsed() < Duration::from_secs(5));
    assert!(view.exists());
    fs::write(repo.join("owned-release"), "").unwrap();
    wait(&repo.join("owned-done"));
    assert_eq!(fs::read(view.join(".gitignore")).unwrap(), source);
    let start = Instant::now();
    loop {
        if let Ok(gate) = crate::local::publication_gate(&view) {
            drop(gate);
            break;
        }
        assert!(start.elapsed() < Duration::from_secs(5));
        thread::sleep(Duration::from_millis(5));
    }
    let recovered = super::super::submit(&policy, &request);
    assert_eq!(recovered.status, "current", "{recovered:?}");
    assert!(!view.exists());
    assert!(
        independent.view.as_ref().unwrap().exists(),
        "next operation must not sweep retained reader"
    );
    assert!(!view.parent().unwrap().exists());
    assert_eq!(git(&repo, &["rev-parse", "HEAD"]).unwrap(), head);
    assert_eq!(git(&repo, &["rev-parse", "main"]).unwrap(), head);
    assert_eq!(fs::read(repo.join(".gitignore")).unwrap(), source);
    let _released = crate::local::DraftLease::acquire(&repo, &draft).unwrap();
    assert_eq!(git(&draft, &["rev-parse", "HEAD"]).unwrap(), head);
    assert_eq!(fs::read(draft.join(".gitignore")).unwrap(), source);
    assert_eq!(git(&repo, &["rev-list", "--count", "main"]).unwrap(), "1");
}
