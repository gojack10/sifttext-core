#[path = "../fixtures/check_fixture.rs"]
mod fixture_contract;
#[path = "../fixtures/peer_demo.rs"]
mod peer_demo;

#[test]
fn manual_drafts_match_independent_authored_expectations_without_native_mutation() {
    use std::{fs, path::PathBuf, process::Command};
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let temp = tempfile::tempdir().unwrap();
    let call = |store: &std::path::Path, case: &str, success: bool| {
        let out = Command::new(env!("CARGO_BIN_EXE_peer-fixture"))
            .args(["manual-draft", store.to_str().unwrap(), case])
            .output()
            .unwrap();
        assert_eq!(
            out.status.success(),
            success,
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    for case in ["move-b-under-a", "rename-b"] {
        let store = temp.path().join(case);
        call(&store, "unknown", false);
        assert!(!store.exists());
        call(&store, case, true);
        // Check all authored bytes and links against the independent declarative
        // expectations; no native inspect/move/rename call supplies these answers.
        let want: serde_json::Value = serde_json::from_slice(
            &fs::read(
                project
                    .join("fixtures/expected")
                    .join(format!("{case}.json")),
            )
            .unwrap(),
        )
        .unwrap();
        for node in want["nodes"].as_array().unwrap() {
            let path = store.join("nodes").join(node["id"].as_str().unwrap());
            let markdown = fs::read_to_string(path.join("node.md")).unwrap();
            assert_eq!(
                markdown,
                format!(
                    "---\nid: {}\nname: {}\nmicro_summary: {}\nsee_also: {}\n---\n{}",
                    node["id"],
                    node["name"],
                    node["micro_summary"],
                    node["see_also"],
                    node["content"].as_str().unwrap()
                )
            );
            let children = node["children"].as_array().unwrap();
            assert_eq!(
                fs::read_to_string(path.join("children.order")).unwrap(),
                children
                    .iter()
                    .map(|id| format!("{}\n", id.as_str().unwrap()))
                    .collect::<String>()
            );
            assert_eq!(
                fs::read_dir(path.join("children")).unwrap().count(),
                children.len() + 1
            );
            for child in children {
                let id = child.as_str().unwrap();
                assert_eq!(
                    fs::read_link(path.join("children").join(id)).unwrap(),
                    PathBuf::from(format!("../../{id}"))
                );
            }
        }
        for sentinel in want["sentinels"].as_array().unwrap() {
            assert_eq!(
                fs::read(store.join(sentinel["path"].as_str().unwrap())).unwrap(),
                sentinel["content"].as_str().unwrap().as_bytes()
            );
        }
        call(&store, case, false); // Exclusive creation cannot overwrite a draft.
        std::os::unix::fs::symlink(&store, temp.path().join("alias")).unwrap();
        call(&temp.path().join("alias"), case, false);
        fs::remove_file(temp.path().join("alias")).unwrap();
    }
}

#[test]
fn manual_utility_materializes_checks_and_refuses_overwrite() {
    use std::{fs, path::PathBuf, process::Command};

    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let native_project = std::env::var_os("SIFTTEXT_PROJECT")
        .map(PathBuf::from)
        .unwrap_or_else(|| project.clone());
    let native = std::env::var_os("SIFTTEXT_TEST_NATIVE_BIN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| native_project.join("target/debug"))
        .join("sifttext-file-graph");
    let fixture: serde_json::Value =
        serde_json::from_slice(&fs::read(project.join("fixtures/fixture-contract.json")).unwrap())
            .unwrap();
    let temp = tempfile::tempdir().unwrap();
    let utility = env!("CARGO_BIN_EXE_peer-fixture");
    let call = |args: &[&str], success: bool| {
        let out = Command::new(utility)
            .current_dir(temp.path())
            .args(args)
            .output()
            .unwrap();
        assert_eq!(
            out.status.success(),
            success,
            "{args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    for (case, operation) in [
        ("before", None),
        (
            "move-b-under-a",
            Some(("move", fixture["labels"]["A"].as_str().unwrap())),
        ),
        (
            "reorder-b-after-c",
            Some(("reorder", fixture["labels"]["C"].as_str().unwrap())),
        ),
        ("rename-b", Some(("rename", "Renamed B"))),
    ] {
        call(&["materialize", case], true); // Relative input, including macOS /var spelling.
        let store = temp.path().join(case).canonicalize().unwrap();
        if let Some((kind, argument)) = operation {
            let renamed: serde_json::Value = serde_json::from_slice(
                &fs::read(project.join("fixtures/expected/rename-b.json")).unwrap(),
            )
            .unwrap();
            let argument = if kind == "rename" {
                renamed["transition"]["new_display_name"].as_str().unwrap()
            } else {
                argument
            };
            let out = Command::new(&native)
                .args([
                    kind,
                    store.to_str().unwrap(),
                    fixture["labels"]["B"].as_str().unwrap(),
                    argument,
                ])
                .output()
                .unwrap();
            assert!(
                out.status.success(),
                "{}",
                String::from_utf8_lossy(&out.stderr)
            );
        }
        call(&["check", case, case], true);
        let node = store
            .join("nodes")
            .join(fixture["labels"]["A"].as_str().unwrap())
            .join("node.md");
        let original = fs::read(&node).unwrap();
        call(&["materialize", case], false);
        assert_eq!(fs::read(&node).unwrap(), original);
        let sentinel = store.join(fixture["sentinels"][0]["path"].as_str().unwrap());
        fs::write(&sentinel, "tampered\n").unwrap();
        call(&["check", case, case], false);
        assert_eq!(fs::read(&sentinel).unwrap(), b"tampered\n");
    }
    for args in [
        vec![],
        vec!["check", "before", "unknown"],
        vec!["materialize", "unused", "extra"],
        vec!["unknown", "unused"],
    ] {
        call(&args, false);
    }
    assert!(!temp.path().join("unused").exists());
    // An existing link must not redirect exclusive creation into someone else's store.
    std::os::unix::fs::symlink(temp.path().join("before"), temp.path().join("alias")).unwrap();
    call(&["materialize", "alias"], false);
}
