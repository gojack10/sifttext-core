use std::{
    fs,
    os::unix::fs::symlink,
    process::{Command, Output, Stdio},
};

const ROOT: &str = "00000000-0000-4000-8000-000000000001";
const CHILD: &str = "00000000-0000-4000-8000-000000000002";

fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_sifttext"))
        .args(args)
        .env("SIFTTEXT_NO_UPDATE", "1")
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

#[test]
fn full_outline_suppresses_large_returns_and_y_prints_exact_bytes() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().canonicalize().unwrap().join("home");
    let home_text = home.to_str().unwrap();
    let initialized = cli(&["init", home_text, "--empty"]);
    assert!(initialized.status.success(), "{initialized:?}");

    let root = home.join("nodes").join(ROOT);
    let child = home.join("nodes").join(CHILD);
    fs::create_dir_all(root.join("children")).unwrap();
    fs::create_dir_all(child.join("children")).unwrap();
    fs::write(root.join("children.order"), format!("{CHILD}\n")).unwrap();
    fs::write(child.join("children.order"), "").unwrap();
    fs::write(
        root.join("node.md"),
        format!("---\nid: \"{ROOT}\"\nname: \"Root\"\nmicro_summary: \"Synthetic root\"\nstatus: \"not_examined\"\nsee_also: []\n---\nRoot.\n"),
    )
    .unwrap();
    fs::write(
        child.join("node.md"),
        format!("---\nid: \"{CHILD}\"\nname: \"Large child\"\nmicro_summary: \"{}\"\nstatus: \"not_examined\"\nsee_also: []\n---\nChild.\n", " ordinary".repeat(20_000)),
    )
    .unwrap();
    symlink(format!("../../{CHILD}"), root.join("children").join(CHILD)).unwrap();
    symlink(format!("../nodes/{ROOT}"), home.join("trees").join(ROOT)).unwrap();

    let full = cli(&["outline", home_text, ROOT, "full", "-y"]);
    assert!(full.status.success(), "{full:?}");
    assert!(full.stderr.is_empty(), "{full:?}");
    let value = sifttext_file_graph::outlines::decode_lean(std::str::from_utf8(&full.stdout).unwrap()).unwrap();
    assert_eq!(value["rows"][1]["micro_summary"], " ordinary".repeat(20_000));
    assert_eq!(value["rows"][0]["children"], 1);
    assert_eq!(value["rows"][1]["children"], 0);
    let tokens = tiktoken_rs::o200k_base_singleton()
        .lock()
        .encode_ordinary(std::str::from_utf8(&full.stdout).unwrap())
        .len();
    assert!(tokens > 15_000, "expected oversized outline, got {tokens}");

    let guarded = cli(&["outline", home_text, ROOT, "full"]);
    assert_eq!(guarded.status.code(), Some(1), "{guarded:?}");
    assert!(guarded.stderr.is_empty(), "{guarded:?}");
    assert_eq!(
        guarded.stdout,
        format!("[{tokens} tokens suppressed] Re-run this sifttext outline command with -y to print them.\n").as_bytes()
    );
}

#[test]
fn outline_cli_keeps_children_and_guardrails_with_exact_branch_expansion_costs() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().canonicalize().unwrap().join("home");
    let path = home.to_str().unwrap();
    assert!(cli(&["init", path, "--empty"]).status.success());
    let ids: Vec<_> = (1..=4).map(|n| format!("00000000-0000-4000-8000-{n:012}")).collect();
    for (i, id) in ids.iter().enumerate() {
        let node = home.join("nodes").join(id);
        fs::create_dir_all(node.join("children")).unwrap();
        let children: Vec<usize> = match i { 0 => vec![1,2], 1 => vec![3], _ => vec![] };
        fs::write(node.join("children.order"), children.iter().map(|i| ids[*i].clone() + "\n").collect::<String>()).unwrap();
        for child in children {
            symlink(format!("../../{}", ids[child]), node.join("children").join(&ids[child])).unwrap();
        }
        let fields = serde_json::json!({"id":id,"name":format!("Fixture Ω {i}"),
            "micro_summary":format!("Intact summary 界 {i}"),"see_also":[],"warnings":"Exact warning Ω\nNext line.",
            "status":if i==2 {"deferred"} else {"in_progress"},"deferred_reason":"Exact active reason.","blocker":"Inactive reason."});
        fs::write(node.join("node.md"), format!("---\n{}---\nFixture body.\n", fields.as_object().unwrap().iter().map(|(k,v)| format!("{k}: {v}\n")).collect::<String>())).unwrap();
    }
    symlink(format!("../nodes/{}", ids[0]), home.join("trees").join(&ids[0])).unwrap();
    let decode = |out: &Output| {
        assert!(out.status.success(), "{out:?}");
        sifttext_file_graph::outlines::decode_lean(std::str::from_utf8(&out.stdout).unwrap()).unwrap()
    };
    let shallow = decode(&cli(&["outline", path, &ids[0]]));
    let full_output = cli(&["outline", path, &ids[0], "full"]);
    let full = decode(&full_output);
    assert_eq!(shallow["coverage"], "shallow");
    assert_eq!(full["coverage"], "full");
    assert_eq!(shallow["path_template"], "nodes/{id}/node.md");
    assert_eq!(full["rows"].as_array().unwrap().len(), 4);
    assert_eq!(shallow["rows"].as_array().unwrap().len(), 3);
    for key in ["source_version", "source_version_scope", "token_encoding"] {
        assert!(shallow.get(key).is_none(), "{key}");
        assert!(full.get(key).is_none(), "{key}");
    }
    assert_eq!(shallow["version"], decode(&cli(&["sql", path, "SELECT 1"]))["version"]);
    assert_eq!(shallow["version"], decode(&cli(&["search", path, "Fixture"]))["version"]);
    for (i, row) in shallow["rows"].as_array().unwrap().iter().enumerate() {
        assert_eq!(row["id"], ids[i]);
        assert_eq!(row["name"], format!("Fixture Ω {i}"));
        assert_eq!(row["micro_summary"], format!("Intact summary 界 {i}"));
        assert_eq!(row["warnings"], "Exact warning Ω\nNext line.");
        assert_eq!(row["children"], [2,1,0][i]);
        assert!(row.get("descendants").is_none());
        assert!(row.get("blocker").is_none());
        if i == 2 {
            assert_eq!(row["deferred_reason"], "Exact active reason.");
            assert!(row.get("full_view_tokens").is_none());
        } else {
            let expanded = cli(&["outline", path, &ids[i], "full"]);
            let count = tiktoken_rs::o200k_base_singleton().lock().encode_ordinary(std::str::from_utf8(&expanded.stdout).unwrap()).len();
            assert_eq!(row["full_view_tokens"], count);
        }
    }
    assert!(full["rows"].as_array().unwrap().iter().all(|row| row.get("full_view_tokens").is_none() && row.get("descendants").is_none()));
    // The internal checked artifacts retain their rich identities/counts; CLI projection is not a verifier bypass.
    let count = |text: &str| tiktoken_rs::o200k_base_singleton().lock().encode_ordinary(text).len();
    for (mode, file, current) in [("full", "outline.full.lean", &full), ("shallow", "outline.lean", &shallow)] {
        let old = fs::read_to_string(home.join("nodes").join(&ids[0]).join(file)).unwrap();
        let new = sifttext_file_graph::lean_render::render(current).unwrap() + "\n";
        println!("outline {mode} nodes=4 old_bytes={} old_tokens={} new_bytes={} new_tokens={}", old.len(), count(&old), new.len(), count(&new));
    }
    let internal = fs::read_to_string(home.join("nodes").join(&ids[0]).join("outline.full.lean")).unwrap();
    let internal = sifttext_file_graph::outlines::decode_lean(&internal).unwrap();
    assert_eq!(internal["source_version_scope"], "outline_view");
    assert_eq!(internal["rows"][0]["descendants"], 3);
}
