use super::{advance, advance_inner};
use crate::{Policy, checked_publication, index, local};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn git(home: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(home)
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgSign=false",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap().trim().to_owned()
}
fn policy() -> Policy {
    Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap()
}
fn home(dir: &Path, policy: &Policy) -> PathBuf {
    let home = dir.join("home");
    local::init(policy, &home, None).unwrap();
    home
}
fn publish(policy: &Policy, home: &Path, name: &str) -> checked_publication::Receipt {
    let a = local::worktree(policy, home).unwrap();
    let draft = Path::new(a["worktree"].as_str().unwrap());
    local::create(
        policy,
        draft,
        name,
        "Owns a reversible synthetic observation.",
        None,
    )
    .unwrap();
    git(draft, &["add", "--", "nodes", "trees"]);
    git(draft, &["commit", "-m", "knowledge: synthetic advance"]);
    let r = checked_publication::submit(
        policy,
        &checked_publication::Request {
            repo: home.into(),
            accepted_ref: "refs/heads/main".into(),
            draft: Some(draft.into()),
            commit: Some(git(draft, &["rev-parse", "HEAD"])),
            base: Some(a["base"].as_str().unwrap().into()),
            explicit: BTreeSet::new(),
            pause_before_publish: None,
            pause_after_publish: None,
        },
    );
    assert_eq!(r.status, "current", "{r:?}");
    r
}
#[test]
fn clean_advance_is_idempotent_and_preserves_detached_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let policy = policy();
    let home = home(dir.path(), &policy);
    let before = git(&home, &["rev-parse", "HEAD"]);
    let r = publish(&policy, &home, "Advancefirst");
    assert_eq!(git(&home, &["rev-parse", "HEAD"]), before);
    let view = r.view.unwrap();
    assert_ne!(view, home);
    assert!(
        advance(&policy, &view).is_err(),
        "linked accepted views are not independent homes"
    );
    let result = advance(&policy, &home).unwrap();
    assert_eq!(result["reader_before"], before);
    assert_eq!(result["reader"].as_str(), r.candidate.as_deref());
    assert_eq!(result["reader"], result["accepted"]);
    assert_eq!(result["version"].as_str(), r.version.as_deref());
    assert_eq!(result["advanced"], true);
    let again = advance(&policy, &home).unwrap();
    assert_eq!(again["advanced"], false);
    assert_eq!(again["version"], result["version"]);
    let second = publish(&policy, &home, "Advancesecond");
    advance(&policy, &home).unwrap();
    assert_eq!(
        local::sql(&policy, &home, "SELECT count(*) FROM nodes").unwrap()["result"]["rows"][0][0],
        2
    );
    assert_eq!(
        git(&view, &["rev-parse", "HEAD"]).as_str(),
        r.candidate.as_deref().unwrap()
    );
    assert_eq!(
        git(&home, &["rev-parse", "HEAD"]).as_str(),
        second.candidate.as_deref().unwrap()
    );
    local::worktree(&policy, &home).unwrap();
}
#[test]
fn refuses_dirty_divergent_branch_owned_and_unsafe_readers_without_overwrite() {
    let dir = tempfile::tempdir().unwrap();
    let policy = policy();
    let home = home(dir.path(), &policy);
    publish(&policy, &home, "First");
    advance(&policy, &home).unwrap();
    let before = git(&home, &["rev-parse", "HEAD"]);
    let id = crate::Store::read(&home).unwrap().roots[0].clone();
    let file = home.join(format!("nodes/{id}/node.md"));
    let original = fs::read(&file).unwrap();
    fs::write(
        &file,
        [original.as_slice(), b"Unfinished authored work\n"].concat(),
    )
    .unwrap();
    fs::write(home.join("user-note"), "Untracked intent").unwrap();
    assert!(advance(&policy, &home).is_err());
    assert!(
        fs::read_to_string(&file)
            .unwrap()
            .ends_with("Unfinished authored work\n")
    );
    assert_eq!(
        fs::read_to_string(home.join("user-note")).unwrap(),
        "Untracked intent"
    );
    assert_eq!(git(&home, &["rev-parse", "HEAD"]), before);
    git(
        &home,
        &[
            "update-index",
            "--assume-unchanged",
            &format!("nodes/{id}/node.md"),
        ],
    );
    assert!(advance(&policy, &home).is_err());
    git(
        &home,
        &[
            "update-index",
            "--no-assume-unchanged",
            &format!("nodes/{id}/node.md"),
        ],
    );
    fs::write(&file, original).unwrap();
    fs::remove_file(home.join("user-note")).unwrap();
    let link = dir.path().join("alias");
    std::os::unix::fs::symlink(&home, &link).unwrap();
    assert!(advance(&policy, &link).is_err());
    assert!(advance(&policy, &home.join("nodes")).is_err());
    assert!(advance(&policy, Path::new("relative-home")).is_err());
    git(&home, &["switch", "-c", "user-reader"]);
    assert!(advance(&policy, &home).is_err());
    git(&home, &["checkout", "--detach"]);
    git(
        &home,
        &[
            "commit",
            "--allow-empty",
            "-m",
            "knowledge: divergent reader intent",
        ],
    );
    let divergent = git(&home, &["rev-parse", "HEAD"]);
    assert!(advance(&policy, &home).is_err());
    assert_eq!(git(&home, &["rev-parse", "HEAD"]), divergent);
    assert_eq!(git(&home, &["rev-parse", "refs/heads/main"]), before);
}
#[test]
fn accepted_race_retains_reader_and_interrupted_refresh_retries_truthfully() {
    let dir = tempfile::tempdir().unwrap();
    let policy = policy();
    let home = home(dir.path(), &policy);
    let before = git(&home, &["rev-parse", "HEAD"]);
    let r = publish(&policy, &home, "Racefirst");
    let target = r.candidate.unwrap();
    let err = advance_inner(&policy, &home, None, || {
        git(&home, &["update-ref", "refs/heads/main", &before, &target]);
        Ok(())
    })
    .unwrap_err();
    assert!(err.to_string().contains("accepted ref changed"), "{err}");
    assert_eq!(git(&home, &["rev-parse", "HEAD"]), before);
    // Our synthetic controller restores its own accepted ref; no caller draft was moved.
    git(&home, &["update-ref", "refs/heads/main", &target, &before]);
    let ready = dir.path().join("ready");
    let release = dir.path().join("release");
    let pause = index::PauseBeforeAccept { ready, release };
    // An unreleased real refresh barrier expires; HEAD is retained at the ff target,
    // never reported current. A normal retry repairs the derived view.
    let err = advance_inner(&policy, &home, Some(&pause), || Ok(())).unwrap_err();
    assert!(
        err.to_string().contains("reader advance incomplete"),
        "{err}"
    );
    assert_eq!(git(&home, &["rev-parse", "HEAD"]), target);
    let repaired = advance(&policy, &home).unwrap();
    assert_eq!(repaired["advanced"], false);
    assert_eq!(repaired["accepted"], target);
    let next = publish(&policy, &home, "Racesecond").candidate.unwrap();
    let pause = index::PauseBeforeAccept {
        ready: dir.path().join("post-ff-ready"),
        release: dir.path().join("post-ff-release"),
    };
    let mut worker = None;
    let err = advance_inner(&policy, &home, Some(&pause), || {
        let home = home.clone();
        let ready = pause.ready.clone();
        let release = pause.release.clone();
        let old = target.clone();
        let new = next.clone();
        worker = Some(std::thread::spawn(move || {
            let start = std::time::Instant::now();
            while !ready.exists() {
                assert!(start.elapsed() < std::time::Duration::from_secs(10));
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            git(&home, &["update-ref", "refs/heads/main", &old, &new]);
            fs::write(release, "continue").unwrap();
        }));
        Ok(())
    })
    .unwrap_err();
    worker.unwrap().join().unwrap();
    assert!(err.to_string().contains("accepted ref changed"), "{err}");
    assert_eq!(git(&home, &["rev-parse", "HEAD"]), next);
    assert_eq!(git(&home, &["rev-parse", "refs/heads/main"]), target);
    git(&home, &["update-ref", "refs/heads/main", &next, &target]);
    assert_eq!(advance(&policy, &home).unwrap()["accepted"], next);
}
#[test]
fn unvalidated_invalid_accepted_branch_does_not_move_reader() {
    let dir = tempfile::tempdir().unwrap();
    let policy = policy();
    let home = home(dir.path(), &policy);
    let before = git(&home, &["rev-parse", "HEAD"]);
    let a = local::worktree(&policy, &home).unwrap();
    let draft = Path::new(a["worktree"].as_str().unwrap());
    let created = local::create(
        &policy,
        draft,
        "Invalidaccepted",
        "A required summary.",
        None,
    )
    .unwrap();
    let file = Path::new(created["path"].as_str().unwrap());
    let text = fs::read_to_string(file)
        .unwrap()
        .replace("A required summary.", "");
    fs::write(file, &text).unwrap();
    git(draft, &["add", "--", "nodes", "trees"]);
    git(
        draft,
        &[
            "commit",
            "-m",
            "knowledge: invalid test-only accepted branch",
        ],
    );
    let invalid = git(draft, &["rev-parse", "HEAD"]);
    // Deliberately bypass publication in the disposable negative specimen only.
    git(&home, &["update-ref", "refs/heads/main", &invalid, &before]);
    let err = advance(&policy, &home).unwrap_err();
    assert!(
        err.to_string()
            .contains("checked accepted view unavailable"),
        "{err}"
    );
    assert_eq!(git(&home, &["rev-parse", "HEAD"]), before);
    assert_eq!(fs::read_to_string(file).unwrap(), text);
    assert_eq!(git(draft, &["rev-parse", "HEAD"]), invalid);
}

#[test]
fn symbolic_and_symlink_accepted_refs_are_not_trusted() {
    let dir = tempfile::tempdir().unwrap();
    let policy = policy();
    let home = home(dir.path(), &policy);
    let before = git(&home, &["rev-parse", "HEAD"]);
    git(&home, &["update-ref", "refs/heads/other", &before]);
    git(
        &home,
        &["symbolic-ref", "refs/heads/main", "refs/heads/other"],
    );
    assert!(advance(&policy, &home).is_err());
    assert_eq!(git(&home, &["rev-parse", "HEAD"]), before);
    let main = home.join(".git/refs/heads/main");
    fs::remove_file(&main).unwrap();
    std::os::unix::fs::symlink("other", &main).unwrap();
    assert!(advance(&policy, &home).is_err());
    assert_eq!(git(&home, &["rev-parse", "HEAD"]), before);
}
