//! Exercise the installed CLI receipt boundary as well as its actual status writes.
use serde_json::{Value, json};
use std::{fs, os::unix::fs::symlink, path::Path, process::Command};

const PARENT: &str = "50000000-0000-4000-8000-000000000001";
const CHILD: &str = "50000000-0000-4000-8000-000000000002";

fn node(store: &Path, id: &str, status: &str) {
    let dir = store.join("nodes").join(id);
    fs::create_dir_all(dir.join("children")).unwrap();
    fs::write(dir.join("children.order"), "").unwrap();
    fs::write(
        dir.join("node.md"),
        format!("---\nid: {id:?}\nname: \"Node\"\nmicro_summary: \"Status receipt regression.\"\nstatus: {status:?}\nsee_also: []\n---\nBody\n"),
    )
    .unwrap();
}

fn normalize(before: &Path, after: &Path) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_sifttext"))
        .arg("normalize")
        .arg(before)
        .arg(after)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn normalize_reports_unchanged_and_reopened_statuses_without_render_failure() {
    let tmp = tempfile::tempdir().unwrap();
    let before = tmp.path().join("before");
    let after = tmp.path().join("after");
    for store in [&before, &after] {
        node(store, PARENT, "resolved");
        fs::create_dir(store.join("trees")).unwrap();
        symlink(
            format!("../nodes/{PARENT}"),
            store.join("trees").join(PARENT),
        )
        .unwrap();
    }
    let unchanged = normalize(&before, &after);
    assert_eq!(
        unchanged,
        json!({"status":"current", "normalized_ids":[], "nodes":{PARENT:{"status":"resolved"}}})
    );

    node(&after, CHILD, "in_progress");
    let parent = after.join("nodes").join(PARENT);
    fs::write(parent.join("children.order"), format!("{CHILD}\n")).unwrap();
    symlink(
        format!("../../{CHILD}"),
        parent.join("children").join(CHILD),
    )
    .unwrap();
    let reopened = normalize(&before, &after);
    assert_eq!(
        reopened,
        json!({"status":"current", "normalized_ids":[PARENT], "nodes":{PARENT:{"status":"in_progress"}, CHILD:{"status":"in_progress"}}})
    );
    let authored = fs::read_to_string(parent.join("node.md")).unwrap();
    assert!(authored.contains("status: \"in_progress\""));
    assert!(!authored.contains("status: \"resolved\""));
    assert_eq!(normalize(&after, &after)["normalized_ids"], json!([]));
}
