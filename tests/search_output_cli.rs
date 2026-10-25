//! Search discovery retains complete hit text, not repeated ancestor rows or oversized warnings.
use sifttext_file_graph::Store;
use std::{
    fs,
    os::unix::fs::symlink,
    process::{Command, Output, Stdio},
};

const ID: &str = "00000000-0000-4000-8000-000000000001";

#[test]
fn search_bounds_rendered_returns_and_y_preserves_scope_and_refusals() {
    let tmp = tempfile::tempdir().unwrap();
    let store = tmp.path().canonicalize().unwrap().join("store");
    let node = store.join("nodes").join(ID);
    fs::create_dir_all(node.join("children")).unwrap();
    fs::create_dir_all(store.join("trees")).unwrap();
    fs::write(node.join("children.order"), "").unwrap();
    symlink(format!("../nodes/{ID}"), store.join("trees").join(ID)).unwrap();
    let path = store.to_str().unwrap();
    let mut records = Vec::new();
    let mut cli = |args: &[&str]| -> Output {
        let out = Command::new(env!("CARGO_BIN_EXE_sifttext"))
            .args(args)
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .env("SIFTTEXT_NO_UPDATE", "1")
            .stdin(Stdio::null())
            .output()
            .unwrap();
        records.push(serde_json::json!({"args":args,"exit":out.status.code(),
            "stdout":String::from_utf8_lossy(&out.stdout),"stderr":String::from_utf8_lossy(&out.stderr)}));
        if let Some(path) = std::env::var_os("SIFTTEXT_SEARCH_EVIDENCE") {
            fs::create_dir_all(&path).unwrap();
            fs::write(
                std::path::Path::new(&path).join("commands.json"),
                serde_json::to_vec_pretty(&records).unwrap(),
            )
            .unwrap();
        }
        out
    };
    for (warnings, name, summary) in [
        ("Small warning.".to_owned(), "Search boundary".to_owned(), "Synthetic fixture".to_owned()),
        (format!("日本語🦀{}", " ordinary".repeat(15_001)), "Search boundary".to_owned(), "Synthetic fixture".to_owned()),
        ("Small warning.".to_owned(), format!("Search boundary{}", " long-name".repeat(8000)), " intact-summary".repeat(8000)),
    ] {
        let fields = serde_json::json!({"id":ID,"name":name,
            "micro_summary":summary,"status":"in_progress",
            "see_also":[],"warnings":warnings});
        fs::write(
            node.join("node.md"),
            format!(
                "---\n{}---\nSmall content.\n",
                fields
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(k, v)| format!("{k}: {v}\n"))
                    .collect::<String>()
            ),
        )
        .unwrap();
        // The complete forced return is also checked against known authored content.
        let full = cli(&["search", path, "boundary", "-y"]);
        assert_eq!(full.status.code(), Some(0), "{full:?}");
        assert!(full.stderr.is_empty(), "{full:?}");
        assert!(full.stdout.ends_with(b"\n"));
        let value =
            sifttext_file_graph::outlines::decode_lean(std::str::from_utf8(&full.stdout).unwrap())
                .unwrap();
        assert_eq!(value["status"], "current");
        assert!(value.get("version_scope").is_none());
        assert_eq!(value["path_template"], format!("{path}/nodes/{{id}}/node.md"));
        assert_eq!(value["total_matches"], 1);
        assert_eq!(value["rows"].as_array().unwrap().len(), 1);
        assert_eq!(value["rows"][0]["id"], ID);
        assert_eq!(value["rows"][0]["name"], name);
        assert_eq!(value["rows"][0]["micro_summary"], summary);
        for key in ["match", "depth", "hit", "path", "active_reason", "warnings"] {
            assert!(value["rows"][0].get(key).is_none(), "{key}");
        }
        if warnings == "Small warning." {
            assert_eq!(value[format!("warnings_{ID}")], warnings);
        } else {
            assert_eq!(value[format!("must_read_warnings_{ID}")], format!("{path}/nodes/{ID}/node.md"));
            assert!(!String::from_utf8_lossy(&full.stdout).contains(&warnings));
        }
        let tokens = tiktoken_rs::o200k_base_singleton()
            .lock()
            .encode_ordinary(std::str::from_utf8(&full.stdout).unwrap())
            .len();
        assert_eq!(tokens > 15_000, summary != "Synthetic fixture");
        let inventory = Store::read(&store).unwrap().inventory;
        for scope in [None, Some(ID)] {
            let mut args = vec!["search", path, "boundary"];
            args.extend(scope);
            let guarded = cli(&args);
            assert!(guarded.stderr.is_empty(), "{guarded:?}");
            if tokens > 15_000 {
                assert_eq!(guarded.status.code(), Some(1), "{guarded:?}");
                assert_eq!(guarded.stdout, format!("[{tokens} tokens suppressed] Re-run this sifttext search command with -y to print them.\n").as_bytes());
            } else {
                assert_eq!(guarded.status.code(), Some(0), "{guarded:?}");
                assert_eq!(guarded.stdout, full.stdout);
            }
            args.push("-y");
            let forced = cli(&args);
            assert_eq!(forced.status.code(), Some(0), "{forced:?}");
            assert!(forced.stderr.is_empty(), "{forced:?}");
            assert_eq!(forced.stdout, full.stdout);
        }
        let refused = cli(&["search", path, "boundary", "not-a-node", "-y"]);
        assert_eq!(refused.status.code(), Some(1), "{refused:?}");
        assert!(refused.stdout.is_empty(), "{refused:?}");
        assert!(String::from_utf8_lossy(&refused.stderr).starts_with("rejected:"));
        let malformed = cli(&["search", path, "boundary", ID, "extra", "-y"]);
        assert_eq!(malformed.status.code(), Some(1), "{malformed:?}");
        assert!(malformed.stdout.is_empty(), "{malformed:?}");
        assert!(String::from_utf8_lossy(&malformed.stderr).contains("usage: sifttext search"));
        assert_eq!(Store::read(&store).unwrap().inventory, inventory);
    }
}

#[test]
fn search_cli_returns_only_hits_with_compact_context_and_mandatory_warning_read() {
    let temp = tempfile::tempdir().unwrap();
    let store = temp.path().canonicalize().unwrap().join("store");
    fs::create_dir_all(store.join("trees")).unwrap();
    let ids: Vec<_> = (1..=3)
        .map(|n| format!("00000000-0000-4000-8000-{n:012}"))
        .collect();
    let warning = " inherited".repeat(15_001);
    for (depth, id) in ids.iter().enumerate() {
        let node = store.join("nodes").join(id);
        fs::create_dir_all(node.join("children")).unwrap();
        let fields = serde_json::json!({"id":id,"name":if depth==2 {"Needle"} else {"Context"},
            "micro_summary":format!("Summary {depth}"),"status":"deferred","deferred_reason":"Later Ω",
            "see_also":[],"warnings":if depth==0 {warning.as_str()} else {""}});
        fs::write(
            node.join("node.md"),
            format!(
                "---\n{}---\nNeedle body.\n",
                fields
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(k, v)| format!("{k}: {v}\n"))
                    .collect::<String>()
            ),
        )
        .unwrap();
        if let Some(child) = ids.get(depth + 1) {
            fs::write(node.join("children.order"), child.to_owned() + "\n").unwrap();
            symlink(format!("../../{child}"), node.join("children").join(child)).unwrap();
        } else {
            fs::write(node.join("children.order"), "").unwrap();
        }
    }
    symlink(
        format!("../nodes/{}", ids[0]),
        store.join("trees").join(&ids[0]),
    )
    .unwrap();
    let run = |force: bool| {
        let mut command = Command::new(env!("CARGO_BIN_EXE_sifttext"));
        command.args(["search", store.to_str().unwrap(), "Needle", &ids[2]]);
        if force {
            command.arg("-y");
        }
        command.env("SIFTTEXT_NO_UPDATE", "1").output().unwrap()
    };
    let full = run(true);
    assert!(full.status.success(), "{full:?}");
    let value =
        sifttext_file_graph::outlines::decode_lean(std::str::from_utf8(&full.stdout).unwrap())
            .unwrap();
    assert_eq!(value["total_matches"], 1);
    let rows = value["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], ids[2]);
    assert_eq!(rows[0]["root"], ids[0]);
    assert_eq!(rows[0]["parent"], ids[1]);
    assert_eq!(rows[0]["micro_summary"], "Summary 2");
    assert_eq!(rows[0]["status"], "deferred");
    assert!(rows[0].get("active_reason").is_none());
    assert_eq!(value[format!("must_read_warnings_{}", ids[0])], store.join("nodes").join(&ids[0]).join("node.md").to_str().unwrap());
    assert!(rows[0]["matching_excerpt"].is_string());
    let tokens = tiktoken_rs::o200k_base_singleton()
        .lock()
        .encode_ordinary(std::str::from_utf8(&full.stdout).unwrap())
        .len();
    assert!(tokens < 1000);
    let guarded = run(false);
    assert_eq!(guarded.status.code(), Some(0));
    assert_eq!(guarded.stdout, full.stdout);
    fs::write(
        store.join("nodes").join(&ids[0]).join("node.md"),
        "invalid ancestor",
    )
    .unwrap();
    let refused = run(true);
    assert!(!refused.status.success());
    assert!(refused.stdout.is_empty());
    assert!(String::from_utf8_lossy(&refused.stderr).starts_with("rejected:"));
}
