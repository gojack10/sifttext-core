use crate::{
    Policy, Store, asset_bytes,
    checked_publication::{self, Request},
    local,
};
use serde_json::json;
use std::{
    collections::BTreeSet,
    fs,
    path::Path,
    process::Command,
    sync::{Arc, Barrier},
    thread,
};

const NODE: &str = "40000000-0000-4000-8000-000000000001";

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

fn fixture(root: &Path) {
    let node = root.join("nodes").join(NODE);
    fs::create_dir_all(node.join("children")).unwrap();
    fs::create_dir_all(root.join("trees")).unwrap();
    fs::write(node.join("children.order"), "").unwrap();
    fs::write(node.join("children/.keep"), "").unwrap();
    fs::write(
        node.join("node.md"),
        format!(
            "---\nid: \"{NODE}\"\nname: \"Root\"\nmicro_summary: \"Root node\"\nstatus: \"in_progress\"\nsee_also: []\n---\nRoot body\n"
        ),
    )
    .unwrap();
    std::os::unix::fs::symlink(format!("../nodes/{NODE}"), root.join("trees").join(NODE)).unwrap();
}

#[test]
fn chat_links_are_local_and_never_change_the_tracked_tree_snapshot() {
    let home = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    fixture(home.path());
    let archived_session = archive.path().join("session.jsonl");
    fs::write(&archived_session, "synthetic session\n").unwrap();
    let chats = home.path().join(format!("nodes/{NODE}/artifacts/chats"));
    fs::create_dir_all(&chats).unwrap();
    std::os::unix::fs::symlink(&archived_session, chats.join("pi-synthetic-topic.jsonl")).unwrap();
    fs::write(
        home.path()
            .join(format!("nodes/{NODE}/session-sources.json")),
        serde_json::to_vec(&json!([{
            "schema_version": 1,
            "harness": "pi",
            "session_id": "00000000-0000-4000-8000-000000000001",
            "availability": "available",
            "archive_key": "sessions/00000000-0000-4000-8000-000000000001.jsonl",
            "entry_ids": ["entry-1", "entry-2", "entry-3"],
            "branch_entry_ids": ["entry-1", "entry-3"],
            "leaf_id": "entry-3",
            "cutoff_entry_id": "entry-3"
        }, {
            "schema_version": 1,
            "harness": "codex",
            "session_id": "codex-full-session-id",
            "availability": "available",
            "archive_key": "sessions/codex-full-session-id.jsonl",
            "entry_ids": ["entry-a"],
            "branch_entry_ids": ["entry-a"],
            "leaf_id": "entry-a",
            "cutoff_entry_id": "entry-a"
        }]))
        .unwrap(),
    )
    .unwrap();

    let attached = Store::read(home.path()).unwrap();
    let tracked = asset_bytes(&attached);
    assert!(
        attached
            .inventory
            .contains_key(Path::new(&format!("nodes/{NODE}/session-sources.json")))
    );
    assert!(
        !attached
            .inventory
            .keys()
            .any(|path| path.to_string_lossy().contains("artifacts/chats"))
    );

    fs::remove_file(&archived_session).unwrap();
    let archive_missing = Store::read(home.path()).unwrap();
    assert_eq!(asset_bytes(&archive_missing), tracked);
    assert!(
        fs::symlink_metadata(chats.join("pi-synthetic-topic.jsonl"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
}

#[test]
fn attach_and_restore_rebuilds_link_after_archive_relocation() {
    let home = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    let relocated = tempfile::tempdir().unwrap();
    fixture(home.path());
    // Relative input through a parent alias remains a genuine standalone root;
    // slash/dot suffixes must not be rejected wholesale.
    let alias_dir = tempfile::tempdir_in(".").unwrap();
    let parent_alias = alias_dir.path().join("parent");
    std::os::unix::fs::symlink(home.path().parent().unwrap(), &parent_alias).unwrap();
    let relative_home = parent_alias.strip_prefix(std::env::current_dir().unwrap()).unwrap()
        .join(home.path().file_name().unwrap());
    assert!(!relative_home.is_absolute());
    let key = "sessions/session-1.jsonl";
    let source = archive.path().join(key);
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, "synthetic session\n").unwrap();
    let descriptor = json!({
        "schema_version": 1,
        "harness": "pi",
        "session_id": "session-1",
        "availability": "available",
        "archive_key": key,
        "entry_ids": ["entry-1"],
        "branch_entry_ids": ["entry-1"],
        "leaf_id": "entry-1",
        "cutoff_entry_id": "entry-1"
    });
    let attached = local::attach_session(
        Path::new(&format!("{}/", relative_home.display())),
        NODE,
        &descriptor,
        "sample topic",
        archive.path(),
    )
    .unwrap();
    let name = attached["attachment_name"].as_str().unwrap();
    let chats = home.path().join(format!("nodes/{NODE}/artifacts/chats"));
    assert_eq!(
        fs::read_link(chats.join(name)).unwrap(),
        fs::canonicalize(&source).unwrap()
    );
    fs::remove_file(chats.join(name)).unwrap();
    let moved = relocated.path().join(key);
    fs::create_dir_all(moved.parent().unwrap()).unwrap();
    fs::write(&moved, "synthetic session\n").unwrap();
    assert_eq!(
        local::restore_session_links(Path::new(&format!("{}/.", relative_home.display())), relocated.path()).unwrap()["links"],
        1
    );
    assert_eq!(
        fs::read_link(chats.join(name)).unwrap(),
        fs::canonicalize(&moved).unwrap()
    );
    assert_eq!(
        fs::read_to_string(chats.join(name)).unwrap(),
        "synthetic session\n"
    );
}

#[test]
fn attach_refuses_traversal_in_existing_attachment_name() {
    let home = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    fixture(home.path());
    let key = "sessions/session-1.jsonl";
    let source = archive.path().join(key);
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, "synthetic session\\n").unwrap();
    let mut descriptor = json!({
        "schema_version": 1,
        "harness": "pi",
        "session_id": "session-1",
        "availability": "available",
        "archive_key": key,
        "entry_ids": ["entry-1"],
        "branch_entry_ids": ["entry-1"],
        "leaf_id": "entry-1",
        "cutoff_entry_id": "entry-1"
    });
    descriptor["attachment_name"] = json!("../escape.jsonl");
    fs::write(
        home.path()
            .join(format!("nodes/{NODE}/session-sources.json")),
        serde_json::to_vec(&json!([descriptor])).unwrap(),
    )
    .unwrap();

    assert!(
        local::attach_session(
            home.path(),
            NODE,
            &descriptor,
            "sample topic",
            archive.path()
        )
        .is_err()
    );
    assert!(
        !home
            .path()
            .join(format!("nodes/{NODE}/artifacts/escape.jsonl"))
            .exists()
    );
}

#[test]
fn one_archived_session_can_be_attached_to_multiple_nodes() {
    const SECOND: &str = "40000000-0000-4000-8000-000000000002";
    let home = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    fixture(home.path());
    let second = home.path().join("nodes").join(SECOND);
    fs::create_dir_all(second.join("children")).unwrap();
    fs::write(second.join("children.order"), "").unwrap();
    fs::write(second.join("children/.keep"), "").unwrap();
    fs::write(
        second.join("node.md"),
        format!(
            "---\nid: \"{SECOND}\"\nname: \"Second\"\nmicro_summary: \"Second node\"\nstatus: \"in_progress\"\nsee_also: []\n---\nSecond body\n"
        ),
    )
    .unwrap();
    std::os::unix::fs::symlink(
        format!("../nodes/{SECOND}"),
        home.path().join("trees").join(SECOND),
    )
    .unwrap();
    let key = "sessions/session-1.jsonl";
    let source = archive.path().join(key);
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(&source, "synthetic session\n").unwrap();
    let descriptor = json!({
        "schema_version": 1,
        "harness": "pi",
        "session_id": "session-1",
        "availability": "available",
        "archive_key": key,
        "entry_ids": ["entry-1"],
        "branch_entry_ids": ["entry-1"],
        "leaf_id": "entry-1",
        "cutoff_entry_id": "entry-1"
    });

    for node in [NODE, SECOND] {
        assert_eq!(
            local::attach_session(home.path(), node, &descriptor, "topic", archive.path()).unwrap()
                ["status"],
            "attached"
        );
    }
    let first_sidecar: serde_json::Value = serde_json::from_slice(
        &fs::read(
            home.path()
                .join(format!("nodes/{NODE}/session-sources.json")),
        )
        .unwrap(),
    )
    .unwrap();
    let second_sidecar: serde_json::Value = serde_json::from_slice(
        &fs::read(
            home.path()
                .join(format!("nodes/{SECOND}/session-sources.json")),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(first_sidecar.as_array().unwrap().len(), 1);
    assert_eq!(second_sidecar.as_array().unwrap().len(), 1);
    assert_eq!(
        first_sidecar[0]["archive_key"],
        second_sidecar[0]["archive_key"]
    );
    let first_link = home
        .path()
        .join(format!("nodes/{NODE}/artifacts/chats/pi-topic.jsonl"));
    let second_link = home
        .path()
        .join(format!("nodes/{SECOND}/artifacts/chats/pi-topic.jsonl"));
    assert_eq!(
        fs::read_link(first_link).unwrap(),
        fs::canonicalize(&source).unwrap()
    );
    assert_eq!(
        fs::read_link(second_link).unwrap(),
        fs::canonicalize(&source).unwrap()
    );
}

#[test]
fn same_topic_sessions_get_distinct_links_without_overwriting() {
    let home = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    fixture(home.path());

    let mut names = Vec::new();
    for (session_id, contents) in [("session-a", "first\n"), ("session-b", "second\n")] {
        let key = format!("sessions/{session_id}.jsonl");
        let source = archive.path().join(&key);
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, contents).unwrap();
        let descriptor = json!({
            "schema_version": 1,
            "harness": "pi",
            "session_id": session_id,
            "availability": "available",
            "archive_key": key,
            "entry_ids": ["entry-1"],
            "branch_entry_ids": ["entry-1"],
            "leaf_id": "entry-1",
            "cutoff_entry_id": "entry-1"
        });
        let result = local::attach_session(home.path(), NODE, &descriptor, "same topic", archive.path())
            .unwrap();
        names.push(result["attachment_name"].as_str().unwrap().to_owned());
    }

    assert_ne!(names[0], names[1]);
    let chats = home.path().join(format!("nodes/{NODE}/artifacts/chats"));
    let mut contents: Vec<_> = names
        .iter()
        .map(|name| fs::read_to_string(chats.join(name)).unwrap())
        .collect();
    contents.sort();
    assert_eq!(contents, ["first\n", "second\n"]);
    let records: serde_json::Value = serde_json::from_slice(
        &fs::read(home.path().join(format!("nodes/{NODE}/session-sources.json"))).unwrap(),
    )
    .unwrap();
    assert_eq!(records.as_array().unwrap().len(), 2);
}

#[test]
fn concurrent_same_node_attachments_preserve_every_record_and_link() {
    let home = tempfile::tempdir().unwrap();
    let archive = tempfile::tempdir().unwrap();
    fixture(home.path());
    git(home.path(), &["init", "-q", "-b", "main"]);
    let count = 8;
    let start = Arc::new(Barrier::new(count));
    let workers: Vec<_> = (0..count)
        .map(|index| {
            let key = format!("sessions/session-{index}.jsonl");
            let source = archive.path().join(&key);
            fs::create_dir_all(source.parent().unwrap()).unwrap();
            fs::write(&source, format!("session-{index}\\n")).unwrap();
            let descriptor = json!({
                "schema_version": 1,
                "harness": "pi",
                "session_id": format!("session-{index}"),
                "availability": "available",
                "archive_key": key,
                "entry_ids": ["entry-1"],
                "branch_entry_ids": ["entry-1"],
                "leaf_id": "entry-1",
                "cutoff_entry_id": "entry-1"
            });
            let home = home.path().to_owned();
            let archive = archive.path().to_owned();
            let start = Arc::clone(&start);
            thread::spawn(move || {
                start.wait();
                local::attach_session(&home, NODE, &descriptor, "same topic", &archive).unwrap();
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }

    let node = home.path().join(format!("nodes/{NODE}"));
    let records: serde_json::Value =
        serde_json::from_slice(&fs::read(node.join("session-sources.json")).unwrap()).unwrap();
    assert_eq!(records.as_array().unwrap().len(), count);
    for record in records.as_array().unwrap() {
        let name = record["attachment_name"].as_str().unwrap();
        let link = node.join("artifacts/chats").join(name);
        assert!(
            fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert_eq!(
            fs::read_to_string(link).unwrap(),
            format!("{}\\n", record["session_id"].as_str().unwrap())
        );
    }
}

#[test]
fn unavailable_session_is_recorded_without_archive_or_link() {
    let home = tempfile::tempdir().unwrap();
    let absent_archive = home.path().join("not-created");
    fixture(home.path());
    let descriptor = json!({
        "schema_version": 1,
        "harness": "pi",
        "session_id": null,
        "availability": "unavailable",
        "archive_key": null,
        "unavailable_reason": "source_file_unavailable"
    });
    let result = local::attach_session(
        home.path(),
        NODE,
        &descriptor,
        "sample topic",
        &absent_archive,
    )
    .unwrap();
    assert_eq!(result["status"], "source_unavailable");
    let sidecar = home
        .path()
        .join(format!("nodes/{NODE}/session-sources.json"));
    let bytes = fs::read(&sidecar).unwrap();
    assert!(bytes.ends_with(b"\n"));
    let records: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(records[0]["availability"], "unavailable");
    assert!(
        !home
            .path()
            .join(format!("nodes/{NODE}/artifacts/chats"))
            .exists()
    );
    assert!(!absent_archive.exists());
}

#[test]
fn checked_publication_keeps_source_descriptors_but_not_local_chat_links() {
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path().join("repo");
    let draft = temp.path().join("draft");
    fs::create_dir(&repo).unwrap();
    fs::write(repo.join(".gitignore"), ".sifttext/\n*.lean\n*.meta.json\n").unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "baseline"]);
    let base = git(&repo, &["rev-parse", "HEAD"]);
    git(&repo, &["checkout", "--detach", "-q", &base]);
    git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "session-attachment",
            draft.to_str().unwrap(),
            &base,
        ],
    );

    fixture(&draft);
    let source_descriptor = json!({
        "schema_version": 1,
        "harness": "pi",
        "session_id": "00000000-0000-4000-8000-000000000001",
        "availability": "available",
        "archive_key": "sessions/00000000-0000-4000-8000-000000000001.jsonl",
        "entry_ids": ["entry-1"],
        "branch_entry_ids": ["entry-1"],
        "leaf_id": "entry-1",
        "cutoff_entry_id": "entry-1"
    });
    let descriptor_path = format!("nodes/{NODE}/session-sources.json");
    let archive_root = temp.path().join("archive");
    let archive = archive_root.join(source_descriptor["archive_key"].as_str().unwrap());
    fs::create_dir_all(archive.parent().unwrap()).unwrap();
    fs::write(&archive, "synthetic source\n").unwrap();
    local::attach_session(&draft, NODE, &source_descriptor, "synthetic topic", &archive_root).unwrap();
    let descriptor = fs::read(draft.join(&descriptor_path)).unwrap();
    let chats = draft.join(format!("nodes/{NODE}/artifacts/chats"));
    let exclude = git(
        &draft,
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-path",
            "info/exclude",
        ],
    );
    fs::write(exclude, "/nodes/*/artifacts/chats/\n").unwrap();
    let policy = Policy::checked(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap();
    local::create(&policy, &draft, "Child", "Synthetic child", Some(NODE)).unwrap();
    assert_eq!(fs::read(draft.join(&descriptor_path)).unwrap(), descriptor);
    git(&draft, &["add", "nodes", "trees"]);
    assert!(git(&draft, &["ls-files", "--", "nodes"]).contains(&descriptor_path));
    git(&draft, &["commit", "-qm", "attach source descriptor"]);
    let commit = git(&draft, &["rev-parse", "HEAD"]);
    let result = checked_publication::submit(
        &policy,
        &Request {
            repo: repo.clone(),
            accepted_ref: "refs/heads/main".into(),
            draft: Some(draft.clone()),
            commit: Some(commit),
            base: Some(base),
            explicit: BTreeSet::new(),
            pause_before_publish: None,
            pause_after_publish: None,
        },
    );
    assert_eq!(result.status, "current", "{result:?}");
    let view = result.view.unwrap();
    assert_eq!(fs::read(view.join(&descriptor_path)).unwrap(), descriptor);
    assert!(!view.join(format!("nodes/{NODE}/artifacts/chats")).exists());
    assert_eq!(fs::read(&archive).unwrap(), b"synthetic source\n");
    assert!(
        fs::symlink_metadata(chats.join("pi-synthetic-topic.jsonl"))
            .unwrap()
            .file_type()
            .is_symlink()
    );
}
