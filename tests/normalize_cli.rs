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
    let receipt: Value = serde_json::from_slice(&output.stdout).unwrap();
    if receipt["status"] == "unchanged" {
        let tokens = tiktoken_rs::o200k_base_singleton()
            .lock()
            .encode_ordinary(std::str::from_utf8(&output.stdout).unwrap())
            .len();
        eprintln!("OUTPUT_TOKENS normalize_unchanged={tokens}");
    }
    receipt
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
        json!({"status":"unchanged"})
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
        assert_eq!(unchanged, json!({"status":"unchanged"}));
        assert!(
            fs::read_to_string(after.join("nodes").join(PARENT).join("node.md"))
                .unwrap()
                .contains("status: \"resolved\"")
        );
    }
    node(&after, CHILD, "in_progress");
    let reopened = normalize(&before, &after, "");
    assert_eq!(
        reopened,
        json!({"status":"normalized", "changes":[{"id":PARENT,"from":"resolved","to":"in_progress"}]})
    );
    assert_eq!(normalize(&after, &after, ""), json!({"status":"unchanged"}));

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

#[test]
fn normalize_reports_removed_reason_field_names_without_their_contents() {
    let tmp = tempfile::tempdir().unwrap();
    let before = tmp.path().join("before");
    let after = tmp.path().join("after");
    fixture(&before);
    fixture(&after);
    for (root, status) in [(&before, "stuck"), (&after, "in_progress")] {
        let path = root.join("nodes").join(PARENT).join("node.md");
        let text = fs::read_to_string(&path).unwrap().replace(
            "status: \"resolved\"",
            &format!("status: {status:?}\nblocker: \"PRIVATE_REASON_CONTENT\""),
        );
        fs::write(path, text).unwrap();
    }
    let output = cli(&[
        "normalize",
        before.to_str().unwrap(),
        after.to_str().unwrap(),
        PARENT,
    ]);
    success(&output);
    assert!(output.stderr.is_empty(), "{output:?}");
    assert!(!String::from_utf8_lossy(&output.stdout).contains("PRIVATE_REASON_CONTENT"));
    let receipt: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        receipt,
        json!({"status":"normalized", "changes":[{
        "id":PARENT,"from":"in_progress","to":"in_progress","removed_reason_fields":["blocker"]}]})
    );
    assert!(
        !fs::read_to_string(after.join("nodes").join(PARENT).join("node.md"))
            .unwrap()
            .contains("blocker:")
    );
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
    let accepted = cli(&[
        "normalize",
        before.to_str().unwrap(),
        after.to_str().unwrap(),
    ]);
    success(&accepted);
    assert!(accepted.stderr.is_empty(), "{accepted:?}");
    let receipt: Value = serde_json::from_slice(&accepted.stdout).unwrap();
    assert_eq!(
        receipt,
        json!({"status":"normalized", "changes":[
        {"id":last,"from":"not_examined","to":"in_progress"}]})
    );
    let tokens = tiktoken_rs::o200k_base_singleton()
        .lock()
        .encode_ordinary(std::str::from_utf8(&accepted.stdout).unwrap())
        .len();
    eprintln!("OUTPUT_TOKENS normalize_one_change_1025={tokens}");
    assert!(tokens < 100, "one change grew to {tokens} tokens");
    assert_eq!(normalize(&after, &after, ""), json!({"status":"unchanged"}));
    assert_eq!(Store::read(&before).unwrap().inventory, before_inventory);

    // Real large change sets, not unchanged state maps, still require preflight.
    for (path, entry) in &original {
        if path.ends_with("node.md")
            && let sifttext_file_graph::Entry::File(bytes) = entry
        {
            let mut text = String::from_utf8(bytes.clone()).unwrap();
            text.push_str("Edited\n");
            fs::write(after.join(path), text).unwrap();
        }
    }
    let edited = Store::read(&after).unwrap().inventory;
    let suppressed = cli(&[
        "normalize",
        before.to_str().unwrap(),
        after.to_str().unwrap(),
    ]);
    assert!(!suppressed.status.success(), "{suppressed:?}");
    assert!(suppressed.stderr.is_empty(), "{suppressed:?}");
    assert_eq!(Store::read(&after).unwrap().inventory, edited);
    let accepted = cli(&[
        "normalize",
        before.to_str().unwrap(),
        after.to_str().unwrap(),
        "-y",
    ]);
    success(&accepted);
    assert!(accepted.stderr.is_empty(), "{accepted:?}");
    let receipt: Value = serde_json::from_slice(&accepted.stdout).unwrap();
    assert_eq!(receipt["changes"].as_array().unwrap().len(), 1025);
    let tokens = tiktoken_rs::o200k_base_singleton()
        .lock()
        .encode_ordinary(std::str::from_utf8(&accepted.stdout).unwrap())
        .len();
    eprintln!("OUTPUT_TOKENS normalize_1025_actual_changes={tokens}");
    assert!(tokens > 15_000, "actual changes too small: {tokens}");
    assert_eq!(
        String::from_utf8_lossy(&suppressed.stdout),
        format!(
            "[{tokens} tokens suppressed] Re-run this sifttext normalize command with -y to print them.\n"
        )
    );
    assert_eq!(Store::read(&before).unwrap().inventory, before_inventory);
    assert_ne!(Store::read(&after).unwrap().inventory, edited);
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
