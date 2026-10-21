//! Exercise real CLI notices, candidate validation, and refusal nonmutation.
use serde_json::{Value, json};
use sifttext_file_graph::{Store, outlines::decode_lean};
use std::{
    fs,
    os::unix::fs::symlink,
    path::Path,
    process::{Command, Output},
};

const PARENT: &str = "50000000-0000-4000-8000-000000000001";
const CHILD: &str = "50000000-0000-4000-8000-000000000002";

fn node(store: &Path, id: &str, status: &str) {
    let dir = store.join("nodes").join(id);
    fs::create_dir_all(dir.join("children")).unwrap();
    fs::write(dir.join("children/.keep"), "").unwrap();
    fs::write(dir.join("children.order"), "").unwrap();
    fs::write(
        dir.join("node.md"),
        format!("---\nid: {id:?}\nname: \"Node\"\nmicro_summary: \"Status receipt regression.\"\nstatus: {status:?}\nsee_also: []\n---\nBody\n"),
    )
    .unwrap();
}

fn link(store: &Path, parent: &str, child: &str) {
    let dir = store.join("nodes").join(parent);
    fs::write(dir.join("children.order"), format!("{child}\n")).unwrap();
    symlink(format!("../../{child}"), dir.join("children").join(child)).unwrap();
}

fn fixture(store: &Path) {
    node(store, PARENT, "resolved");
    fs::create_dir(store.join("trees")).unwrap();
    symlink(
        format!("../nodes/{PARENT}"),
        store.join("trees").join(PARENT),
    )
    .unwrap();
}

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sifttext"))
        .args(args)
        .env("SIFTTEXT_NO_UPDATE", "1")
        .output()
        .unwrap()
}

fn success(output: &Output) {
    assert!(output.status.success(), "{output:?}");
}

fn normalize(before: &Path, after: &Path, notice: &str) -> Value {
    let output = cli(&[
        "normalize",
        before.to_str().unwrap(),
        after.to_str().unwrap(),
    ]);
    success(&output);
    assert_eq!(String::from_utf8_lossy(&output.stderr), notice);
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn normalize_reports_only_actual_changes_and_refuses_resolution_without_mutation() {
    let tmp = tempfile::tempdir().unwrap();
    let before = tmp.path().join("before");
    let after = tmp.path().join("after");
    fixture(&before);
    fixture(&after);
    assert_eq!(
        normalize(&before, &after, ""),
        json!({"status":"current", "normalized_ids":[], "reopened_ids":[], "nodes":{PARENT:{"status":"resolved"}}})
    );

    // Neither inactive state alone reopens a resolved parent.
    for (status, reason) in [
        ("discarded", "discard_reason"),
        ("deferred", "deferred_reason"),
    ] {
        node(&after, CHILD, status);
        let path = after.join("nodes").join(CHILD).join("node.md");
        let text = fs::read_to_string(&path).unwrap().replace(
            "see_also: []",
            &format!("{reason}: \"Not active\"\nsee_also: []"),
        );
        fs::write(path, text).unwrap();
        if !after
            .join("nodes")
            .join(PARENT)
            .join("children")
            .join(CHILD)
            .exists()
        {
            link(&after, PARENT, CHILD);
        }
        let unchanged = normalize(&before, &after, "");
        assert_eq!(unchanged["reopened_ids"], json!([]));
        assert_eq!(unchanged["nodes"][PARENT]["status"], "resolved");
    }
    node(&after, CHILD, "in_progress");
    let reopened = normalize(
        &before,
        &after,
        &format!("[{PARENT}] set to in_progress.\n"),
    );
    assert_eq!(
        reopened,
        json!({"status":"current", "normalized_ids":[PARENT], "reopened_ids":[PARENT], "nodes":{PARENT:{"status":"in_progress"}, CHILD:{"status":"in_progress"}}})
    );
    assert_eq!(normalize(&after, &after, "")["reopened_ids"], json!([]));

    // Explicit resolution with an active child must fail without touching either store.
    let parent = after.join("nodes").join(PARENT).join("node.md");
    let text = fs::read_to_string(&parent)
        .unwrap()
        .replace("status: \"in_progress\"", "status: \"resolved\"");
    fs::write(parent, text).unwrap();
    let base = Store::read(&before).unwrap().inventory;
    let candidate = Store::read(&after).unwrap().inventory;
    let refused = cli(&[
        "normalize",
        before.to_str().unwrap(),
        after.to_str().unwrap(),
        PARENT,
    ]);
    assert!(!refused.status.success());
    assert!(
        String::from_utf8_lossy(&refused.stderr).contains("active direct child blocks resolution")
    );
    assert!(!String::from_utf8_lossy(&refused.stderr).contains("set to in_progress"));
    assert!(refused.stdout.is_empty());
    assert_eq!(Store::read(&before).unwrap().inventory, base);
    assert_eq!(Store::read(&after).unwrap().inventory, candidate);
}

fn git(repo: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args([
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .output()
        .unwrap();
    success(&output);
}

#[test]
fn normalize_suppresses_large_receipt_before_mutation_and_y_retrieves_it() {
    let tmp = tempfile::tempdir().unwrap();
    let before = tmp.path().join("before");
    let after = tmp.path().join("after");
    fixture(&before);
    fixture(&after);
    for root in [&before, &after] {
        let path = root.join("nodes").join(PARENT).join("node.md");
        let text = fs::read_to_string(&path)
            .unwrap()
            .replace("status: \"resolved\"", "status: \"not_examined\"");
        fs::write(path, text).unwrap();
    }
    let mut previous = PARENT.to_owned();
    let mut last = String::new();
    for index in 1..=1024 {
        let id = format!("60000000-0000-4000-8000-{index:012}");
        node(&before, &id, "not_examined");
        node(&after, &id, "not_examined");
        link(&before, &previous, &id);
        link(&after, &previous, &id);
        previous = id.clone();
        last = id;
    }
    let last_path = after.join("nodes").join(&last).join("node.md");
    let mut edited = fs::read_to_string(&last_path).unwrap();
    edited.push(' ');
    fs::write(last_path, edited).unwrap();

    let before_inventory = Store::read(&before).unwrap().inventory;
    let original = Store::read(&after).unwrap().inventory;
    let suppressed = cli(&[
        "normalize",
        before.to_str().unwrap(),
        after.to_str().unwrap(),
    ]);
    assert!(!suppressed.status.success());
    assert!(
        String::from_utf8_lossy(&suppressed.stderr).is_empty(),
        "{suppressed:?}"
    );
    assert_eq!(Store::read(&before).unwrap().inventory, before_inventory);
    assert_eq!(Store::read(&after).unwrap().inventory, original);

    let accepted = cli(&[
        "normalize",
        before.to_str().unwrap(),
        after.to_str().unwrap(),
        "-y",
    ]);
    success(&accepted);
    assert_eq!(
        String::from_utf8_lossy(&accepted.stderr),
        format!("[{last}] set to in_progress.\n")
    );
    let receipt: Value = serde_json::from_slice(&accepted.stdout).unwrap();
    assert_eq!(receipt["normalized_ids"], json!([last]));
    assert_eq!(receipt["reopened_ids"], json!([last]));
    let tokens = tiktoken_rs::o200k_base_singleton()
        .lock()
        .encode_ordinary(std::str::from_utf8(&accepted.stdout).unwrap())
        .len();
    assert!(tokens > 15_000, "receipt rendered as {tokens} tokens");
    let notice = format!(
        "[{} tokens suppressed] Re-run this sifttext normalize command with -y to print them.\n",
        tokens
    );
    assert_eq!(String::from_utf8_lossy(&suppressed.stdout), notice);
    assert_ne!(Store::read(&after).unwrap().inventory, original);

    let normalized = after.join("nodes").join(last).join("node.md");
    assert!(
        fs::read_to_string(normalized)
            .unwrap()
            .contains(r#"status: "in_progress""#)
    );
}

#[test]
fn create_reopens_resolved_chain_and_keeps_lean_receipt_usable() {
    let tmp = tempfile::tempdir().unwrap();
    let repo = tmp.path().join("repo");
    fixture(&repo);
    node(&repo, CHILD, "resolved");
    link(&repo, PARENT, CHILD);
    git(&repo, &["init", "-b", "main"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "fixture: resolved chain"]);
    let draft = tmp.path().join("draft");
    git(
        &repo,
        &["worktree", "add", "--detach", draft.to_str().unwrap()],
    );
    let output = cli(&[
        "create",
        draft.to_str().unwrap(),
        "New finding",
        "Active child",
        CHILD,
    ]);
    success(&output);
    assert_eq!(
        String::from_utf8_lossy(&output.stderr),
        format!("[{PARENT}] set to in_progress.\n[{CHILD}] set to in_progress.\n")
    );
    let receipt = decode_lean(std::str::from_utf8(&output.stdout).unwrap()).unwrap();
    let id = receipt["id"].as_str().unwrap();
    assert_eq!(receipt["parent"], CHILD);
    assert_eq!(receipt["committed"], false);
    assert_eq!(
        receipt["path"],
        fs::canonicalize(&draft)
            .unwrap()
            .join("nodes")
            .join(id)
            .join("node.md")
            .to_str()
            .unwrap()
    );
    assert_eq!(Store::read(&draft).unwrap().children[CHILD], [id]);
    for ancestor in [PARENT, CHILD] {
        assert!(
            fs::read_to_string(draft.join("nodes").join(ancestor).join("node.md"))
                .unwrap()
                .contains("status: \"in_progress\"")
        );
    }
    let checked = cli(&["check", draft.to_str().unwrap(), repo.to_str().unwrap()]);
    success(&checked);
    assert!(checked.stderr.is_empty());

    let again = cli(&[
        "create",
        draft.to_str().unwrap(),
        "Another finding",
        "No repeated notices",
        CHILD,
    ]);
    success(&again);
    assert!(again.stderr.is_empty(), "{again:?}");
    let original = Store::read(&draft).unwrap().inventory;
    let refused = cli(&["create", draft.to_str().unwrap(), "", "Invalid name", CHILD]);
    assert!(!refused.status.success());
    assert!(!String::from_utf8_lossy(&refused.stderr).contains("set to in_progress"));
    assert_eq!(Store::read(&draft).unwrap().inventory, original);
}
