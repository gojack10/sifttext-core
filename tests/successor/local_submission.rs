//! Deterministic post-CAS process death and post-removal cleanup recovery.
use super::submit;
use crate::{
    Policy,
    checked_publication::{Barrier, Request},
    local,
};
use serde_json::json;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
fn git(p: &Path, args: &[&str]) -> String {
    let mut cmd = Command::new("git");
    for (key, _) in std::env::vars_os() {
        if key.as_encoded_bytes().starts_with(b"GIT_") {
            cmd.env_remove(key);
        }
    }
    let out = cmd
        .arg("-C")
        .arg(p)
        .args([
            "-c",
            "user.name=Synthetic Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgSign=false",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    String::from_utf8(out.stdout).unwrap().trim().into()
}
fn fixture(dir: &Path) -> (Policy, PathBuf, Request, PathBuf) {
    let policy = Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    let home = dir.join("home");
    fs::create_dir(&home).unwrap();
    fs::create_dir(home.join("nodes")).unwrap();
    fs::create_dir(home.join("trees")).unwrap();
    fs::write(home.join("trees/.keep"), "").unwrap();
    fs::write(home.join(".gitignore"), super::super::IGNORE).unwrap();
    git(&home, &["init", "-q", "-b", "main"]);
    git(&home, &["add", "."]);
    git(&home, &["commit", "-qm", "knowledge: empty fixture"]);
    git(&home, &["checkout", "--detach", "-q"]);
    let a = local::worktree(&policy, &home).unwrap();
    let other = local::worktree(&policy, &home).unwrap();
    let draft = PathBuf::from(a["worktree"].as_str().unwrap());
    local::create(
        &policy,
        &draft,
        "Recoveryanchor",
        "Checks interrupted custody.",
        None,
    )
    .unwrap();
    git(&draft, &["add", "nodes", "trees"]);
    git(&draft, &["commit", "-qm", "knowledge: recover save"]);
    let q = Request {
        repo: home.clone(),
        accepted_ref: "refs/heads/main".into(),
        draft: Some(draft.clone()),
        commit: Some(git(&draft, &["rev-parse", "HEAD"])),
        base: Some(a["base"].as_str().unwrap().into()),
        explicit: BTreeSet::new(),
        pause_before_publish: None,
        pause_after_publish: None,
    };
    (
        policy,
        home,
        q,
        PathBuf::from(other["worktree"].as_str().unwrap()),
    )
}
#[test]
fn ignored_user_file_survives_submission_refusal() {
    for relative in [
        "private-note",
        ".sifttext/outlines/private-note.txt",
        ".sifttext/outline-stage/private-note.txt",
    ] {
        let temp = tempfile::tempdir().unwrap();
        let dir = temp.path().canonicalize().unwrap();
        let (policy, home, q, other) = fixture(&dir);
        let draft = q.draft.as_ref().unwrap();
        let before = git(&home, &["rev-parse", "main"]);
        let other_head = git(&other, &["rev-parse", "HEAD"]);
        // Repository-local excludes can hide user files from ordinary porcelain.
        let exclude = home.join(".git/info/exclude");
        fs::write(&exclude, "private-note\n").unwrap();
        let note = draft.join(relative);
        fs::create_dir_all(note.parent().unwrap()).unwrap();
        fs::write(&note, "uncommitted user data").unwrap();
        assert_eq!(
            git(draft, &["status", "--porcelain", "--untracked-files=all"]),
            ""
        );
        let result = submit(&policy, &q).unwrap();
        assert_eq!(result["status"], "rejected", "{result}");
        assert_eq!(git(&home, &["rev-parse", "main"]), before);
        assert_eq!(git(&home, &["rev-parse", "HEAD"]), before);
        assert_eq!(fs::read_to_string(note).unwrap(), "uncommitted user data");
        assert_eq!(
            git(draft, &["rev-parse", "HEAD"]),
            q.commit.as_ref().unwrap().as_str()
        );
        assert_eq!(git(&other, &["rev-parse", "HEAD"]), other_head);
    }
}

#[test]
fn rejected_combined_candidate_clears_custody_for_explicit_repair() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().canonicalize().unwrap();
    let (policy, home, mut q, other) = fixture(&dir);
    let draft = q.draft.as_ref().unwrap().clone();
    let retained = local::worktree(&policy, &home).unwrap();
    let retained = PathBuf::from(retained["worktree"].as_str().unwrap());
    let retained_head = git(&retained, &["rev-parse", "HEAD"]);
    let file = fs::read_dir(draft.join("nodes"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path()
        .join("node.md");
    let valid = fs::read_to_string(&file).unwrap();
    let invalid = valid.replace("status: \"not_examined\"", "status: \"invalid\"");
    assert_ne!(invalid, valid);
    fs::write(&file, invalid).unwrap();
    git(&draft, &["add", "nodes"]);
    git(
        &draft,
        &["commit", "-qm", "knowledge: invalid candidate control"],
    );
    q.commit = Some(git(&draft, &["rev-parse", "HEAD"]));
    local::create(
        &policy,
        &other,
        "Concurrentanchor",
        "Requires integration before rejection.",
        None,
    )
    .unwrap();
    git(&other, &["add", "nodes", "trees"]);
    git(&other, &["commit", "-qm", "knowledge: competing root"]);
    let competing: Request = serde_json::from_value(json!({
        "repo":home,"accepted_ref":q.accepted_ref,"draft":other,
        "commit":git(&other, &["rev-parse", "HEAD"]),"base":q.base,"explicit":[]
    }))
    .unwrap();
    assert_eq!(submit(&policy, &competing).unwrap()["status"], "current");
    let accepted = git(&home, &["rev-parse", "main"]);
    let refused = submit(&policy, &q).unwrap();
    assert_eq!(refused["status"], "rejected", "{refused}");
    assert_eq!(refused["check"], "candidate_validation_rejected");
    assert_eq!(refused["published"], false);
    assert_ne!(Some(git(&draft, &["rev-parse", "HEAD"])), q.commit);
    assert_eq!(git(&home, &["rev-parse", "main"]), accepted);
    assert_eq!(git(&home, &["rev-parse", "HEAD"]), accepted);
    assert_eq!(git(&draft, &["status", "--porcelain"]), "");
    let journal = home.join(".git/sifttext-saves").join(format!(
        "{:x}.json",
        <sha2::Sha256 as sha2::Digest>::digest(draft.as_os_str().as_encoded_bytes())
    ));
    assert!(
        !journal.exists(),
        "refusal must permit explicit repaired resubmission"
    );
    // The original request must not silently accept the changed unaccepted HEAD.
    assert_eq!(submit(&policy, &q).unwrap()["status"], "rejected");
    assert!(!journal.exists());
    fs::write(&file, &valid).unwrap();
    git(&draft, &["add", "nodes"]);
    git(
        &draft,
        &[
            "-c",
            "user.name=Selected Repair Author",
            "-c",
            "user.email=repair@example.invalid",
            "commit",
            "-qm",
            "knowledge: repair candidate\n\nCo-authored-by: SiftText Agent <agent@sifttext.com>",
        ],
    );
    let repaired = git(&draft, &["rev-parse", "HEAD"]);
    q.commit = Some(repaired.clone());
    let result = submit(&policy, &q).unwrap();
    assert_eq!(result["status"], "current", "{result}");
    assert_eq!(
        result["candidate"], repaired,
        "repaired fast-forward must reuse chosen credit"
    );
    assert_eq!(git(&home, &["rev-parse", "main"]), repaired);
    assert_eq!(git(&home, &["rev-parse", "HEAD"]), repaired);
    assert!(!draft.exists());
    assert_eq!(git(&retained, &["rev-parse", "HEAD"]), retained_head);
    assert_eq!(git(&retained, &["status", "--porcelain"]), "");
    assert_eq!(
        git(
            &home,
            &[
                "log",
                "-1",
                "--format=%an%x00%ae%x00%(trailers:key=Co-authored-by)"
            ]
        ),
        "Selected Repair Author\0repair@example.invalid\0Co-authored-by: SiftText Agent <agent@sifttext.com>"
    );
    assert_eq!(
        local::sql(&policy, &home, "SELECT name FROM nodes ORDER BY name").unwrap()["result"]["rows"],
        json!([["Concurrentanchor"], ["Recoveryanchor"]])
    );
    let outlines = crate::outlines::inspect(&policy, &home, None).unwrap();
    assert_eq!(outlines.status, "current");
    assert_eq!(outlines.version.as_deref(), result["version"].as_str());
}
struct PendingChild(Option<Child>);
impl Drop for PendingChild {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
#[test]
fn save_pause_worker() {
    let Some(path) = std::env::var_os("SIFTTEXT_SAVE_TEST_REQUEST") else {
        return;
    };
    let q: Request = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let policy = Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    // Parent kills this process at the real publisher barrier.
    let _ = submit(&policy, &q);
    panic!("paused worker unexpectedly settled");
}
#[test]
fn post_cas_sigkill_recovers_original_request_without_second_publication() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().canonicalize().unwrap();
    let (policy, home, q, other) = fixture(&dir);
    let before = git(&home, &["rev-parse", "HEAD"]);
    let other_head = git(&other, &["rev-parse", "HEAD"]);
    let ready = dir.join("ready");
    let release = dir.join("release");
    let path = dir.join("request.json");
    fs::write(&path, serde_json::to_vec(&json!({"repo":q.repo,"accepted_ref":q.accepted_ref,"draft":q.draft,"commit":q.commit,"base":q.base,"explicit":[],"pause_after_publish":{"ready":ready,"release":release}})).unwrap()).unwrap();
    let mut worker = PendingChild(Some(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "local::submission::tests::save_pause_worker",
                "--nocapture",
            ])
            .env("SIFTTEXT_SAVE_TEST_REQUEST", &path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    ));
    let started = Instant::now();
    while !ready.exists() {
        assert!(
            worker.0.as_mut().unwrap().try_wait().unwrap().is_none(),
            "worker exited before CAS barrier"
        );
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "publisher did not reach CAS barrier"
        );
        thread::sleep(Duration::from_millis(5));
    }
    let mut child = worker.0.take().unwrap();
    child.kill().unwrap();
    assert!(!child.wait().unwrap().success());
    let accepted = git(&home, &["rev-parse", "main"]);
    assert_ne!(accepted, before);
    assert_eq!(git(&home, &["rev-parse", "HEAD"]), before);
    assert!(q.draft.as_ref().unwrap().exists());
    let count = git(&home, &["rev-list", "--count", "main"]);
    // Recovery must preserve reader work even though acceptance already happened.
    // Reuse this interrupted specimen rather than repeat the expensive CAS setup.
    let reader_file = home.join("trees/.keep");
    let original = fs::read(&reader_file).unwrap();
    let note = home.join("uncommitted-reader-note.txt");
    for path in [&reader_file, &note] {
        fs::write(path, b"reader work must survive").unwrap();
        let error = submit(&policy, &q).unwrap_err().to_string();
        assert!(error.contains("reader is dirty"), "{error}");
        assert_eq!(fs::read(path).unwrap(), b"reader work must survive");
        assert_eq!(git(&home, &["rev-parse", "HEAD"]), before);
        assert_eq!(git(&home, &["rev-parse", "main"]), accepted);
        assert_eq!(git(&home, &["rev-list", "--count", "main"]), count);
        assert!(q.draft.as_ref().unwrap().exists());
        assert_eq!(git(&other, &["rev-parse", "HEAD"]), other_head);
        if path == &reader_file {
            fs::write(path, &original).unwrap();
        } else {
            fs::remove_file(path).unwrap();
        }
    }
    // The stopped operation does not own new user files in its retained draft.
    // Reuse the killed specimen to check recovery custody without another CAS.
    let draft = q.draft.as_ref().unwrap();
    for relative in [
        ".sifttext/outlines/private-note.txt",
        ".sifttext/outline-stage/private-note.txt",
    ] {
        let note = draft.join(relative);
        fs::create_dir_all(note.parent().unwrap()).unwrap();
        fs::write(&note, b"interrupted draft work must survive").unwrap();
        assert_eq!(git(draft, &["status", "--porcelain"]), "");
        let refused = submit(&policy, &q).unwrap();
        assert_eq!(refused["status"], "rejected", "{refused}");
        assert_eq!(refused["published"], false);
        assert_eq!(
            fs::read(&note).unwrap(),
            b"interrupted draft work must survive"
        );
        assert_eq!(git(draft, &["rev-parse", "HEAD"]), accepted);
        assert_eq!(git(&home, &["rev-parse", "HEAD"]), before);
        assert_eq!(git(&home, &["rev-parse", "main"]), accepted);
        assert_eq!(git(&home, &["rev-list", "--count", "main"]), count);
        assert_eq!(git(&other, &["rev-parse", "HEAD"]), other_head);
        fs::remove_file(note).unwrap();
    }
    // Reserved output is disposable only while untracked. During stopped
    // recovery, staged bytes still belong to the user even without a work file.
    let relative = ".sifttext/outlines/00000000-0000-0000-0000-000000000001/outline.lean";
    let indexed = draft.join(relative);
    fs::create_dir_all(indexed.parent().unwrap()).unwrap();
    fs::write(&indexed, b"staged draft work must survive").unwrap();
    git(draft, &["add", "-f", "--", relative]);
    for working_copy in [true, false] {
        if !working_copy {
            fs::remove_file(&indexed).unwrap();
        }
        let refused = submit(&policy, &q).unwrap();
        assert_eq!(refused["status"], "rejected", "{refused}");
        assert_eq!(refused["published"], false);
        assert_eq!(
            git(draft, &["show", &format!(":{relative}")]),
            "staged draft work must survive"
        );
        assert_eq!(refused["check"], "unsafe_submission", "{refused}");
        assert_eq!(indexed.exists(), working_copy);
        if working_copy {
            assert_eq!(
                fs::read(&indexed).unwrap(),
                b"staged draft work must survive"
            );
        }
        assert_eq!(git(draft, &["rev-parse", "HEAD"]), accepted);
        assert_eq!(git(&home, &["rev-parse", "HEAD"]), before);
        assert_eq!(git(&home, &["rev-parse", "main"]), accepted);
        assert_eq!(git(&home, &["rev-list", "--count", "main"]), count);
        assert_eq!(git(&other, &["rev-parse", "HEAD"]), other_head);
    }
    git(draft, &["reset", "-q", "HEAD", "--", relative]);
    let result = submit(&policy, &q).unwrap();
    assert_eq!(result["status"], "current");
    assert_eq!(result["published"], false);
    assert_eq!(result["candidate"], accepted);
    assert_eq!(git(&home, &["rev-list", "--count", "main"]), count);
    assert_eq!(git(&home, &["rev-parse", "HEAD"]), accepted);
    assert!(!q.draft.as_ref().unwrap().exists());
    assert_eq!(git(&other, &["rev-parse", "HEAD"]), other_head);
    assert_eq!(
        local::sql(&policy, &home, "SELECT count(*) FROM nodes").unwrap()["result"]["rows"][0][0],
        1
    );
}
// The durable final state is identical whether success was delivered or the
// process died after finalization but before the caller received its receipt.
#[test]
fn completed_save_retry_recovers_lost_success_receipt() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().canonicalize().unwrap();
    let (policy, home, q, other) = fixture(&dir);
    let other_head = git(&other, &["rev-parse", "HEAD"]);
    let branch = git(
        q.draft.as_ref().unwrap(),
        &["symbolic-ref", "--quiet", "HEAD"],
    );
    let lost = super::submit_inner(&policy, &q, || Err("lost success receipt".into()));
    assert!(
        lost.unwrap_err()
            .to_string()
            .contains("lost success receipt")
    );
    assert!(!q.draft.as_ref().unwrap().exists());
    let accepted = git(&home, &["rev-parse", "main"]);
    let count = git(&home, &["rev-list", "--count", "main"]);
    // A completed journal does not own a branch acquired by another worktree,
    // even when that branch still points at the exact completed commit.
    let acquired = dir.join("reacquired-branch");
    git(
        &home,
        &[
            "worktree",
            "add",
            "-b",
            branch.strip_prefix("refs/heads/").unwrap(),
            acquired.to_str().unwrap(),
            &accepted,
        ],
    );
    let note = acquired.join("user-note.txt");
    fs::write(&note, b"new worktree custody").unwrap();
    let refused = submit(&policy, &q).unwrap_err().to_string();
    assert!(
        refused.contains("cleanup branch acquired by another worktree"),
        "{refused}"
    );
    assert_eq!(fs::read(&note).unwrap(), b"new worktree custody");
    assert_eq!(git(&acquired, &["symbolic-ref", "--quiet", "HEAD"]), branch);
    assert_eq!(git(&acquired, &["rev-parse", "HEAD"]), accepted);
    assert_eq!(git(&home, &["rev-parse", "main"]), accepted);
    assert_eq!(git(&home, &["rev-parse", "HEAD"]), accepted);
    assert_eq!(git(&home, &["rev-list", "--count", "main"]), count);
    assert_eq!(git(&other, &["rev-parse", "HEAD"]), other_head);
    fs::remove_file(note).unwrap();
    git(&home, &["worktree", "remove", acquired.to_str().unwrap()]);
    // Another user can reuse the removed path. A refused retry must preserve
    // both those bytes and the completed journal needed after the path clears.
    let draft = q.draft.as_ref().unwrap();
    fs::create_dir(draft).unwrap();
    let note = draft.join("user-note.txt");
    fs::write(&note, b"reused path custody").unwrap();
    let refused = submit(&policy, &q).unwrap();
    assert_eq!(refused["status"], "rejected", "{refused}");
    assert_eq!(refused["published"], false);
    assert_eq!(fs::read(&note).unwrap(), b"reused path custody");
    assert_eq!(git(&home, &["rev-parse", "main"]), accepted);
    assert_eq!(git(&home, &["rev-parse", "HEAD"]), accepted);
    assert_eq!(git(&home, &["rev-list", "--count", "main"]), count);
    assert_eq!(git(&other, &["rev-parse", "HEAD"]), other_head);
    fs::remove_file(note).unwrap();
    fs::remove_dir(draft).unwrap();
    // Non-directory reuse is also custody, including dangling links that
    // Path::exists would mistake for an absent completed worktree.
    let other_note = other.join("user-note.txt");
    fs::write(&other_note, b"symlink target custody").unwrap();
    let absent = dir.join("absent-target");
    for target in [None, Some(absent.as_path()), Some(other.as_path())] {
        if let Some(target) = target {
            std::os::unix::fs::symlink(target, draft).unwrap();
        } else {
            fs::write(draft, b"regular file custody").unwrap();
        }
        let refused = submit(&policy, &q).unwrap();
        assert_eq!(refused["status"], "rejected", "{refused}");
        assert_eq!(refused["published"], false);
        if let Some(target) = target {
            assert_eq!(fs::read_link(draft).unwrap(), target);
        } else {
            assert_eq!(fs::read(draft).unwrap(), b"regular file custody");
        }
        assert_eq!(fs::read(&other_note).unwrap(), b"symlink target custody");
        assert!(!absent.exists());
        assert_eq!(git(&home, &["rev-parse", "main"]), accepted);
        assert_eq!(git(&home, &["rev-parse", "HEAD"]), accepted);
        assert_eq!(git(&home, &["rev-list", "--count", "main"]), count);
        assert_eq!(git(&other, &["rev-parse", "HEAD"]), other_head);
        fs::remove_file(draft).unwrap();
    }
    fs::remove_file(other_note).unwrap();
    // Retry now owns only its stale branch ref, not the removed new worktree.
    let retried = submit(&policy, &q).unwrap();
    assert_eq!(
        git(&home, &["for-each-ref", "--format=%(refname)", &branch]),
        ""
    );
    assert_eq!(retried["status"], "current");
    assert_eq!(retried["published"], false);
    assert_eq!(retried["candidate"], accepted);
    assert_eq!(retried["view"], home.to_str().unwrap());
    assert_eq!(git(&home, &["rev-parse", "HEAD"]), accepted);
    assert_eq!(git(&home, &["rev-list", "--count", "main"]), count);
    assert_eq!(git(&other, &["rev-parse", "HEAD"]), other_head);
    assert_eq!(
        local::sql(&policy, &home, "SELECT count(*) FROM nodes").unwrap()["result"]["rows"][0][0],
        1
    );
}

#[test]
fn missing_completed_directory_retains_registration_until_exact_cleanup_retry() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().canonicalize().unwrap();
    let (policy, home, q, other) = fixture(&dir);
    let draft = q.draft.as_ref().unwrap();
    let branch = git(draft, &["symbolic-ref", "--quiet", "HEAD"]);
    let other_head = git(&other, &["rev-parse", "HEAD"]);
    let checked = crate::checked_publication::save(&policy, &q);
    assert_eq!(checked.status, "current");
    let accepted = checked.candidate.unwrap();
    local::advance(&policy, &home).unwrap();
    let root = home.join(".git/sifttext-saves");
    fs::create_dir(&root).unwrap();
    let key = format!(
        "{:x}",
        <sha2::Sha256 as sha2::Digest>::digest(draft.as_os_str().as_encoded_bytes())
    );
    // Reproduce the actual durable state just before git worktree remove: its
    // filesystem deletion and registration deletion are not one atomic action.
    super::persist(
        &root.join(format!("{key}.json")),
        &super::Pending {
            home: home.clone(),
            draft: draft.clone(),
            commit: q.commit.clone().unwrap(),
            base: q.base.clone().unwrap(),
            branch: branch.clone(),
            explicit: q.explicit.clone(),
            prepared_heads: None,
            completed_head: Some(accepted.clone()),
        },
    )
    .unwrap();
    fs::remove_dir_all(draft).unwrap();
    assert!(
        git(&home, &["worktree", "list", "--porcelain"])
            .lines()
            .any(|line| line == format!("branch {branch}"))
    );
    let count = git(&home, &["rev-list", "--count", "main"]);
    let retried = submit(&policy, &q).unwrap();
    assert_eq!(retried["status"], "current");
    assert_eq!(retried["published"], false);
    assert_eq!(retried["candidate"], accepted);
    assert_eq!(git(&home, &["rev-parse", "HEAD"]), accepted);
    assert_eq!(git(&home, &["rev-list", "--count", "main"]), count);
    assert_eq!(
        git(&home, &["for-each-ref", "--format=%(refname)", &branch]),
        ""
    );
    assert!(
        !git(&home, &["worktree", "list", "--porcelain"])
            .lines()
            .any(|line| line == format!("branch {branch}"))
    );
    assert_eq!(git(&other, &["rev-parse", "HEAD"]), other_head);
    assert_eq!(
        local::sql(&policy, &home, "SELECT count(*) FROM nodes").unwrap()["result"]["rows"][0][0],
        1
    );
}

#[test]
fn pre_cas_sigkill_resumes_exact_combined_candidate_from_original_request() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().canonicalize().unwrap();
    let (policy, home, q, other) = fixture(&dir);
    let retained = local::worktree(&policy, &home).unwrap();
    let retained = PathBuf::from(retained["worktree"].as_str().unwrap());
    let retained_head = git(&retained, &["rev-parse", "HEAD"]);
    local::create(
        &policy,
        &other,
        "Concurrentanchor",
        "Creates divergent accepted history.",
        None,
    )
    .unwrap();
    git(&other, &["add", "nodes", "trees"]);
    git(&other, &["commit", "-qm", "knowledge: competing root"]);
    let mut competing: Request = serde_json::from_value(json!({"repo":home,"accepted_ref":q.accepted_ref,"draft":other,"commit":git(&other, &["rev-parse", "HEAD"]),"base":q.base,"explicit":[]})).unwrap();
    competing.pause_before_publish = None;
    assert_eq!(submit(&policy, &competing).unwrap()["status"], "current");
    let accepted_before = git(&home, &["rev-parse", "main"]);
    let ready = dir.join("pre-cas-ready");
    let release = dir.join("pre-cas-release");
    let path = dir.join("pre-cas-request.json");
    fs::write(&path, serde_json::to_vec(&json!({"repo":q.repo,"accepted_ref":q.accepted_ref,"draft":q.draft,"commit":q.commit,"base":q.base,"explicit":[],"pause_before_publish":{"ready":ready,"release":release}})).unwrap()).unwrap();
    let mut worker = PendingChild(Some(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "local::submission::tests::save_pause_worker",
                "--nocapture",
            ])
            .env("SIFTTEXT_SAVE_TEST_REQUEST", &path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    ));
    let started = Instant::now();
    while !ready.exists() {
        assert!(
            worker.0.as_mut().unwrap().try_wait().unwrap().is_none(),
            "worker exited before pre-CAS barrier"
        );
        assert!(
            started.elapsed() < Duration::from_secs(30),
            "publisher did not reach pre-CAS barrier"
        );
        thread::sleep(Duration::from_millis(5));
    }
    let combined = git(q.draft.as_ref().unwrap(), &["rev-parse", "HEAD"]);
    assert_ne!(Some(&combined), q.commit.as_ref());
    assert_eq!(git(&home, &["rev-parse", "main"]), accepted_before);
    let mut child = worker.0.take().unwrap();
    child.kill().unwrap();
    assert!(!child.wait().unwrap().success());
    let result = submit(&policy, &q).unwrap();
    assert_eq!(result["status"], "current", "{result}");
    assert_eq!(result["published"], true);
    assert_eq!(
        result["candidate"], combined,
        "retry must reuse the exact recorded integration commit"
    );
    assert_eq!(git(&home, &["rev-parse", "HEAD"]), combined);
    assert_eq!(git(&home, &["rev-parse", "main"]), combined);
    assert!(!q.draft.as_ref().unwrap().exists());
    assert_eq!(git(&retained, &["rev-parse", "HEAD"]), retained_head);
    assert_eq!(git(&retained, &["status", "--porcelain"]), "");
    assert_eq!(
        local::sql(&policy, &home, "SELECT count(*) FROM nodes").unwrap()["result"]["rows"][0][0],
        2
    );
}
// An accepted save can remain unfinished while another draft publishes and
// finishes the home. Retry must target the newer accepted result, not roll back.
#[test]
fn post_cas_retry_after_competing_completion_preserves_both_saves() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().canonicalize().unwrap();
    let (policy, home, q, other) = fixture(&dir);
    let retained = local::worktree(&policy, &home).unwrap();
    let retained = PathBuf::from(retained["worktree"].as_str().unwrap());
    let retained_head = git(&retained, &["rev-parse", "HEAD"]);
    local::create(
        &policy,
        &other,
        "Lateranchor",
        "Checks a newer accepted save.",
        None,
    )
    .unwrap();
    git(&other, &["add", "nodes", "trees"]);
    git(&other, &["commit", "-qm", "knowledge: later save"]);
    let competing: Request = serde_json::from_value(json!({
        "repo":home,"accepted_ref":q.accepted_ref,"draft":other,
        "commit":git(&other, &["rev-parse", "HEAD"]),"base":q.base,"explicit":[]
    }))
    .unwrap();
    let ready = dir.join("interleaved-ready");
    let release = dir.join("interleaved-release");
    let path = dir.join("interleaved-request.json");
    fs::write(
        &path,
        serde_json::to_vec(&json!({
            "repo":q.repo,"accepted_ref":q.accepted_ref,"draft":q.draft,
            "commit":q.commit,"base":q.base,"explicit":[],
            "pause_after_publish":{"ready":ready,"release":release}
        }))
        .unwrap(),
    )
    .unwrap();
    let mut worker = PendingChild(Some(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "local::submission::tests::save_pause_worker",
                "--nocapture",
            ])
            .env("SIFTTEXT_SAVE_TEST_REQUEST", &path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    ));
    let started = Instant::now();
    while !ready.exists() {
        assert!(
            worker.0.as_mut().unwrap().try_wait().unwrap().is_none(),
            "worker exited before post-CAS barrier"
        );
        assert!(started.elapsed() < Duration::from_secs(30));
        thread::sleep(Duration::from_millis(5));
    }
    let mut child = worker.0.take().unwrap();
    child.kill().unwrap();
    assert!(!child.wait().unwrap().success());
    let first = git(&home, &["rev-parse", "main"]);
    assert_eq!(Some(&first), q.commit.as_ref());
    assert!(q.draft.as_ref().unwrap().exists());
    let later = submit(&policy, &competing).unwrap();
    assert_eq!(later["status"], "current", "{later}");
    let accepted = git(&home, &["rev-parse", "main"]);
    assert_ne!(accepted, first);
    git(&home, &["merge-base", "--is-ancestor", &first, &accepted]);
    assert_eq!(git(&home, &["rev-parse", "HEAD"]), accepted);
    assert!(!other.exists());
    let count = git(&home, &["rev-list", "--count", "main"]);
    let retried = submit(&policy, &q).unwrap();
    assert_eq!(retried["status"], "current", "{retried}");
    assert_eq!(retried["published"], false);
    assert_eq!(retried["candidate"], accepted);
    assert_eq!(retried["version"], later["version"]);
    assert_eq!(git(&home, &["rev-list", "--count", "main"]), count);
    assert_eq!(git(&home, &["rev-parse", "HEAD"]), accepted);
    assert!(!q.draft.as_ref().unwrap().exists());
    assert_eq!(git(&retained, &["rev-parse", "HEAD"]), retained_head);
    assert_eq!(
        git(
            &retained,
            &["status", "--porcelain=v1", "--untracked-files=all"]
        ),
        ""
    );
    let rows = local::sql(&policy, &home, "SELECT name FROM nodes ORDER BY name").unwrap();
    assert_eq!(
        rows["result"]["rows"],
        json!([["Lateranchor"], ["Recoveryanchor"]])
    );
    let outlines = crate::outlines::inspect(&policy, &home, None).unwrap();
    assert_eq!(outlines.status, "current");
    assert_eq!(outlines.version.as_deref(), retried["version"].as_str());
}

// Keep one save alive at its checked pre-CAS boundary while a second save
// publishes. The loser must retain its draft, then combine both on retry.
#[test]
fn live_pre_cas_contention_retains_loser_and_combines_on_retry() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().canonicalize().unwrap();
    let (policy, home, mut q, other) = fixture(&dir);
    let retained = local::worktree(&policy, &home).unwrap();
    let retained = PathBuf::from(retained["worktree"].as_str().unwrap());
    let retained_head = git(&retained, &["rev-parse", "HEAD"]);
    let draft = q.draft.as_ref().unwrap().clone();
    let original = git(&draft, &["rev-parse", "HEAD"]);
    let authored = crate::read_regular(
        &fs::read_dir(draft.join("nodes"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path()
            .join("node.md"),
    )
    .unwrap();
    local::create(
        &policy,
        &other,
        "Liveanchor",
        "Checks competing live publication.",
        None,
    )
    .unwrap();
    git(&other, &["add", "nodes", "trees"]);
    git(&other, &["commit", "-qm", "knowledge: live competing save"]);
    let competing: Request = serde_json::from_value(json!({
        "repo":home,"accepted_ref":q.accepted_ref,"draft":other,
        "commit":git(&other, &["rev-parse", "HEAD"]),"base":q.base,"explicit":[]
    }))
    .unwrap();
    let ready = dir.join("live-ready");
    let release = dir.join("live-release");
    q.pause_before_publish = Some(Barrier {
        ready: ready.clone(),
        release: release.clone(),
    });
    let refused = thread::scope(|scope| {
        let first = scope.spawn(|| submit(&policy, &q).map_err(|e| e.to_string()));
        let started = Instant::now();
        while !ready.exists() {
            assert!(!first.is_finished(), "first save exited before pre-CAS");
            assert!(started.elapsed() < Duration::from_secs(30));
            thread::sleep(Duration::from_millis(5));
        }
        // Release even on failure so the scoped worker cannot hang the test.
        let second = submit(&policy, &competing);
        fs::write(&release, "release").unwrap();
        assert_eq!(second.unwrap()["status"], "current");
        first.join().unwrap().unwrap()
    });
    assert_eq!(refused["status"], "rejected", "{refused}");
    assert_eq!(refused["check"], "source_or_ref_changed");
    assert_eq!(refused["published"], false);
    assert!(draft.exists());
    assert!(!other.exists());
    assert_eq!(git(&draft, &["rev-parse", "HEAD"]), original);
    let file = fs::read_dir(draft.join("nodes"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path()
        .join("node.md");
    assert_eq!(fs::read(&file).unwrap(), authored);
    assert_eq!(git(&draft, &["status", "--porcelain"]), "");
    q.pause_before_publish = None;
    let result = submit(&policy, &q).unwrap();
    assert_eq!(result["status"], "current", "{result}");
    assert_eq!(result["published"], true);
    let accepted = result["candidate"].as_str().unwrap();
    assert_eq!(git(&home, &["rev-parse", "HEAD"]), accepted);
    assert_eq!(git(&home, &["rev-parse", "main"]), accepted);
    for commit in [&original, competing.commit.as_ref().unwrap()] {
        git(&home, &["merge-base", "--is-ancestor", commit, accepted]);
    }
    assert_eq!(
        fs::read(home.join(file.strip_prefix(&draft).unwrap())).unwrap(),
        authored
    );
    assert!(!draft.exists());
    assert_eq!(git(&retained, &["rev-parse", "HEAD"]), retained_head);
    assert_eq!(git(&retained, &["status", "--porcelain"]), "");
    let rows = local::sql(&policy, &home, "SELECT name FROM nodes ORDER BY name").unwrap();
    assert_eq!(rows["version"], result["version"]);
    assert_eq!(
        rows["result"]["rows"],
        json!([["Liveanchor"], ["Recoveryanchor"]])
    );
    let outlines = crate::outlines::inspect(&policy, &home, None).unwrap();
    assert_eq!(outlines.status, "current");
    assert_eq!(outlines.version.as_deref(), result["version"].as_str());
}

#[test]
fn removed_worktree_failed_branch_cleanup_resumes_without_new_commit() {
    let temp = tempfile::tempdir().unwrap();
    let dir = temp.path().canonicalize().unwrap();
    let (policy, home, q, other) = fixture(&dir);
    let draft = q.draft.as_ref().unwrap();
    let branch = git(draft, &["symbolic-ref", "--quiet", "HEAD"]);
    let lock = home.join(".git").join(format!("{branch}.lock"));
    // Created only after save reaches its post-CAS barrier, because Git's merge
    // may itself need this branch lock while constructing the exact candidate.
    let ready = dir.join("cleanup-ready");
    let release = dir.join("cleanup-release");
    let mut paused: Request = serde_json::from_value(json!({"repo":q.repo,"accepted_ref":q.accepted_ref,"draft":q.draft,"commit":q.commit,"base":q.base,"explicit":[]})).unwrap();
    paused.pause_after_publish = Some(Barrier {
        ready: ready.clone(),
        release: release.clone(),
    });
    thread::scope(|scope| {
        let result = scope.spawn(|| submit(&policy, &paused).is_err());
        let start = Instant::now();
        while !ready.exists() {
            assert!(start.elapsed() < Duration::from_secs(30));
            thread::sleep(Duration::from_millis(5));
        }
        fs::write(&lock, "held cleanup lock").unwrap();
        fs::write(&release, "release").unwrap();
        assert!(result.join().unwrap());
    });
    assert!(
        !draft.exists(),
        "failure must be after actual worktree removal"
    );
    let head = git(&home, &["rev-parse", "HEAD"]);
    let count = git(&home, &["rev-list", "--count", "main"]);
    fs::remove_file(lock).unwrap();
    let result = submit(&policy, &q).unwrap();
    assert_eq!(result["status"], "current");
    assert_eq!(result["candidate"], head);
    assert_eq!(git(&home, &["rev-list", "--count", "main"]), count);
    assert_eq!(
        git(&home, &["for-each-ref", "--format=%(refname)", &branch]),
        ""
    );
    assert!(other.exists());
}
