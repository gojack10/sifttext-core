use crate::local;
use serde_json::{json, Value};
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
