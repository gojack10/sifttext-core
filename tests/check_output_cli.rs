use std::{
    fs,
    os::unix::fs::symlink,
    process::{Command, Output, Stdio},
};

#[test]
fn check_gates_large_reports_without_changing_validation_or_sources() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let store = root.join("store");
    let before = root.join("before");
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
        if let Some(path) = std::env::var_os("SIFTTEXT_CHECK_EVIDENCE") {
            fs::create_dir_all(&path).unwrap();
            fs::write(
                std::path::Path::new(&path).join("commands.json"),
                serde_json::to_vec_pretty(&records).unwrap(),
            )
            .unwrap();
        }
        out
    };
    for home in [&store, &before] {
        fs::create_dir_all(home.join("nodes")).unwrap();
        fs::create_dir_all(home.join("trees")).unwrap();
    }
    let path = store.to_str().unwrap();
    let prior = before.to_str().unwrap();
    let id = "00000000-0000-4000-8000-000000000001";
    let node = |id: &str| {
        format!(
            "---\nid: \"{id}\"\nname: \"Synthetic check\"\nmicro_summary: \"日本語🦀\"\nstatus: \"in_progress\"\nsee_also: []\n---\nSmall crystallization.\n"
        )
    };
    let add = |home: &std::path::Path, id: &str| {
        let dir = home.join("nodes").join(id);
        fs::create_dir_all(dir.join("children")).unwrap();
        fs::write(dir.join("children.order"), "").unwrap();
        fs::write(dir.join("node.md"), node(id)).unwrap();
        symlink(format!("../nodes/{id}"), home.join("trees").join(id)).unwrap();
    };
    add(&store, id);
    add(&before, id);
    let small = cli(&["check", path]);
    assert_eq!(small.status.code(), Some(0), "{small:?}");
    assert!(small.stderr.is_empty(), "{small:?}");
    assert!(String::from_utf8_lossy(&small.stdout).contains("status:current"));
    let forced_small = cli(&["check", path, "-y"]);
    assert_eq!(forced_small.status.code(), Some(0), "{forced_small:?}");
    assert_eq!(forced_small.stdout, small.stdout);

    // Explicit resolution still requires BEFORE and the exact requested UUID;
    // the output flag must not become either a path or an explicit request.
    let source = store.join("nodes").join(id).join("node.md");
    let resolved = node(id).replace("\"in_progress\"", "\"resolved\"");
    fs::write(&source, &resolved).unwrap();
    let refused = cli(&["check", path, prior, "-y"]);
    assert_eq!(refused.status.code(), Some(1), "{refused:?}");
    assert!(refused.stdout.is_empty(), "{refused:?}");
    assert!(String::from_utf8_lossy(&refused.stderr).contains("status_transition_or_reason"));
    let small_refusal = cli(&["check", path, prior]);
    assert_eq!(small_refusal.status.code(), Some(1), "{small_refusal:?}");
    assert!(small_refusal.stdout.is_empty(), "{small_refusal:?}");
    assert_eq!(small_refusal.stderr, refused.stderr);
    let explicit = cli(&["check", path, prior, id, "-y"]);
    assert_eq!(explicit.status.code(), Some(0), "{explicit:?}");
    assert!(explicit.stderr.is_empty(), "{explicit:?}");
    assert_eq!(fs::read_to_string(&source).unwrap(), resolved);
    assert_eq!(
        fs::read_to_string(before.join("nodes").join(id).join("node.md")).unwrap(),
        node(id)
    );
    fs::write(&source, node(id)).unwrap();

    for n in 2..=400 {
        add(&store, &format!("00000000-0000-4000-8000-{n:012x}"));
    }
    let full = cli(&["check", path, "-y"]);
    assert_eq!(full.status.code(), Some(0), "{full:?}");
    assert!(full.stderr.is_empty(), "{full:?}");
    let tokens = tiktoken_rs::o200k_base_singleton()
        .lock()
        .encode_ordinary(std::str::from_utf8(&full.stdout).unwrap())
        .len();
    assert!(tokens > 15_000, "fixture report too small: {tokens}");
    let guarded = cli(&["check", path]);
    assert_eq!(guarded.status.code(), Some(1), "{guarded:?}");
    assert!(guarded.stderr.is_empty(), "{guarded:?}");
    assert_eq!(guarded.stdout, format!("[{tokens} tokens suppressed] Re-run with -y to print them, read sql schema if needed.\n").as_bytes());
    let retried = cli(&["check", path, "-y"]);
    assert_eq!(retried.status.code(), Some(0), "{retried:?}");
    assert_eq!(retried.stdout, full.stdout);
    assert!(retried.stderr.is_empty(), "{retried:?}");
    // Rejection is still a complete report, not just the failing row. The same
    // output boundary must cover stderr without making -y bypass validation.
    fs::write(&source, &resolved).unwrap();
    let full_refusal = cli(&["check", path, prior, "-y"]);
    assert_eq!(full_refusal.status.code(), Some(1), "{full_refusal:?}");
    assert!(full_refusal.stdout.is_empty(), "{full_refusal:?}");
    let diagnostics = std::str::from_utf8(&full_refusal.stderr).unwrap();
    assert!(diagnostics.starts_with("rejected: "), "{diagnostics}");
    assert!(diagnostics.contains("status_transition_or_reason"));
    let tokens = tiktoken_rs::o200k_base_singleton()
        .lock()
        .encode_ordinary(diagnostics)
        .len();
    assert!(tokens > 15_000, "fixture refusal too small: {tokens}");
    let guarded_refusal = cli(&["check", path, prior]);
    assert_eq!(
        guarded_refusal.status.code(),
        Some(1),
        "{guarded_refusal:?}"
    );
    assert!(guarded_refusal.stderr.is_empty(), "{guarded_refusal:?}");
    assert_eq!(guarded_refusal.stdout, format!("[{tokens} tokens suppressed] Re-run with -y to print them, read sql schema if needed.\n").as_bytes());
    let retried_refusal = cli(&["check", path, prior, "-y"]);
    assert_eq!(
        retried_refusal.status.code(),
        Some(1),
        "{retried_refusal:?}"
    );
    assert!(retried_refusal.stdout.is_empty(), "{retried_refusal:?}");
    assert_eq!(retried_refusal.stderr, full_refusal.stderr);
    assert_eq!(fs::read_to_string(&source).unwrap(), resolved);
    fs::write(&source, node(id)).unwrap();

    for n in 1..=400 {
        let id = format!("00000000-0000-4000-8000-{n:012x}");
        let dir = store.join("nodes").join(&id);
        assert_eq!(fs::read_to_string(dir.join("node.md")).unwrap(), node(&id));
        assert_eq!(fs::read(dir.join("children.order")).unwrap(), b"");
        assert_eq!(fs::read_dir(dir.join("children")).unwrap().count(), 0);
        assert_eq!(
            fs::read_link(store.join("trees").join(&id)).unwrap(),
            std::path::PathBuf::from(format!("../nodes/{id}"))
        );
    }
    assert_eq!(fs::read_dir(&store).unwrap().count(), 2);
    assert_eq!(
        fs::read_to_string(before.join("nodes").join(id).join("node.md")).unwrap(),
        node(id)
    );
    assert_eq!(fs::read_dir(&before).unwrap().count(), 2);
}
