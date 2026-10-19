//! CLI controls from successor/default_native_kb.mjs, complementing the seed unit tests.
//! Build checked assets first. SIFTTEXT_TEST_EVIDENCE names a fresh receipt directory.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::{
        fs::{PermissionsExt, symlink},
        process::ExitStatusExt,
    },
    path::{Path, PathBuf},
    process::Command,
};
const ENTRY: &str = "5e565245-14fe-406c-977c-cf94a9be246b";
const ROOT: &str = "38a9fc79-8a84-494d-a962-2ec5134bc856";
const ORDER: [&str; 6] = [
    "b6107909-a33b-4c63-ab0d-351ba1a1b672",
    "f961a20f-1df5-4c0f-a263-9730e2a28aca",
    "0278c212-d51f-46db-8bd6-b537593ef186",
    "4cc0b396-4b55-415e-bb26-b51c97e6ca78",
    "fb1e0c9f-8c04-4baf-91e6-4688856a14af",
    "a472e05c-9bbf-48f2-89d7-6c6092c339ed",
];
fn text(p: &Path) -> &str {
    p.to_str().unwrap()
}
fn inventory(root: &Path) -> BTreeMap<PathBuf, Value> {
    fn walk(root: &Path, rel: &Path, out: &mut BTreeMap<PathBuf, Value>) {
        let path = root.join(rel);
        let m = fs::symlink_metadata(&path).unwrap();
        let value = if m.is_symlink() {
            json!({"link":fs::read_link(&path).unwrap()})
        } else if m.is_dir() {
            for e in fs::read_dir(&path).unwrap() {
                let name = e.unwrap().file_name();
                let name_text = name.to_str().unwrap();
                if name_text != "node.meta.json" && !name_text.starts_with("outline") {
                    walk(root, &rel.join(name), out);
                }
            }
            json!({"dir":true})
        } else {
            assert!(m.is_file());
            json!({"bytes":fs::read(path).unwrap()})
        };
        out.insert(rel.into(), value);
    }
    let mut out = BTreeMap::new();
    for name in ["nodes", "trees"] {
        walk(root, Path::new(name), &mut out);
    }
    out
}
struct Native {
    evidence: PathBuf,
    git_home: PathBuf,
    records: Vec<Value>,
}
impl Native {
    fn run(&mut self, program: &str, args: &[&str], ok: bool, env: &[(&str, &str)]) -> String {
        let mut cmd = Command::new(program);
        for (key, _) in std::env::vars_os() {
            if key.as_encoded_bytes().starts_with(b"GIT_") {
                cmd.env_remove(key);
            }
        }
        cmd.args(args)
            .current_dir(&self.evidence)
            .env("HOME", &self.git_home)
            .env("XDG_CONFIG_HOME", &self.git_home)
            .env("GIT_CONFIG_NOSYSTEM", "1");
        for (k, v) in env {
            cmd.env(k, v);
        }
        let start = std::time::Instant::now();
        let out = cmd.output().unwrap();
        let stdout = String::from_utf8(out.stdout).unwrap();
        let stderr = String::from_utf8(out.stderr).unwrap();
        self.records.push(json!({"program":program,"args":args,"exit":out.status.code(),"signal":out.status.signal(),"ms":start.elapsed().as_secs_f64()*1000.0,"stdout":stdout,"stderr":stderr}));
        fs::write(
            self.evidence.join("commands.json"),
            serde_json::to_vec_pretty(&self.records).unwrap(),
        )
        .unwrap();
        assert_eq!(
            out.status.success(),
            ok,
            "{program} {args:?}: {stdout}\n{stderr}"
        );
        if ok {
            assert_eq!(out.status.signal(), None);
        }
        stdout
    }
    fn cli(&mut self, args: &[&str], ok: bool) -> String {
        self.run(env!("CARGO_BIN_EXE_sifttext"), args, ok, &[])
    }
    fn json(&mut self, args: &[&str]) -> Value {
        let output = self.cli(args, true);
        if args.first().is_some_and(|cmd| {
            !matches!(
                *cmd,
                "sql" | "search" | "submit" | "accepted" | "session-capture" | "session-attach" | "session-backup" | "session-restore"
            )
        }) {
            sifttext_file_graph::outlines::decode_lean(&output).unwrap()
        } else {
            serde_json::from_str(&output).unwrap()
        }
    }
    fn git(&mut self, path: &Path, args: &[&str]) -> String {
        let mut all = vec![
            "-C",
            text(path),
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgSign=false",
        ];
        all.extend_from_slice(args);
        self.run("git", &all, true, &[]).trim().into()
    }
}
#[test]
fn default_bundle_cli_preserves_copy_refusal_publication_and_recovery_controls() {
    let temp = tempfile::tempdir().unwrap();
    let evidence = match std::env::var_os("SIFTTEXT_TEST_EVIDENCE") {
        Some(p) => {
            let p = PathBuf::from(p);
            assert!(p.is_absolute());
            fs::create_dir(&p).unwrap();
            fs::canonicalize(p).unwrap()
        }
        None => fs::canonicalize(temp.path()).unwrap(),
    };
    let git_home = evidence.join("git-home");
    fs::create_dir(&git_home).unwrap();
    fs::write(git_home.join(".gitconfig"),"[user]\nname = Synthetic Fixture\nemail = fixture@example.invalid\n[commit]\ngpgSign = false\n").unwrap();
    let mut n = Native {
        evidence: evidence.clone(),
        git_home,
        records: vec![],
    };
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR"));
    let source_bytes = fs::read(checkout.join("src/native_kb.rs")).unwrap();
    assert!(n.cli(&["-h"], true).contains("embedded first-party KB"));
    let home = evidence.join("home");
    let h = text(&home);
    let initial = n.json(&["init", h]);
    assert_eq!(initial["pointer"], "AGENTS.md");
    let original = inventory(&home);
    let source = inventory(checkout);
    let source_nodes = source
        .keys()
        .filter(|path| path.starts_with("nodes") && path.file_name().is_some_and(|name| name == "node.md"))
        .count();
    assert_eq!(initial["starter_nodes"], source_nodes);
    assert_eq!(original, source, "exact readable source bytes and links");
    let pipeline = n.json(&["instructions", h]);
    assert_eq!(
        pipeline["path"].as_str().unwrap(),
        format!("{h}/nodes/{ENTRY}/node.md")
    );
    let ordered: Vec<&str> = std::iter::once(ENTRY).chain(ORDER).collect();
    for id in ordered.iter().copied() {
        assert_eq!(
            n.cli(&["read", h, id], true),
            fs::read_to_string(format!("{h}/nodes/{id}/node.md")).unwrap()
        );
    }
    let pointer = home.join("AGENTS.md");
    let pointer_bytes = fs::read(&pointer).unwrap();
    assert!(fs::symlink_metadata(&pointer).unwrap().is_file());
    let pointer_text = String::from_utf8(pointer_bytes.clone()).unwrap();
    let named: std::collections::BTreeSet<_> = pointer_text
        .split('`')
        .filter(|s| s.starts_with("nodes/") && s.ends_with("/node.md"))
        .collect();
    assert_eq!(
        named,
        std::collections::BTreeSet::from([format!("nodes/{ENTRY}/node.md").as_str()]),
        "the pointer names only the canonical entry; the entry itself selects the rest"
    );
    let from_pointer: Vec<_> = std::iter::once(ENTRY.to_owned())
        .chain(
            fs::read_to_string(home.join(format!("nodes/{ENTRY}/children.order")))
                .unwrap()
                .lines()
                .map(str::to_owned),
        )
        .map(|id| fs::canonicalize(home.join(format!("nodes/{id}/node.md"))).unwrap())
        .collect();
    let entry_path = fs::canonicalize(pipeline["path"].as_str().unwrap()).unwrap();
    assert!(
        from_pointer.contains(&entry_path),
        "the single returned entry path must be the pointer's own first read target"
    );
    assert_eq!(
        n.git(&home, &["status", "--porcelain", "--untracked-files=all"]),
        ""
    );
    assert_eq!(n.git(&home, &["ls-files", "--", "AGENTS.md"]), "");
    let operations = n.cli(&["read", h, ORDER[5]], true);
    for obsolete in [
        "generates no home",
        "no top-level instruction file",
        "/Users/",
        "PKG=",
    ] {
        assert!(!operations.contains(obsolete));
    }
    for expected in [
        "\"pointer\":\"AGENTS.md\"",
        "\"pointer\":null",
        "remains the supported lookup",
    ] {
        assert!(operations.contains(expected));
    }
    assert!(
        n.cli(&["read", h, ROOT], true)
            .contains("SiftText Knowledge Base")
    );
    let runtime = n.cli(&["runtime-check"], true);
    assert_eq!(
        sifttext_file_graph::outlines::decode_lean(&runtime).unwrap()["status"],
        "checked"
    );
    assert_eq!(n.json(&["index", h])["nodes"], source_nodes);
    assert!(
        n.json(&["search", h, "SiftText"])["total_matches"]
            .as_u64()
            .unwrap()
            > 0
    );
    assert_eq!(
        n.json(&["sql", h, "SELECT count(*) FROM nodes"])["rows"][0]["c0"],
        source_nodes.to_string()
    );
    assert_eq!(
        n.json(&[
            "sql",
            h,
            "SELECT count(*) FROM nodes WHERE name = 'Skill Library'"
        ])["rows"][0]["c0"],
        "1"
    );
    for query in [
        "DELETE FROM nodes",
        "PRAGMA user_version=1",
        "SELECT 1; DELETE FROM nodes",
    ] {
        n.cli(&["sql", h, query], false);
    }
    assert!(
        n.cli(&["outline", h, ROOT], true)
            .contains("coverage:shallow")
    );
    assert!(
        n.cli(&["outline", h, ROOT, "full"], true)
            .contains("coverage:full")
    );
    let blocks: Vec<_> = operations
        .split("```sh\n")
        .skip(1)
        .map(|s| s.split("```").next().unwrap())
        .collect();
    // Source build/setup is independently covered by portable qualification. Do not
    // recursively build from a Cargo test; execute the body's unique default-init line.
    // Identity/setup examples may add other shell blocks before the operations.
    let init_lines: Vec<_> = blocks.iter()
        .flat_map(|block| block.lines())
        .filter(|s| s.starts_with("\"$BIN\" init \"$home\""))
        .collect();
    assert_eq!(init_lines.len(), 1, "one documented default-init command");
    let init_line = init_lines[0];
    let portable = evidence.join("portable-home");
    n.run(
        "/bin/sh",
        &["-eu", "-c", init_line],
        true,
        &[
            ("BIN", env!("CARGO_BIN_EXE_sifttext")),
            ("home", text(&portable)),
        ],
    );
    assert_eq!(inventory(&portable), original);
    let other = evidence.join("other");
    let o = text(&other);
    n.json(&["init", o]);
    assert_eq!(inventory(&other), original);
    let alternate = evidence.join("alternate");
    assert_eq!(
        n.json(&["init", text(&alternate), "--bundle", h])["starter_nodes"],
        source_nodes
    );
    assert_eq!(inventory(&alternate), original);
    let empty = evidence.join("empty");
    let e = text(&empty);
    let empty_result = n.json(&["init", e, "--empty"]);
    assert_eq!(empty_result["starter_nodes"], 0);
    assert!(empty_result["pointer"].is_null());
    assert!(!empty.join("AGENTS.md").exists());
    assert_eq!(n.json(&["init", e])["created"], false);
    assert_eq!(n.json(&["index", e])["nodes"], 0);
    let editable = home.join(format!("nodes/{ENTRY}/node.md"));
    let bytes = fs::read(&editable).unwrap();
    fs::write(&editable, "unfinished edit").unwrap();
    fs::write(home.join("untracked"), "retain me").unwrap();
    assert_eq!(n.json(&["init", h])["created"], false);
    assert_eq!(
        n.json(&[
            "init",
            h,
            "--bundle",
            text(&evidence.join("missing-bundle"))
        ])["created"],
        false
    );
    assert_eq!(fs::read_to_string(&editable).unwrap(), "unfinished edit");
    assert_eq!(
        fs::read_to_string(home.join("untracked")).unwrap(),
        "retain me"
    );
    assert_eq!(fs::read(&pointer).unwrap(), pointer_bytes);
    n.cli(&["index", h], false);
    fs::write(&editable, bytes).unwrap();
    assert_eq!(inventory(&other), original);
    let prior = evidence.join("prior");
    fs::create_dir(&prior).unwrap();
    fs::write(prior.join("notes"), "keep").unwrap();
    n.cli(&["init", text(&prior)], false);
    assert_eq!(fs::read_to_string(prior.join("notes")).unwrap(), "keep");
    let linked = evidence.join("linked");
    symlink(&prior, &linked).unwrap();
    for path in [
        linked.clone(),
        linked.join("new"),
        evidence.join("missing/new"),
        prior.join("../escape"),
    ] {
        n.cli(&["init", text(&path)], false);
    }
    assert!(!prior.join("new").exists() && !evidence.join("escape").exists());
    let bad = evidence.join("bad");
    n.cli(
        &[
            "init",
            text(&bad),
            "--bundle",
            text(&evidence.join("missing")),
        ],
        false,
    );
    assert!(!bad.exists());
    n.cli(&["init", text(&bad), "--empty", "--bundle", h], false);
    assert!(!bad.exists());
    let identityless = evidence.join("identityless");
    fs::create_dir(&identityless).unwrap();
    fs::write(
        identityless.join(".gitconfig"),
        "[user]\nuseConfigOnly = true\n",
    )
    .unwrap();
    let no_identity = evidence.join("no-identity-home");
    n.run(
        env!("CARGO_BIN_EXE_sifttext"),
        &["init", text(&no_identity)],
        false,
        &[
            ("HOME", text(&identityless)),
            ("XDG_CONFIG_HOME", text(&identityless)),
        ],
    );
    assert!(!no_identity.exists());
    let tools = evidence.join("tools");
    fs::create_dir(&tools).unwrap();
    let git = tools.join("git");
    fs::write(&git,"#!/bin/sh\nfor arg do\n if [ \"$arg\" = init ]; then kill -TERM \"$PPID\"; exit 143; fi\ndone\nexec /usr/bin/git \"$@\"\n").unwrap();
    fs::set_permissions(git, fs::Permissions::from_mode(0o700)).unwrap();
    let partial = evidence.join("interrupted");
    n.run(
        env!("CARGO_BIN_EXE_sifttext"),
        &["init", text(&partial)],
        false,
        &[(
            "PATH",
            &format!("{}:{}", tools.display(), std::env::var("PATH").unwrap()),
        )],
    );
    assert!(partial.join(".gitignore").exists() && !partial.join(".git").exists());
    let partial_inventory = inventory(&partial);
    n.cli(&["init", text(&partial)], false);
    assert_eq!(inventory(&partial), partial_inventory);
    let allocation = n.json(&["worktree", o]);
    let draft = PathBuf::from(allocation["worktree"].as_str().unwrap());
    let d = text(&draft);
    assert!(!draft.join("AGENTS.md").exists());
    assert_eq!(
        n.json(&["instructions", d])["path"].as_str().unwrap(),
        format!("{d}/nodes/{ENTRY}/node.md")
    );
    let created = n.json(&[
        "create",
        d,
        "Defaultbundlescratchtrial",
        "Records a reversible default-bundle test.",
    ]);
    let file = PathBuf::from(created["path"].as_str().unwrap());
    let created_bytes = fs::read_to_string(&file).unwrap();
    fs::write(
        &file,
        created_bytes.replace("Records a reversible default-bundle test.", ""),
    )
    .unwrap();
    n.cli(&["check", d, o], false);
    n.cli(&["index", d], false);
    assert_eq!(
        n.git(&other, &["rev-parse", "refs/heads/main"]),
        allocation["base"].as_str().unwrap()
    );
    fs::write(
        &file,
        format!("{created_bytes}\nAn ordinary authored scratch edit.\n"),
    )
    .unwrap();
    n.cli(&["check", d, o], true);
    assert_eq!(
        n.json(&["search", d, "Defaultbundlescratchtrial"])["rows"].as_array().unwrap().iter().find(|r| r["match"] == true).unwrap()["id"],
        created["id"]
    );
    n.git(&draft, &["add", "--", "nodes", "trees"]);
    n.git(
        &draft,
        &["commit", "-m", "knowledge: test default bundle scratch"],
    );
    let commit = n.git(&draft, &["rev-parse", "HEAD"]);
    let committed_inventory = inventory(&draft);
    let receipt = n.json(&[
        "submit",
        o,
        d,
        &commit,
        allocation["base"].as_str().unwrap(),
    ]);
    assert_eq!(receipt["status"], "current");
    assert_eq!(receipt["published"], true);
    assert_eq!(receipt["draft_commit"], commit);
    assert_eq!(inventory(&other), committed_inventory);
    assert_eq!(receipt["home"], o);
    assert_eq!(receipt["view"], o);
    assert_eq!(receipt["worktree_removed"], true);
    assert!(!draft.exists());
    assert_eq!(n.git(&other, &["rev-parse", "HEAD"]), commit);
    assert_eq!(
        n.json(&[
            "sql",
            receipt["view"].as_str().unwrap(),
            "SELECT count(*) FROM nodes"
        ])["rows"][0]["c0"],
        (source_nodes + 1).to_string()
    );
    let advanced = n.json(&["advance", o]);
    assert_eq!(advanced["accepted"], receipt["candidate"]);
    assert_eq!(advanced["advanced"], false);
    assert_eq!(
        n.json(&["search", o, "Defaultbundlescratchtrial"])["rows"].as_array().unwrap().iter().find(|r| r["match"] == true).unwrap()["id"],
        created["id"]
    );
    let recovery = n.json(&["worktree", o]);
    let work = Path::new(recovery["worktree"].as_str().unwrap());
    n.git(work, &["revert", "--no-edit", &commit]);
    n.cli(&["check", text(work), o], true);
    let revert_commit = n.git(work, &["rev-parse", "HEAD"]);
    let restored = n.json(&[
        "submit",
        o,
        text(work),
        &revert_commit,
        recovery["base"].as_str().unwrap(),
    ]);
    assert_eq!(restored["published"], true);
    n.json(&["advance", o]);
    assert_eq!(
        n.json(&["sql", o, "SELECT count(*) FROM nodes"])["rows"][0]["c0"],
        source_nodes.to_string()
    );
    assert_eq!(
        n.json(&["search", o, "Defaultbundlescratchtrial"])["total_matches"],
        0
    );
    // Strict cleanup removes a deleted node's recognized generated files and the
    // directory they occupied; restoration comes from retained Git history, not
    // from leftover residue in the reader view.
    let residue = other.join("nodes").join(created["id"].as_str().unwrap());
    assert!(
        !residue.exists(),
        "deleted node left generated residue at {residue:?}"
    );
    assert_eq!(inventory(&other), original);
    assert_eq!(
        fs::read(checkout.join("src/native_kb.rs")).unwrap(),
        source_bytes
    );
    fs::write(evidence.join("result.json"),serde_json::to_vec_pretty(&json!({"status":"passed","commands":n.records.len(),"home":home,"source_hash":sifttext_file_graph::sha256(&source_bytes),"scope":"CLI default-copy/refusal/publication/recovery only; source setup remains separately qualified, no whole readiness or speed claim"})).unwrap()).unwrap();
}
