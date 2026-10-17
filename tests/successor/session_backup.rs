use crate::local;
use serde_json::json;
use std::{fs, path::Path, process::Command};

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
    assert!(fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());
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

#[test]
fn backup_configuration_is_not_returned_as_a_secret() {
    let home = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let remote = tempfile::tempdir().unwrap();
    config(home.path(), archive.path(), remote.path());
    let value = local::session_retention_config(home.path()).unwrap();
    assert_eq!(value["backup_enabled"], true);
    assert_eq!(value["backup_configured"], true);
    assert!(value
        .to_string()
        .find(remote.path().to_str().unwrap())
        .is_none());
}
