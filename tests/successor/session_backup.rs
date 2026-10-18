use crate::local;
use serde_json::json;
use std::{
    fs,
    path::Path,
    process::Command,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const NODE: &str = "40000000-0000-4000-8000-000000000001";
const SESSION: &str = "00000000-0000-4000-8000-000000000001";

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

fn config(home: &Path, archive: &Path, remote: &Path) {
    fs::create_dir_all(home.join(".sifttext")).unwrap();
    fs::write(
        home.join(".sifttext/config.json"),
        serde_json::to_vec(&json!({"session_retention": {
            "enabled": true,
            "archive_root": archive,
            "backup_enabled": true,
            "backup_remote": remote
        }}))
        .unwrap(),
    )
    .unwrap();
}

fn archive_with_session(root: &Path) {
    git(root, &["init", "-q", "-b", "main"]);
    git(root, &["config", "--local", "user.name", "Test"]);
    git(
        root,
        &["config", "--local", "user.email", "test@example.invalid"],
    );
    fs::create_dir_all(root.join("sessions")).unwrap();
    fs::write(
        root.join(format!("sessions/{SESSION}.jsonl")),
        b"synthetic session bytes\n",
    )
    .unwrap();
    git(root, &["add", "sessions"]);
    git(root, &["commit", "-qm", "synthetic session"]);
}

fn home_with_attachment(home: &Path) {
    let node = home.join("nodes").join(NODE);
    fs::create_dir_all(node.join("children")).unwrap();
    fs::create_dir_all(home.join("trees")).unwrap();
    fs::write(node.join("children.order"), "").unwrap();
    fs::write(node.join("children/.keep"), "").unwrap();
    fs::write(
        node.join("node.md"),
        format!("---\nid: \"{NODE}\"\nname: \"Root\"\nmicro_summary: \"Root\"\nstatus: \"in_progress\"\nsee_also: []\n---\nRoot\n"),
    )
    .unwrap();
    std::os::unix::fs::symlink(format!("../nodes/{NODE}"), home.join("trees").join(NODE)).unwrap();
    fs::write(
        node.join("session-sources.json"),
        serde_json::to_vec(&json!([{
            "schema_version": 1,
            "harness": "pi",
            "session_id": SESSION,
            "availability": "available",
            "archive_key": format!("sessions/{SESSION}.jsonl"),
            "attachment_name": "pi-synthetic.jsonl",
            "entry_ids": [],
            "branch_entry_ids": [],
            "leaf_id": null,
            "cutoff_entry_id": null
        }]))
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn backup_pushes_only_after_explicit_configuration() {
    let home = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let remote = tempfile::tempdir().unwrap();
    archive_with_session(archive.path());
    git(remote.path(), &["init", "--bare", "-q", "-b", "main"]);
    config(home.path(), archive.path(), remote.path());

    let receipt = local::backup_session_archive(home.path()).unwrap();
    assert_eq!(receipt["status"], "backed_up");
    assert_eq!(receipt["destination_configured"], true);
    assert_eq!(
        git(remote.path(), &["rev-parse", "refs/heads/main"]),
        git(archive.path(), &["rev-parse", "HEAD"])
    );
}

#[test]
fn restore_clones_configured_archive_and_rebuilds_local_link() {
    let home = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let remote = tempfile::tempdir().unwrap();
    let restore_parent = tempfile::tempdir().unwrap();
    let restored = restore_parent.path().join("archive");
    archive_with_session(archive.path());
    git(remote.path(), &["init", "--bare", "-q", "-b", "main"]);
    git(
        archive.path(),
        &[
            "push",
            "-q",
            remote.path().to_str().unwrap(),
            "HEAD:refs/heads/main",
        ],
    );
    config(home.path(), &restored, remote.path());
    home_with_attachment(home.path());

    let receipt = local::restore_session_links(home.path(), &restored).unwrap();
    assert_eq!(receipt["status"], "restored");
    assert_eq!(receipt["links"], 1);
    let link = home
        .path()
        .join(format!("nodes/{NODE}/artifacts/chats/pi-synthetic.jsonl"));
    assert!(
        fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read(&link).unwrap(), b"synthetic session bytes\n");
    assert_eq!(
        git(&restored, &["rev-parse", "HEAD"]),
        git(archive.path(), &["rev-parse", "HEAD"])
    );

    fs::write(
        archive.path().join(format!("sessions/{SESSION}.jsonl")),
        b"updated synthetic session bytes\n",
    )
    .unwrap();
    git(archive.path(), &["add", "sessions"]);
    git(
        archive.path(),
        &["commit", "-qm", "updated synthetic session"],
    );
    git(
        archive.path(),
        &[
            "push",
            "-q",
            remote.path().to_str().unwrap(),
            "HEAD:refs/heads/main",
        ],
    );
    local::restore_session_links(home.path(), &restored).unwrap();
    assert_eq!(
        fs::read(link).unwrap(),
        b"updated synthetic session bytes\n"
    );
    assert_eq!(
        git(&restored, &["rev-parse", "HEAD"]),
        git(archive.path(), &["rev-parse", "HEAD"])
    );
}

/// A knowledge home that reached another machine by clone or pull carries no
/// `.git/info/exclude`, so a restored private chat link must not surface there.
#[test]
fn restore_excludes_private_chat_links_in_a_cloned_home() {
    let remote = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    archive_with_session(archive.path());
    git(remote.path(), &["init", "--bare", "-q", "-b", "main"]);
    git(
        archive.path(),
        &[
            "push",
            "-q",
            remote.path().to_str().unwrap(),
            "HEAD:refs/heads/main",
        ],
    );

    let source = tempfile::tempdir().unwrap();
    git(source.path(), &["init", "-q", "-b", "main"]);
    git(source.path(), &["config", "--local", "user.name", "Test"]);
    git(
        source.path(),
        &["config", "--local", "user.email", "test@example.invalid"],
    );
    home_with_attachment(source.path());
    fs::write(
        source.path().join(".gitignore"),
        "/.sifttext/\n/nodes/*/node.meta.json\n/nodes/*/outline*.lean\n/nodes/*/outline*.lean.meta.json\n",
    )
    .unwrap();
    git(source.path(), &["add", ".gitignore", "nodes", "trees"]);
    git(
        source.path(),
        &["commit", "-qm", "knowledge with session source"],
    );

    let machine = tempfile::tempdir().unwrap();
    let home = machine.path().join("home");
    git(
        machine.path(),
        &["clone", "-q", source.path().to_str().unwrap(), "home"],
    );
    let restored = machine.path().join("archive");
    config(&home, &restored, remote.path());

    let receipt = local::restore_session_links(&home, &restored).unwrap();
    assert_eq!(receipt["links"], 1);
    assert_eq!(git(&home, &["status", "--porcelain"]), "");
}

#[test]
fn backup_receipt_matches_the_commit_sent_before_concurrent_capture() {
    let home = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let remote = tempfile::tempdir().unwrap();
    archive_with_session(archive.path());
    git(remote.path(), &["init", "--bare", "-q", "-b", "main"]);
    config(home.path(), archive.path(), remote.path());

    let hook_entered = remote.path().join("hook-entered");
    let allow_push = remote.path().join("allow-push");
    fs::write(
        remote.path().join("hooks/pre-receive"),
        format!(
            "#!/bin/sh\ncat > '{}'\ntouch '{}'\nwhile [ ! -e '{}' ]; do sleep 0.01; done\n",
            remote.path().join("received-refs").display(),
            hook_entered.display(),
            allow_push.display()
        ),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(
        remote.path().join("hooks/pre-receive"),
        fs::Permissions::from_mode(0o755),
    )
    .unwrap();

    let (backup_tx, backup_rx) = mpsc::channel();
    let home_path = home.path().to_path_buf();
    let backup = thread::spawn(move || {
        let result = local::backup_session_archive(&home_path);
        backup_tx
            .send(result.map_err(|error| error.to_string()))
            .unwrap();
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    while !hook_entered.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(5));
    }
    assert!(hook_entered.exists(), "push did not reach pre-receive hook");

    let source_dir = tempfile::tempdir().unwrap();
    let source = source_dir.path().join("source.jsonl");
    fs::write(
        &source,
        concat!(
            "{\"type\":\"session\",\"id\":\"race-session\"}\n",
            "{\"id\":\"entry-1\",\"parentId\":null}\n"
        ),
    )
    .unwrap();
    let (capture_tx, capture_rx) = mpsc::channel();
    let source = source.clone();
    let archive_path = archive.path().to_path_buf();
    let capture = thread::spawn(move || {
        let result = local::capture_session(
            &source,
            &json!({
                "schema_version": 1,
                "harness": "pi",
                "session_id": "race-session",
                "availability": "available",
                "archive_key": "sessions/race-session.jsonl",
                "entry_ids": ["entry-1"],
                "branch_entry_ids": ["entry-1"],
                "leaf_id": "entry-1",
                "cutoff_entry_id": "entry-1"
            }),
            &archive_path,
        );
        capture_tx
            .send(result.map_err(|error| error.to_string()))
            .unwrap();
    });
    let capture_before_push = capture_rx.recv_timeout(Duration::from_secs(5)).ok();
    let capture_completed_during_push = capture_before_push.is_some();

    // Always release the remote hook before asserting so a failed test cannot strand threads.
    fs::write(&allow_push, "continue").unwrap();
    let backup_result = backup_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    backup.join().unwrap();
    let capture_result = match capture_before_push {
        Some(result) => result,
        None => capture_rx.recv_timeout(Duration::from_secs(10)).unwrap(),
    };
    capture.join().unwrap();
    assert!(
        capture_completed_during_push,
        "capture did not finish during push"
    );

    let receipt = backup_result.unwrap();
    let capture = capture_result.unwrap();
    let remote_commit = git(remote.path(), &["rev-parse", "refs/heads/main"]);
    assert_eq!(receipt["status"], "backed_up");
    assert_eq!(capture["status"], "captured");
    assert_eq!(receipt["commit"].as_str(), Some(remote_commit.as_str()));
}

#[test]
fn backup_configuration_is_not_returned_as_a_secret() {
    let home = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let remote = tempfile::tempdir().unwrap();
    config(home.path(), archive.path(), remote.path());
    let value = local::session_retention_config(home.path()).unwrap();
    assert_eq!(value["backup_enabled"], true);
    assert_eq!(value["backup_configured"], true);
    assert!(
        value
            .to_string()
            .find(remote.path().to_str().unwrap())
            .is_none()
    );
}
