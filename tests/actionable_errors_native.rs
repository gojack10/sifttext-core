//! Real customer CLI: legacy marker compatibility and safe shared capture refusals.
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::symlink,
    path::{Path, PathBuf},
    process::{Command, Output},
};
const ENTRY: &str = "5e565245-14fe-406c-977c-cf94a9be246b";
const SECRET: &str = "PRIVATE_AUTHORED_VALUE_DO_NOT_PRINT";
fn snapshot(root: &Path) -> BTreeMap<PathBuf, Vec<u8>> {
    fn walk(root: &Path, path: &Path, out: &mut BTreeMap<PathBuf, Vec<u8>>) {
        for entry in fs::read_dir(path).unwrap() {
            let p = entry.unwrap().path();
            let m = fs::symlink_metadata(&p).unwrap();
            if m.is_dir() {
                walk(root, &p, out);
            } else {
                let bytes = if m.file_type().is_symlink() {
                    fs::read_link(&p)
                        .unwrap()
                        .as_os_str()
                        .as_encoded_bytes()
                        .to_vec()
                } else {
                    fs::read(&p).unwrap()
                };
                out.insert(p.strip_prefix(root).unwrap().to_owned(), bytes);
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
}
fn cli(home: &Path, args: &[&str]) -> Output {
    let bin = std::env::var_os("SIFTTEXT_TEST_NATIVE_BIN_DIR")
        .map(|p| PathBuf::from(p).join("sifttext"))
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_sifttext")));
    Command::new(bin)
        .args(args)
        .env("HOME", home)
        .env("SIFTTEXT_NO_UPDATE", "1")
        .env("GIT_CONFIG_GLOBAL", home.join(".gitconfig"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap()
}
fn refusal(home: &Path, store: &Path, args: &[&str], expected: &[&str]) {
    let before = snapshot(store);
    let out = cli(home, args);
    assert!(!out.status.success(), "{out:?}");
    assert_eq!(snapshot(store), before, "refusal mutated knowledge");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    for fragment in expected {
        assert!(text.contains(fragment), "missing {fragment:?}: {text}");
    }
    for forbidden in [
        SECRET,
        "commit_rule",
        "decomposition_rule",
        "tokenizer_version",
        "encoding:",
        "graph_reason",
        "graph_mode",
        "e8234ea5-3467-4f2b-8ca2-2bcc5cc80d11",
        "capture_or_graph_invalid",
        "crystallization_tokens",
        "before_version:_",
    ] {
        assert!(!text.contains(forbidden), "leaked {forbidden}: {text}");
    }
}
#[test]
fn legacy_marker_and_shared_capture_errors_are_actionable_and_read_only() {
    let temp = tempfile::tempdir().unwrap();
    let home = if let Some(path) = std::env::var_os("SIFTTEXT_TEST_EVIDENCE") {
        fs::create_dir(&path).unwrap();
        fs::canonicalize(path).unwrap()
    } else {
        fs::canonicalize(temp.path()).unwrap()
    };
    fs::write(home.join(".gitconfig"), "[user]\nname = Synthetic Fixture\nemail = fixture@example.invalid\n[commit]\ngpgSign = false\n").unwrap();
    let store = home.join("sifttext");
    let store_arg = store.to_str().unwrap();
    let out = cli(&home, &["init", store_arg]);
    assert!(out.status.success(), "{out:?}");
    let entry = store.join(format!("nodes/{ENTRY}/node.md"));
    let original = fs::read_to_string(&entry).unwrap();

    // Same pre-parser dynamic-directory guard that used to produce unknown/invalid graph.
    let stray = store.join("nodes/stray.tmp");
    fs::write(&stray, SECRET).unwrap();
    refusal(
        &home,
        &store,
        &["instructions"],
        &[
            "unsafe_filesystem",
            "nodes/stray.tmp",
            "real directory",
            "next_action",
        ],
    );
    fs::remove_file(stray).unwrap();

    // The legacy empty marker remains present and is not interpreted as a node.
    fs::write(store.join("nodes/.keep"), "").unwrap();
    let before = snapshot(&store);
    for args in [
        vec!["instructions"],
        vec!["check", store_arg],
        vec!["read", store_arg, ENTRY],
    ] {
        let out = cli(&home, &args);
        assert!(out.status.success(), "{args:?}: {out:?}");
        if args[0] == "instructions" {
            assert!(String::from_utf8_lossy(&out.stdout).contains(entry.to_str().unwrap()));
        }
        assert_eq!(snapshot(&store), before);
    }
    let indexed = cli(&home, &["index", store_arg]);
    assert!(indexed.status.success(), "{indexed:?}");
    let malformed = original.replace(
        "name: \"Local Core Agent Entrypoint\"",
        &format!("name: {SECRET}"),
    );
    assert_ne!(malformed, original);
    fs::write(&entry, malformed).unwrap();
    let node_path = format!("nodes/{ENTRY}/node.md");
    for args in [
        vec!["instructions"],
        vec!["check", store_arg],
        vec!["read", store_arg, ENTRY],
    ] {
        refusal(
            &home,
            &store,
            &args,
            &[
                "invalid_capture",
                &node_path,
                "field:name",
                "JSON",
                "next_action",
            ],
        );
    }
    for args in [
        vec!["index", store_arg],
        vec!["search", store_arg, "entry"],
        vec!["sql", store_arg, "SELECT id FROM nodes LIMIT 1"],
        vec!["outline", store_arg, ENTRY],
    ] {
        refusal(
            &home,
            &store,
            &args,
            &[
                "invalid_capture",
                &node_path,
                "field name",
                "JSON",
                "correct this metadata",
            ],
        );
    }
    let missing = original
        .lines()
        .filter(|line| !line.starts_with("micro_summary:"))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";
    fs::write(&entry, missing).unwrap();
    refusal(
        &home,
        &store,
        &["instructions"],
        &[&node_path, "micro_summary", "missing required metadata"],
    );
    fs::write(&entry, &original).unwrap();
    let order = store.join(format!("nodes/{ENTRY}/children.order"));
    let saved_order = fs::read(&order).unwrap();
    fs::write(&order, format!("{SECRET}\n")).unwrap();
    refusal(
        &home,
        &store,
        &["check", store_arg],
        &[
            "invalid_candidate",
            &format!("nodes/{ENTRY}/children.order"),
            "each child UUID exactly once",
        ],
    );
    fs::write(order, saved_order).unwrap();

    fs::hard_link(&entry, home.join("extra-link")).unwrap();
    refusal(
        &home,
        &store,
        &["instructions"],
        &[&node_path, "hard links", "unsafe_filesystem"],
    );
    fs::remove_file(home.join("extra-link")).unwrap();
    fs::write(store.join("nodes/.keep"), SECRET).unwrap();
    refusal(
        &home,
        &store,
        &["instructions"],
        &["nodes/.keep", "empty regular file", "unsafe_filesystem"],
    );
    fs::remove_file(store.join("nodes/.keep")).unwrap();
    symlink(&entry, store.join("nodes/.keep")).unwrap();
    refusal(
        &home,
        &store,
        &["instructions"],
        &["nodes/.keep", "regular file", "unsafe_filesystem"],
    );
    fs::remove_file(store.join("nodes/.keep")).unwrap();
    fs::write(store.join("nodes/.keep"), "").unwrap();

    let missing_home = home.join("missing");
    refusal(
        &home,
        &store,
        &["instructions", missing_home.to_str().unwrap()],
        &[
            "not_found",
            "required path is missing",
            "do not reinitialize",
        ],
    );
    fs::rename(store.join("trees"), home.join("retained-trees")).unwrap();
    refusal(
        &home,
        &store,
        &["instructions"],
        &["path:trees", "not_found", "next_action"],
    );
    fs::rename(home.join("retained-trees"), store.join("trees")).unwrap();
    // Native graph rejection is distinct from an unreadable source.
    let root_link = fs::read_dir(store.join("trees"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| fs::symlink_metadata(p).unwrap().file_type().is_symlink())
        .unwrap();
    let root = root_link.file_name().unwrap().to_str().unwrap().to_owned();
    fs::remove_file(&root_link).unwrap();
    refusal(
        &home,
        &store,
        &["check", store_arg],
        &["invalid_candidate", "not_rooted", &root, "next_action"],
    );
    symlink(format!("../nodes/{root}"), &root_link).unwrap();
    let out = cli(&home, &["instructions"]);
    assert!(out.status.success(), "{out:?}");
    assert_eq!(fs::read(store.join("nodes/.keep")).unwrap(), b"");

    // A late per-node validation failure inherits the same failure-only view in
    // read, instructions, worktree allocation and supplied-bundle initialization.
    fs::write(
        &entry,
        original.replace("see_also:", "ruled_out: []\nsee_also:"),
    )
    .unwrap();
    let destination = home.join("refused-init");
    for args in [
        vec!["check", store_arg],
        vec!["read", store_arg, ENTRY],
        vec!["instructions", store_arg],
        vec!["worktree", store_arg],
        vec!["init", destination.to_str().unwrap(), "--bundle", store_arg],
    ] {
        refusal(
            &home,
            &store,
            &args,
            &[ENTRY, "ruled_out_removed", "rows[1]"],
        );
        assert!(!destination.exists());
    }
    fs::write(entry, original).unwrap();
}
