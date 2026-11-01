//! Check an already installed package without changing its source or knowledge.
//! SIFTTEXT_PORTABLE_PACKAGE=/absolute/install cargo test --test portable_runtime -- --ignored --nocapture
use std::{
    fs,
    os::unix::fs::symlink,
    path::Path,
    process::{Command, Output},
};

fn run(bin: &Path, tools: &Path, args: &[&str]) -> Output {
    Command::new(bin)
        .args(args)
        .env("PATH", tools)
        .env("HOME", tools.parent().unwrap())
        .env("XDG_CONFIG_HOME", tools.parent().unwrap().join("config"))
        .env_remove("BEND")
        .env_remove("BEND_BIN")
        .env_remove("RUSTUP_TOOLCHAIN")
        .output()
        .unwrap()
}

fn cli(bin: &Path, tools: &Path, args: &[&str]) -> Output {
    let out = run(bin, tools, args);
    assert!(
        out.status.success(),
        "{args:?}: {}\n{}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    out
}

fn json(bin: &Path, tools: &Path, args: &[&str]) -> serde_json::Value {
    let output = cli(bin, tools, args).stdout;
    if args.first() == Some(&"instructions") {
        let text = std::str::from_utf8(&output).unwrap();
        return sifttext_file_graph::outlines::decode_lean(&format!("{}\n", text.lines().next().unwrap())).unwrap();
    }
    // Nested-schema receipts use JSON; search and sql use LEAN like other ordinary results.
    if matches!(
        args.first(),
        Some(&"normalize")
            | Some(&"submit")
            | Some(&"accepted")
            | Some(&"session-capture")
            | Some(&"session-attach")
            | Some(&"session-backup")
            | Some(&"session-restore")
    ) {
        serde_json::from_slice(&output)
            .unwrap_or_else(|e| panic!("{args:?}: expected JSON receipt: {e}"))
    } else {
        sifttext_file_graph::outlines::decode_lean(std::str::from_utf8(&output).unwrap())
            .unwrap_or_else(|e| panic!("{args:?}: expected LEAN receipt: {e}"))
    }
}

fn git(tools: &Path, path: &Path, args: &[&str]) -> String {
    let mut command = vec![
        "-C",
        path.to_str().unwrap(),
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "commit.gpgSign=false",
    ];
    command.extend_from_slice(args);
    String::from_utf8(cli(&tools.join("git"), tools, &command).stdout)
        .unwrap()
        .trim()
        .to_owned()
}

fn workflow(bin: &Path, tools: &Path, parent: &Path) {
    let home = parent.join("knowledge-home");
    let home = home.to_str().unwrap();
    let initial = json(bin, tools, &["init", home]);
    assert_eq!(initial["created"], true);
    let entry = "5e565245-14fe-406c-977c-cf94a9be246b";
    let node = Path::new(home).join("nodes").join(entry).join("node.md");
    let original = fs::read(&node).unwrap();
    let pointer = fs::read(Path::new(home).join("AGENTS.md")).unwrap();
    let pipeline = json(bin, tools, &["instructions", home]);
    assert_eq!(
        pipeline["onboarding-entry"].as_str().unwrap(),
        format!("{home}/nodes/{entry}/node.md")
    );
    assert_eq!(cli(bin, tools, &["read", home, entry]).stdout, original);
    let index = json(bin, tools, &["index", home]);
    let count = index["nodes"].as_u64().unwrap();
    assert!(
        count > 7,
        "must use the real bundled KB, not an empty fixture"
    );
    let search = json(bin, tools, &["search", home, "SiftText"]);
    assert!(search["total_matches"].as_u64().unwrap() > 0);
    assert_eq!(search["version"], index["version"]);
    assert!(
        String::from_utf8(cli(bin, tools, &["outline", home, entry, "full"]).stdout)
            .unwrap()
            .contains("coverage:full")
    );
    let sql = json(bin, tools, &["sql", home, "SELECT count(*) FROM nodes"]);
    assert_eq!(sql["rows"][0]["c0"], count.to_string());
    assert_eq!(sql["version"], index["version"]);

    // Recognition must not reseed even an unfinished, invalid user edit.
    fs::write(&node, b"unfinished user edit").unwrap();
    assert_eq!(json(bin, tools, &["init", home])["created"], false);
    assert_eq!(fs::read(&node).unwrap(), b"unfinished user edit");
    assert_eq!(
        fs::read(Path::new(home).join("AGENTS.md")).unwrap(),
        pointer
    );
    fs::write(&node, &original).unwrap();

    // Reject an exact committed invalid candidate and preserve its branch/ref.
    let rejected = json(bin, tools, &["worktree", home]);
    let bad = rejected["worktree"].as_str().unwrap();
    let bad_node = Path::new(bad).join("nodes").join(entry).join("node.md");
    fs::write(
        &bad_node,
        String::from_utf8(original.clone())
            .unwrap()
            .replace("status: \"in_progress\"", "status: \"invalid\""),
    )
    .unwrap();
    git(tools, Path::new(bad), &["add", "--", "nodes"]);
    git(
        tools,
        Path::new(bad),
        &["commit", "-m", "knowledge: invalid fixture"],
    );
    let bad_commit = git(tools, Path::new(bad), &["rev-parse", "HEAD"]);
    let base = rejected["base"].as_str().unwrap();
    let refusal = run(bin, tools, &["submit", home, bad, &bad_commit, base]);
    assert!(
        !refusal.status.success(),
        "invalid committed candidate accepted"
    );
    let refusal: serde_json::Value = serde_json::from_slice(&refusal.stdout).unwrap();
    assert_ne!(refusal["status"], "current");
    assert_eq!(
        git(tools, Path::new(home), &["rev-parse", "refs/heads/main"]),
        base
    );
    assert_eq!(
        git(tools, Path::new(bad), &["rev-parse", "HEAD"]),
        bad_commit
    );

    let allocation = json(bin, tools, &["worktree", home]);
    let draft = allocation["worktree"].as_str().unwrap();
    let path = Path::new(draft).join("nodes").join(entry).join("node.md");
    let mut edited = original.clone();
    edited.extend_from_slice(b"\nPortableworkflowuniqueterm: reversible runtime check.\n");
    fs::write(&path, &edited).unwrap();
    cli(bin, tools, &["normalize", home, draft]);
    git(tools, Path::new(draft), &["add", "--", "nodes"]);
    git(
        tools,
        Path::new(draft),
        &["commit", "-m", "knowledge: reversible portable fixture"],
    );
    let commit = git(tools, Path::new(draft), &["rev-parse", "HEAD"]);
    let receipt = json(bin, tools, &["submit", home, draft, &commit, base]);
    assert_eq!(receipt["status"], "saved");
    assert_eq!(
        git(tools, Path::new(home), &["rev-parse", "refs/heads/main"]),
        receipt["accepted"].as_str().unwrap()
    );
    assert_eq!(fs::read(&node).unwrap(), edited);
    assert_eq!(receipt["home"], home);
    assert_eq!(receipt["worktree_removed"], true);
    assert!(!Path::new(draft).exists());
    assert_eq!(git(tools, Path::new(home), &["rev-parse", "HEAD"]), commit);
    let view = receipt["home"].as_str().unwrap();
    assert_eq!(cli(bin, tools, &["read", view, entry]).stdout, edited);
    assert_eq!(
        json(bin, tools, &["search", view, "Portableworkflowuniqueterm"])["total_matches"],
        1
    );

    let recovery = json(bin, tools, &["worktree", view]);
    let restore = recovery["worktree"].as_str().unwrap();
    git(tools, Path::new(restore), &["revert", "--no-edit", &commit]);
    let revert = git(tools, Path::new(restore), &["rev-parse", "HEAD"]);
    let restored = json(
        bin,
        tools,
        &[
            "submit",
            home,
            restore,
            &revert,
            recovery["base"].as_str().unwrap(),
        ],
    );
    assert_eq!(restored["status"], "saved");
    assert_eq!(
        git(
            tools,
            Path::new(home),
            &[
                "diff",
                initial["commit"].as_str().unwrap(),
                restored["accepted"].as_str().unwrap(),
                "--",
                "nodes",
                "trees"
            ]
        ),
        "",
        "revert must recover every authored byte/link"
    );
    let accepted = json(bin, tools, &["accepted", home]);
    assert_eq!(accepted["status"], "ready");
    assert_eq!(accepted["home_advanced"], false);
    assert_eq!(accepted["accepted"], restored["accepted"]);
    let recovered = accepted["view"].as_str().unwrap();
    // Rebuild only this disposable fixture's derived cache, not authored files.
    fs::remove_dir_all(Path::new(recovered).join(".sifttext")).unwrap();
    assert_eq!(
        json(bin, tools, &["index", recovered])["version"],
        restored["version"]
    );
    assert_eq!(
        cli(bin, tools, &["read", recovered, entry]).stdout,
        original
    );
    assert_eq!(
        json(
            bin,
            tools,
            &["search", recovered, "Portableworkflowuniqueterm"]
        )["total_matches"],
        0
    );
    assert_eq!(
        json(
            bin,
            tools,
            &["sql", recovered, "SELECT count(*) FROM nodes"]
        )["rows"][0]["c0"],
        count.to_string()
    );
    cli(bin, tools, &["outline", recovered, entry, "full"]);
    assert_eq!(fs::read(&node).unwrap(), original);
    assert_eq!(
        fs::read(Path::new(home).join("AGENTS.md")).unwrap(),
        pointer
    );
    assert_eq!(json(bin, tools, &["init", home])["created"], false);
    println!(
        "PORTABLE_WORKFLOW_CHECKED nodes={count} initial={} published={} restored={} version={}",
        initial["commit"], receipt["accepted"], restored["accepted"], restored["version"]
    );
}

fn checked(bin: &Path, tools: &Path) {
    let value = json(bin, tools, &["runtime-check"]);
    assert_eq!(value["status"], "checked");
    assert_eq!(value["runtime"], bin.parent().unwrap().to_str().unwrap());
}

#[test]
#[ignore = "requires an explicitly supplied checked installed runtime package"]
fn relocated_runtime_refuses_missing_and_tampered_assets() {
    let original = fs::canonicalize(
        std::env::var_os("SIFTTEXT_PORTABLE_PACKAGE")
            .expect("set SIFTTEXT_PORTABLE_PACKAGE to a checked installed package"),
    )
    .unwrap();
    assert!(original.join("sifttext").is_file());
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(original.join("package.json")).unwrap()).unwrap();
    assert_eq!(
        manifest["pi"]["extensions"][0],
        "./integrations/pi/session-retention.ts"
    );
    for file in [
        "integrations/pi/session-retention.ts",
        "integrations/pi/README.md",
        "integrations/session-retention.ts",
        "integrations/session-capture.ts",
        "integrations/session-checkpoint.ts",
        "integrations/session-descriptor.ts",
    ] {
        assert!(
            original.join(file).is_file(),
            "missing packaged Pi source {file}"
        );
    }
    let temp = tempfile::tempdir().unwrap();
    // macOS TMPDIR commonly traverses /var -> /private/var; homes require canonical parents.
    let root = fs::canonicalize(temp.path()).unwrap();
    let first = root.join("first");
    assert!(
        Command::new("/bin/cp")
            .args(["-R"])
            .arg(&original)
            .arg(&first)
            .status()
            .unwrap()
            .success()
    );
    let package = root.join("relocated");
    fs::rename(&first, &package).unwrap();
    assert!(!first.exists());
    let tools = root.join("tools");
    fs::create_dir(&tools).unwrap();
    fs::write(
        root.join(".gitconfig"),
        "[user]\nname = Runtime Fixture\nemail = runtime@example.invalid\n",
    )
    .unwrap();
    let git = Command::new("/bin/sh")
        .args(["-c", "command -v git"])
        .output()
        .unwrap();
    assert!(git.status.success());
    let git = fs::canonicalize(String::from_utf8(git.stdout).unwrap().trim()).unwrap();
    symlink(git, tools.join("git")).unwrap();
    // PATH contains only Git: no Node, Rust, Bend or compiler lookup is possible.
    // A clean Linux run must additionally establish no machine-pinned compiler exists.
    let bin = package.join("sifttext");
    checked(&bin, &tools);
    let home = root.join("empty-home");
    let out = run(&bin, &tools, &["init", home.to_str().unwrap(), "--empty"]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(home.join(".git").exists());

    for name in [
        "build/graph-policy",
        "build/graph-policy.sha256",
        "build/graph-transition",
        "build/graph-transition.sha256",
        "build/candidate.sha256",
        "README.md",
        "rust-toolchain.toml",
        "nodes/5e565245-14fe-406c-977c-cf94a9be246b/node.md",
    ] {
        let path = package.join(name);
        let original_bytes =
            fs::read(&path).unwrap_or_else(|e| panic!("required asset {name}: {e}"));
        let backup = root.join("asset-backup");
        fs::rename(&path, &backup).unwrap();
        let missing = run(&bin, &tools, &["runtime-check"]);
        assert!(!missing.status.success(), "missing {name} was accepted");
        fs::rename(&backup, &path).unwrap();
        checked(&bin, &tools);
        let mut changed = original_bytes.clone();
        changed.extend_from_slice(b"\ntampered-runtime-asset\n");
        fs::write(&path, changed).unwrap();
        let tampered = run(&bin, &tools, &["runtime-check"]);
        assert!(!tampered.status.success(), "tampered {name} was accepted");
        fs::write(&path, original_bytes).unwrap();
        checked(&bin, &tools);
        println!("RUNTIME_ASSET_REFUSAL missing/tampered {name}");
    }
    workflow(&bin, &tools, &root);
    println!(
        "RELOCATED_RUNTIME_CHECKED {} (copied from {})",
        package.display(),
        original.display()
    );
}
