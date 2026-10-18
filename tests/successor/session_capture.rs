use crate::local;
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command};

fn git(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
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
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn descriptor() -> Value {
    json!({
        "schema_version": 1,
        "harness": "pi",
        "session_id": "session-1",
        "availability": "available",
        "archive_key": "sessions/session-1.jsonl",
        "entry_ids": ["entry-1", "entry-2"],
        "branch_entry_ids": ["entry-1", "entry-2"],
        "leaf_id": "entry-2",
        "cutoff_entry_id": "entry-2"
    })
}

fn snapshot() -> Vec<u8> {
    concat!(
        "{\"type\":\"session\",\"id\":\"session-1\"}\n",
        "{\"id\":\"entry-1\",\"parentId\":null}\n",
        "{\"id\":\"entry-2\",\"parentId\":\"entry-1\"}\n"
    )
    .as_bytes()
    .to_vec()
}

fn archive() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    git(root.path(), &["init", "-q", "-b", "main"]);
    git(root.path(), &["config", "--local", "user.name", "Test"]);
    git(
        root.path(),
        &["config", "--local", "user.email", "test@example.invalid"],
    );
    git(root.path(), &["commit", "--allow-empty", "-qm", "baseline"]);
    root
}

#[test]
fn capture_copies_exact_snapshot_and_commits_only_that_session() {
    let archive = archive();
    let source_dir = tempfile::tempdir().unwrap();
    let source = source_dir.path().join("source.jsonl");
    let bytes = snapshot();
    fs::write(&source, &bytes).unwrap();
    fs::write(archive.path().join("other.txt"), "leave staged").unwrap();
    git(archive.path(), &["add", "other.txt"]);

    let result = local::capture_session(&source, &descriptor(), archive.path()).unwrap();
    let path = archive.path().join("sessions/session-1.jsonl");
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(result["status"], "captured");
    assert_eq!(
        git(
            archive.path(),
            &["show", "--pretty=format:", "--name-only", "HEAD"]
        ),
        "sessions/session-1.jsonl"
    );
    assert_eq!(
        git(archive.path(), &["diff", "--cached", "--name-only"]),
        "other.txt"
    );

    let head = git(archive.path(), &["rev-parse", "HEAD"]);
    let repeated = local::capture_session(&source, &descriptor(), archive.path()).unwrap();
    assert_eq!(repeated["commit"], head);
    assert_eq!(git(archive.path(), &["rev-parse", "HEAD"]), head);
}

#[test]
fn capture_checks_header_and_active_branch_not_only_entry_order() {
    let archive = archive();
    let source_dir = tempfile::tempdir().unwrap();
    let source = source_dir.path().join("source.jsonl");
    fs::write(
        &source,
        concat!(
            "{\"type\":\"session\",\"id\":\"session-1\"}\n",
            "{\"id\":\"entry-1\",\"parentId\":null}\n",
            "{\"id\":\"entry-2\",\"parentId\":\"entry-1\"}\n",
            "{\"id\":\"entry-3\",\"parentId\":\"entry-1\"}\n"
        ),
    )
    .unwrap();
    let mut branch = descriptor();
    branch["entry_ids"] = json!(["entry-1", "entry-2", "entry-3"]);
    branch["branch_entry_ids"] = json!(["entry-1", "entry-3"]);
    branch["leaf_id"] = json!("entry-3");
    branch["cutoff_entry_id"] = json!("entry-3");
    local::capture_session(&source, &branch, archive.path()).unwrap();

    let target = archive.path().join("sessions/session-1.jsonl");
    let saved = fs::read(&target).unwrap();
    let head = git(archive.path(), &["rev-parse", "HEAD"]);
    branch["branch_entry_ids"] = json!(["entry-1", "entry-2"]);
    assert!(local::capture_session(&source, &branch, archive.path()).is_err());
    assert_eq!(fs::read(&target).unwrap(), saved);
    assert_eq!(git(archive.path(), &["rev-parse", "HEAD"]), head);

    branch["branch_entry_ids"] = json!(["entry-1", "entry-3"]);
    let bad_header = fs::read_to_string(&source)
        .unwrap()
        .replacen("session-1", "other-session", 1);
    fs::write(&source, bad_header).unwrap();
    assert!(local::capture_session(&source, &branch, archive.path()).is_err());
    assert_eq!(fs::read(&target).unwrap(), saved);
}

#[test]
fn first_capture_commits_to_an_unborn_archive_without_committing_other_staged_files() {
    let archive = tempfile::tempdir().unwrap();
    git(archive.path(), &["init", "-q", "-b", "main"]);
    git(archive.path(), &["config", "--local", "user.name", "Test"]);
    git(
        archive.path(),
        &["config", "--local", "user.email", "test@example.invalid"],
    );
    fs::write(archive.path().join("other.txt"), "leave staged").unwrap();
    git(archive.path(), &["add", "other.txt"]);

    let source_dir = tempfile::tempdir().unwrap();
    let source = source_dir.path().join("source.jsonl");
    let bytes = snapshot();
    fs::write(&source, &bytes).unwrap();
    let result = local::capture_session(&source, &descriptor(), archive.path()).unwrap();

    assert_eq!(
        fs::read(archive.path().join("sessions/session-1.jsonl")).unwrap(),
        bytes
    );
    assert_eq!(result["status"], "captured");
    assert_eq!(
        git(
            archive.path(),
            &["show", "--pretty=format:", "--name-only", "HEAD"]
        ),
        "sessions/session-1.jsonl"
    );
    assert_eq!(
        git(archive.path(), &["diff", "--cached", "--name-only"]),
        "other.txt"
    );
}

#[test]
fn unavailable_descriptor_does_not_create_or_touch_archive() {
    let archive = tempfile::tempdir().unwrap();
    let descriptor = json!({
        "schema_version": 1,
        "harness": "pi",
        "availability": "unavailable",
        "session_id": null,
        "archive_key": null,
        "unavailable_reason": "source_file_unavailable"
    });
    let result =
        local::capture_session(Path::new("missing.jsonl"), &descriptor, archive.path()).unwrap();
    assert_eq!(result["status"], "source_unavailable");
    assert_eq!(fs::read_dir(archive.path()).unwrap().count(), 0);
}

#[test]
fn capture_refuses_symlink_source() {
    let archive = archive();
    let source_dir = tempfile::tempdir().unwrap();
    let target = source_dir.path().join("real.jsonl");
    let source = source_dir.path().join("source.jsonl");
    fs::write(&target, snapshot()).unwrap();
    std::os::unix::fs::symlink(&target, &source).unwrap();

    let head = git(archive.path(), &["rev-parse", "HEAD"]);
    assert!(local::capture_session(&source, &descriptor(), archive.path()).is_err());
    assert!(!archive.path().join("sessions/session-1.jsonl").exists());
    assert_eq!(git(archive.path(), &["rev-parse", "HEAD"]), head);
}

#[test]
fn stale_or_malformed_snapshot_preserves_previous_archive_and_head() {
    let archive = archive();
    let source_dir = tempfile::tempdir().unwrap();
    let source = source_dir.path().join("source.jsonl");
    let bytes = snapshot();
    fs::write(&source, &bytes).unwrap();
    local::capture_session(&source, &descriptor(), archive.path()).unwrap();
    let target = archive.path().join("sessions/session-1.jsonl");
    let head = git(archive.path(), &["rev-parse", "HEAD"]);

    fs::write(
        &source,
        [
            bytes.as_slice(),
            b"{\"id\":\"entry-3\",\"parentId\":\"entry-2\"}\n",
        ]
        .concat(),
    )
    .unwrap();
    assert!(local::capture_session(&source, &descriptor(), archive.path()).is_err());
    assert_eq!(fs::read(&target).unwrap(), bytes);
    assert_eq!(git(archive.path(), &["rev-parse", "HEAD"]), head);

    fs::write(&source, b"{malformed json}\n").unwrap();
    assert!(local::capture_session(&source, &descriptor(), archive.path()).is_err());
    assert_eq!(fs::read(&target).unwrap(), bytes);
    assert_eq!(git(archive.path(), &["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read_dir(target.parent().unwrap()).unwrap().count(), 1);
}

#[cfg(unix)]
#[test]
fn failed_first_checkpoint_removes_new_archive_file_and_index_entry() {
    let archive = archive();
    git(archive.path(), &["config", "--local", "user.name", ""]);
    git(archive.path(), &["config", "--local", "user.email", ""]);
    let head = git(archive.path(), &["rev-parse", "HEAD"]);
    let source_dir = tempfile::tempdir().unwrap();
    let source = source_dir.path().join("source.jsonl");
    fs::write(&source, snapshot()).unwrap();

    assert!(local::capture_session(&source, &descriptor(), archive.path()).is_err());
    assert!(!archive.path().join("sessions/session-1.jsonl").exists());
    assert_eq!(git(archive.path(), &["rev-parse", "HEAD"]), head);
    assert_eq!(git(archive.path(), &["status", "--porcelain"]), "");
}

#[cfg(unix)]
#[test]
fn failed_checkpoint_keeps_previous_archive_bytes_and_git_state() {
    let archive = archive();
    let source_dir = tempfile::tempdir().unwrap();
    let source = source_dir.path().join("source.jsonl");
    let old_bytes = snapshot();
    fs::write(&source, &old_bytes).unwrap();
    local::capture_session(&source, &descriptor(), archive.path()).unwrap();
    let target = archive.path().join("sessions/session-1.jsonl");
    let head = git(archive.path(), &["rev-parse", "HEAD"]);

    let new_bytes = [
        old_bytes.as_slice(),
        b"{\"id\":\"entry-3\",\"parentId\":\"entry-2\"}\n",
    ]
    .concat();
    fs::write(&source, &new_bytes).unwrap();
    let mut updated = descriptor();
    updated["entry_ids"] = json!(["entry-1", "entry-2", "entry-3"]);
    updated["branch_entry_ids"] = json!(["entry-1", "entry-2", "entry-3"]);
    updated["leaf_id"] = json!("entry-3");
    updated["cutoff_entry_id"] = json!("entry-3");

    git(
        archive.path(),
        &["config", "--local", "--unset", "user.name"],
    );
    git(
        archive.path(),
        &["config", "--local", "--unset", "user.email"],
    );

    assert!(local::capture_session(&source, &updated, archive.path()).is_err());
    assert_eq!(fs::read(&target).unwrap(), old_bytes);
    assert_eq!(git(archive.path(), &["rev-parse", "HEAD"]), head);
    assert_eq!(git(archive.path(), &["status", "--porcelain"]), "");
}

#[test]
fn capture_waits_for_the_shared_archive_lock() {
    let archive = archive();
    let source_dir = tempfile::tempdir().unwrap();
    let source = source_dir.path().join("source.jsonl");
    fs::write(&source, snapshot()).unwrap();

    // Restore fetches and fast-forward merges this same archive worktree while
    // it is checkpointed and, on refusal, rolled back against HEAD. Neither may
    // run across the other, so both take one advisory lock per archive.
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(archive.path().join(".git").join("sifttext-archive.lock"))
        .unwrap();
    lock.lock().unwrap();

    let (sender, receiver) = std::sync::mpsc::channel();
    let (capture_source, capture_archive) = (source.clone(), archive.path().to_path_buf());
    std::thread::spawn(move || {
        let _ = sender
            .send(local::capture_session(&capture_source, &descriptor(), &capture_archive).is_ok());
    });
    assert!(
        receiver
            .recv_timeout(std::time::Duration::from_secs(1))
            .is_err(),
        "capture mutated the archive while another operation held the archive lock"
    );

    drop(lock);
    assert!(
        receiver
            .recv_timeout(std::time::Duration::from_secs(30))
            .unwrap(),
        "capture failed after the archive lock was released"
    );
    assert_eq!(
        fs::read(archive.path().join("sessions/session-1.jsonl")).unwrap(),
        snapshot()
    );
}

#[test]
fn retry_recovers_capture_interrupted_before_git_add() {
    let archive = archive();
    let source_dir = tempfile::tempdir().unwrap();
    let source = source_dir.path().join("source.jsonl");
    let old_bytes = snapshot();
    fs::write(&source, &old_bytes).unwrap();
    local::capture_session(&source, &descriptor(), archive.path()).unwrap();

    let target = archive.path().join("sessions/session-1.jsonl");
    let backup = archive.path().join(
        "sessions/.sifttext-capture-73657373696f6e732f73657373696f6e2d312e6a736f6e6c.previous",
    );
    fs::hard_link(&target, &backup).unwrap();
    let mut new_bytes = old_bytes.clone();
    new_bytes.insert(new_bytes.len() - 1, b' ');
    let replacement = archive.path().join("sessions/replacement.jsonl");
    fs::write(&replacement, &new_bytes).unwrap();
    fs::rename(&replacement, &target).unwrap();
    fs::write(&source, &new_bytes).unwrap();
    let git_dir = git(archive.path(), &["rev-parse", "--absolute-git-dir"]);
    fs::write(
        Path::new(&git_dir)
            .join("sifttext-capture-73657373696f6e732f73657373696f6e2d312e6a736f6e6c.pending"),
        "sessions/session-1.jsonl",
    )
    .unwrap();

    local::capture_session(&source, &descriptor(), archive.path()).unwrap();
    assert_eq!(fs::read(&target).unwrap(), new_bytes);
    assert!(!backup.exists());
    assert_eq!(git(archive.path(), &["status", "--porcelain"]), "");
}

#[test]
fn retry_refuses_symlinked_interrupted_capture_marker() {
    let archive = archive();
    let source_dir = tempfile::tempdir().unwrap();
    let source = source_dir.path().join("source.jsonl");
    let bytes = snapshot();
    fs::write(&source, &bytes).unwrap();
    local::capture_session(&source, &descriptor(), archive.path()).unwrap();

    let key = "sessions/session-1.jsonl";
    let key_hex = key
        .as_bytes()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let git_dir = git(archive.path(), &["rev-parse", "--absolute-git-dir"]);
    let marker = Path::new(&git_dir).join(format!("sifttext-capture-{key_hex}.pending"));
    let external = source_dir.path().join("external-marker");
    fs::write(&external, key).unwrap();
    std::os::unix::fs::symlink(&external, &marker).unwrap();
    let head = git(archive.path(), &["rev-parse", "HEAD"]);
    let target = archive.path().join(key);

    assert!(local::capture_session(&source, &descriptor(), archive.path()).is_err());
    assert!(
        fs::symlink_metadata(&marker)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read_to_string(&external).unwrap(), key);
    assert_eq!(fs::read(&target).unwrap(), bytes);
    assert_eq!(git(archive.path(), &["rev-parse", "HEAD"]), head);
    assert_eq!(git(archive.path(), &["status", "--porcelain"]), "");
}

#[test]
fn abrupt_capture_child() {
    if std::env::var_os("CORE_CAPTURE_KILL_CHILD").is_none() {
        return;
    }
    let source = std::env::var_os("CORE_CAPTURE_SOURCE").unwrap();
    let root = std::env::var_os("CORE_CAPTURE_ARCHIVE").unwrap();
    local::capture_session(Path::new(&source), &descriptor(), Path::new(&root)).unwrap();
}

#[cfg(unix)]
#[test]
fn abrupt_process_death_after_archive_replacement_recovers_on_restart() {
    use std::{
        process::Stdio,
        thread,
        time::{Duration, Instant},
    };

    let archive = archive();
    let source_dir = tempfile::tempdir().unwrap();
    let source = source_dir.path().join("source.jsonl");
    let old_bytes = snapshot();
    fs::write(&source, &old_bytes).unwrap();
    local::capture_session(&source, &descriptor(), archive.path()).unwrap();
    let target = archive.path().join("sessions/session-1.jsonl");
    let head = git(archive.path(), &["rev-parse", "HEAD"]);
    let mut new_bytes = old_bytes.clone();
    new_bytes.insert(new_bytes.len() - 1, b' ');
    fs::write(&source, &new_bytes).unwrap();

    let wrapper_dir = tempfile::tempdir().unwrap();
    let marker = wrapper_dir.path().join("commit-started");
    let calls = wrapper_dir.path().join("git-calls");
    let real_git = std::env::var("PATH")
        .unwrap()
        .split(':')
        .map(|dir| Path::new(dir).join("git"))
        .find(|path| path.is_file())
        .unwrap();
    let wrapper = wrapper_dir.path().join("git");
    fs::write(&wrapper, format!(
        "#!/bin/sh\necho \"$@\" >> '{}'; for arg in \"$@\"; do if [ \"$arg\" = commit ]; then echo $$ > '{}'; while :; do sleep 1; done; fi; done\nexec '{}' \"$@\"\n",
        calls.display(), marker.display(), real_git.display()
    )).unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).unwrap();
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "session_capture_tests::abrupt_capture_child",
            "--nocapture",
        ])
        .env("CORE_CAPTURE_KILL_CHILD", "1")
        .env("CORE_CAPTURE_SOURCE", &source)
        .env("CORE_CAPTURE_ARCHIVE", archive.path())
        .env(
            "PATH",
            format!(
                "{}:{}",
                wrapper_dir.path().display(),
                std::env::var("PATH").unwrap()
            ),
        )
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    while !marker.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    if !marker.exists() {
        let exited = child.try_wait().unwrap().is_some();
        if !exited {
            child.kill().unwrap();
        }
        let status = child.wait().unwrap();
        panic!(
            "capture never reached Git commit; child {status}; calls: {}",
            fs::read_to_string(&calls).unwrap_or_default()
        );
    }
    let git_pid: i32 = fs::read_to_string(&marker).unwrap().trim().parse().unwrap();
    unsafe {
        libc::kill(git_pid, libc::SIGKILL);
    }
    child.kill().unwrap();
    child.wait().unwrap();

    assert_eq!(git(archive.path(), &["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read(&target).unwrap(), new_bytes);

    local::capture_session(&source, &descriptor(), archive.path()).unwrap();
    assert_ne!(git(archive.path(), &["rev-parse", "HEAD"]), head);
    assert_eq!(
        git(
            archive.path(),
            &["rev-parse", "HEAD:sessions/session-1.jsonl"]
        ),
        git(archive.path(), &["hash-object", source.to_str().unwrap()])
    );
    assert_eq!(git(archive.path(), &["status", "--porcelain"]), "");
}
