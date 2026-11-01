//! CLI/Git parity for local_onramp.mjs; build checked assets first.
//! SIFTTEXT_TEST_EVIDENCE retains receipts in a NEW directory.
//! SIFTTEXT_ONRAMP_BUNDLE optionally checks a supplied native bundle read-only.
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::{fs::symlink, process::ExitStatusExt},
    path::{Path, PathBuf},
    process::Command,
};
const ENTRY: &str = "5e565245-14fe-406c-977c-cf94a9be246b";
const CHILDREN: [&str; 7] = [
    "526ed0f5-4e5d-42b3-9e3f-b7ad429f2df8",
    "b6107909-a33b-4c63-ab0d-351ba1a1b672",
    "4cc0b396-4b55-415e-bb26-b51c97e6ca78",
    "fb1e0c9f-8c04-4baf-91e6-4688856a14af",
    "a472e05c-9bbf-48f2-89d7-6c6092c339ed",
    "31dc46ab-eaf1-4f1a-97ed-5ff88200c4d8",
    "221ca2fc-1caf-4c40-92e5-d9f55501ba1e",
];
fn text(p: &Path) -> &str {
    p.to_str().unwrap()
}
// Independent authored inventory: exact bytes and symlink targets, never derived outputs.
fn authored(root: &Path) -> BTreeMap<PathBuf, Value> {
    fn walk(root: &Path, rel: &Path, out: &mut BTreeMap<PathBuf, Value>) {
        let path = root.join(rel);
        let m = fs::symlink_metadata(&path).unwrap();
        if m.is_symlink() {
            out.insert(rel.into(), json!({"link":fs::read_link(path).unwrap()}));
        } else if m.is_dir() {
            for e in fs::read_dir(path).unwrap() {
                let e = e.unwrap();
                let name = e.file_name();
                let name = name.to_str().unwrap();
                if name == "node.meta.json" || name.starts_with("outline") {
                    continue;
                }
                walk(root, &rel.join(name), out);
            }
        } else {
            assert!(m.is_file());
            out.insert(rel.into(), json!({"bytes":fs::read(path).unwrap()}));
        }
    }
    let mut out = BTreeMap::new();
    for name in ["nodes", "trees"] {
        walk(root, Path::new(name), &mut out);
    }
    out
}
fn seed(p: &Path) {
    let root = "10000000-0000-4000-8000-000000000001";
    fs::create_dir_all(p.join("trees")).unwrap();
    for id in std::iter::once(root)
        .chain(std::iter::once(ENTRY))
        .chain(CHILDREN)
    {
        let dir = p.join("nodes").join(id);
        fs::create_dir_all(dir.join("children")).unwrap();
        let ordered: &[&str] = if id == root {
            &[ENTRY]
        } else if id == ENTRY {
            &CHILDREN
        } else {
            &[]
        };
        fs::write(dir.join("node.md"), format!("---\nid: {id:?}\nname: \"Synthetic instruction\"\nmicro_summary: \"Keeps the trial reversible.\"\nstatus: \"not_examined\"\nsee_also: []\n---\nSynthetic authored bytes — café 中.\n")).unwrap();
        fs::write(
            dir.join("children.order"),
            ordered
                .iter()
                .map(|id| format!("{id}\n"))
                .collect::<String>(),
        )
        .unwrap();
        if ordered.is_empty() {
            fs::write(dir.join("children/.keep"), "").unwrap();
        }
        for child in ordered {
            symlink(format!("../../{child}"), dir.join("children").join(child)).unwrap();
        }
    }
    symlink(format!("../nodes/{root}"), p.join("trees").join(root)).unwrap();
}
struct Native {
    evidence: PathBuf,
    git_home: PathBuf,
    records: Vec<Value>,
}
impl Native {
    fn run(&mut self, bin: &str, args: &[&str], ok: bool) -> String {
        let mut c = Command::new(bin);
        for (k, _) in std::env::vars_os() {
            if k.as_encoded_bytes().starts_with(b"GIT_") {
                c.env_remove(k);
            }
        }
        let out = c
            .args(args)
            .current_dir(&self.evidence)
            .env("HOME", &self.git_home)
            .env("XDG_CONFIG_HOME", self.git_home.join(".config"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        let stdout = String::from_utf8(out.stdout).unwrap();
        let stderr = String::from_utf8(out.stderr).unwrap();
        self.records.push(json!({"command":bin,"args":args,"exit":out.status.code(),"signal":out.status.signal(),"stdout":stdout,"stderr":stderr}));
        fs::write(
            self.evidence.join("commands.json"),
            serde_json::to_vec_pretty(&self.records).unwrap(),
        )
        .unwrap();
        assert_eq!(out.status.signal(), None);
        assert_eq!(
            out.status.success(),
            ok,
            "{bin} {args:?}: {stdout}\n{stderr}"
        );
        stdout
    }
    fn cli(&mut self, args: &[&str]) -> String {
        self.run(env!("CARGO_BIN_EXE_sifttext"), args, true)
    }
    fn json(&mut self, args: &[&str]) -> Value {
        let output = self.cli(args);
        serde_json::from_str(&output)
            .unwrap_or_else(|_| sifttext_file_graph::outlines::decode_lean(&output).unwrap())
    }
    fn refuse(&mut self, args: &[&str]) {
        self.run(env!("CARGO_BIN_EXE_sifttext"), args, false);
    }
    fn git(&mut self, p: &Path, args: &[&str]) -> String {
        let mut all = vec![
            "-C",
            text(p),
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgSign=false",
        ];
        all.extend_from_slice(args);
        self.run("git", &all, true).trim().into()
    }
}
#[test]
fn supplied_onramp_preserves_authored_knowledge_through_checked_publish_and_recovery() {
    let tmp = tempfile::tempdir().unwrap();
    let evidence = match std::env::var_os("SIFTTEXT_TEST_EVIDENCE") {
        Some(p) => {
            let p = PathBuf::from(p);
            assert!(p.is_absolute());
            fs::create_dir(&p).unwrap();
            p.canonicalize().unwrap()
        }
        None => tmp.path().canonicalize().unwrap(),
    };
    let git_home = evidence.join("git-home");
    fs::create_dir(&git_home).unwrap();
    fs::write(git_home.join(".gitconfig"), "[user]\nname = Synthetic Fixture\nemail = fixture@example.invalid\n[commit]\ngpgSign = false\n[core]\nhooksPath = /dev/null\n").unwrap();
    let mut n = Native {
        evidence: evidence.clone(),
        git_home,
        records: vec![],
    };
    let synthetic = evidence.join("synthetic-bundle");
    seed(&synthetic);
    let supplied = std::env::var_os("SIFTTEXT_ONRAMP_BUNDLE");
    let bundle = supplied
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or(synthetic)
        .canonicalize()
        .unwrap();
    let original = authored(&bundle);
    assert!(n.cli(&["-h"]).contains("supplied native KB"));
    let home = evidence.join("home");
    assert_eq!(
        n.json(&["init", text(&home), "--bundle", text(&bundle)])["created"],
        true
    );
    assert_eq!(authored(&home), original);
    let instruction_output = n.cli(&["instructions", text(&home)]);
    let mut instruction_lines = instruction_output.lines();
    assert!(instruction_lines.next().unwrap().starts_with("onboarding-entry:"));
    assert!(instruction_lines.next().unwrap().starts_with("hosted-sync:"));
    assert_eq!(instruction_lines.next(), Some("sifttext-help-output:"));
    let pipeline = sifttext_file_graph::outlines::decode_lean(&format!("{}\n", instruction_output.lines().next().unwrap())).unwrap();
    assert_eq!(
        pipeline["onboarding-entry"].as_str().unwrap(),
        text(&home.join("nodes").join(ENTRY).join("node.md"))
    );
    let ordered: Vec<&str> = std::iter::once(ENTRY).chain(CHILDREN).collect();
    for id in ordered.iter().copied() {
        let path = home.join("nodes").join(id).join("node.md");
        assert_eq!(
            n.cli(&["read", text(&home), id]).as_bytes(),
            fs::read(path).unwrap()
        );
    }
    let node = home.join("nodes").join(ENTRY).join("node.md");
    let bytes = fs::read(&node).unwrap();
    fs::write(&node, "a real unfinished user edit").unwrap();
    assert_eq!(
        n.json(&[
            "init",
            text(&home),
            "--bundle",
            text(&evidence.join("missing"))
        ])["created"],
        false
    );
    assert_eq!(
        fs::read_to_string(&node).unwrap(),
        "a real unfinished user edit"
    );
    n.refuse(&["index", text(&home)]);
    n.refuse(&["worktree", text(&home)]);
    fs::write(&node, &bytes).unwrap();
    let prior = evidence.join("prior-home");
    fs::create_dir(&prior).unwrap();
    fs::write(prior.join("notes"), "user content").unwrap();
    n.refuse(&["init", text(&prior), "--empty"]);
    assert_eq!(
        fs::read_to_string(prior.join("notes")).unwrap(),
        "user content"
    );
    let linked = evidence.join("linked-home");
    symlink(&home, &linked).unwrap();
    n.refuse(&["init", text(&linked), "--empty"]);
    let count = original
        .keys()
        .filter(|p| p.file_name().unwrap() == "node.md")
        .count();
    let indexed = n.json(&["index", text(&home)]);
    assert_eq!(indexed["nodes"], count);
    assert!(indexed.get("version_scope").is_none());
    assert!(indexed.get("links").is_none());
    let query = if supplied.is_some() {
        "SiftText"
    } else {
        "Synthetic"
    };
    let search = n.json(&["search", text(&home), query]);
    assert_eq!(search["version"], indexed["version"]);
    assert!(search.get("version_scope").is_none());
    assert!(!search["rows"].as_array().unwrap().is_empty());
    for hit in search["rows"].as_array().unwrap() {
        let path = search["path_template"].as_str().unwrap().replace("{id}", hit["id"].as_str().unwrap());
        assert_eq!(path, text(&home.join("nodes").join(hit["id"].as_str().unwrap()).join("node.md")));
        assert!(hit.get("match").is_none());
    }
    let sql = n.json(&["sql", text(&home), "SELECT count(*) FROM nodes"]);
    assert_eq!(sql["version"], indexed["version"]);
    assert!(sql.get("version_scope").is_none());
    assert_eq!(sql["rows"][0]["c0"], count.to_string());
    assert_eq!(sql["c0_type"], "integer");
    for q in [
        "DELETE FROM nodes",
        "PRAGMA user_version=1",
        "SELECT 1; DELETE FROM nodes",
        "ATTACH 'other' AS other",
    ] {
        n.refuse(&["sql", text(&home), q]);
    }
    let shallow = n.json(&["outline", text(&home), ENTRY]);
    let full = n.json(&["outline", text(&home), ENTRY, "full"]);
    assert_eq!(shallow["coverage"], "shallow");
    assert_eq!(full["coverage"], "full");
    for view in [&shallow, &full] {
        assert!(view.get("source_version_scope").is_none());
        assert!(view.get("source_version").is_none());
        assert_eq!(view["version"], indexed["version"]);
    }
    let name = "Onramppublicationtrial";
    let allocation = n.json(&["worktree", text(&home)]);
    let draft = PathBuf::from(allocation["worktree"].as_str().unwrap());
    let base = allocation["base"].as_str().unwrap();
    let created = n.json(&[
        "create",
        text(&draft),
        name,
        "Checks a private reversible creation.",
        ENTRY,
    ]);
    let file = PathBuf::from(created["path"].as_str().unwrap());
    let created_bytes = fs::read(&file).unwrap();
    fs::write(
        &file,
        String::from_utf8(created_bytes.clone())
            .unwrap()
            .replace("Checks a private reversible creation.", ""),
    )
    .unwrap();
    n.refuse(&["check", text(&draft), text(&home)]);
    n.refuse(&["index", text(&draft)]);
    assert_eq!(n.git(&home, &["rev-parse", "refs/heads/main"]), base);
    fs::write(&file, &created_bytes).unwrap();
    let entry_draft = draft.join("nodes").join(ENTRY).join("node.md");
    let mut edited = fs::read(&entry_draft).unwrap();
    edited.extend_from_slice("\nA separately authored reversible edit — 中.\n".as_bytes());
    fs::write(entry_draft, &edited).unwrap();
    n.cli(&["normalize", text(&home), text(&draft)]);
    n.cli(&["check", text(&draft), text(&home)]);
    let found = n.json(&["search", text(&draft), name]);
    assert_eq!(found["total_matches"], 1);
    let hit = &found["rows"][0];
    assert_eq!(hit["id"], created["id"]);
    assert_eq!(found["path_template"].as_str().unwrap().replace("{id}", hit["id"].as_str().unwrap()), created["path"]);
    n.git(&draft, &["add", "--", "nodes", "trees"]);
    n.git(
        &draft,
        &[
            "commit",
            "-m",
            "knowledge: create and edit reversible trial",
        ],
    );
    let commit = n.git(&draft, &["rev-parse", "HEAD"]);
    let mut dirty = created_bytes;
    dirty.extend_from_slice(b"not committed\n");
    fs::write(&file, dirty).unwrap();
    n.refuse(&["submit", text(&home), text(&draft), &commit, base]);
    assert_eq!(n.git(&home, &["rev-parse", "refs/heads/main"]), base);
    n.git(
        &draft,
        &[
            "restore",
            "--",
            &format!("nodes/{}/node.md", created["id"].as_str().unwrap()),
        ],
    );
    let committed_tree = n.git(&draft, &["rev-parse", &format!("{commit}^{{tree}}")]);
    let committed_authored = authored(&draft);
    let receipt = n.json(&["submit", text(&home), text(&draft), &commit, base]);
    assert_eq!(receipt["status"], "saved");
    let candidate = receipt["accepted"].as_str().unwrap();
    assert_eq!(n.git(&home, &["rev-parse", "refs/heads/main"]), candidate);
    assert_eq!(
        n.git(&home, &["rev-parse", &format!("{candidate}^{{tree}}")]),
        committed_tree
    );
    n.git(&home, &["merge-base", "--is-ancestor", &commit, candidate]);
    n.git(&home, &["merge-base", "--is-ancestor", base, candidate]);
    assert_eq!(authored(&home), committed_authored);
    assert_eq!(n.git(&home, &["rev-parse", "HEAD"]), candidate);
    assert_eq!(receipt["home"], text(&home));
    assert_eq!(receipt["worktree_removed"], true);
    assert!(!draft.exists());
    let view = PathBuf::from(receipt["home"].as_str().unwrap());
    assert_eq!(view, home);
    assert_eq!(
        fs::read(view.join("nodes").join(ENTRY).join("node.md")).unwrap(),
        edited
    );
    assert_eq!(
        n.json(&["sql", text(&view), "SELECT count(*) FROM nodes"])["rows"][0]["c0"],
        (count + 1).to_string()
    );
    assert_eq!(n.json(&["search", text(&view), name])["total_matches"], 1);
    n.cli(&["outline", text(&view), ENTRY, "full"]);
    let recovery = n.json(&["worktree", text(&view)]);
    let restore = PathBuf::from(recovery["worktree"].as_str().unwrap());
    n.git(&restore, &["revert", "--no-edit", &commit]);
    fn status(path: &Path) -> String {
        let s = fs::read_to_string(path).unwrap();
        serde_json::from_str(s.lines().find_map(|l| l.strip_prefix("status: ")).unwrap()).unwrap()
    }
    let explicit: Vec<String> = original
        .keys()
        .filter(|p| {
            p.file_name().unwrap() == "node.md" && status(&view.join(p)) != status(&restore.join(p))
        })
        .map(|p| {
            p.components()
                .nth(1)
                .unwrap()
                .as_os_str()
                .to_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    if !explicit.is_empty() {
        n.refuse(&["check", text(&restore), text(&view)]);
    }
    let mut check = vec!["check", text(&restore), text(&view)];
    check.extend(explicit.iter().map(String::as_str));
    n.cli(&check);
    let reverted = n.git(&restore, &["rev-parse", "HEAD"]);
    let mut args = vec![
        "submit",
        text(&home),
        text(&restore),
        &reverted,
        recovery["base"].as_str().unwrap(),
    ];
    args.extend(explicit.iter().map(String::as_str));
    let restored = n.json(&args);
    assert_eq!(restored["status"], "saved");
    let restored_view = PathBuf::from(restored["home"].as_str().unwrap());
    assert_eq!(authored(&restored_view), original);
    assert_eq!(n.json(&["index", text(&restored_view)])["nodes"], count);
    assert_eq!(
        n.json(&["search", text(&restored_view), name])["total_matches"],
        0
    );
    assert_eq!(
        n.json(&["sql", text(&restored_view), "SELECT count(*) FROM nodes"])["rows"][0]["c0"],
        count.to_string()
    );
    n.cli(&["outline", text(&restored_view), ENTRY, "full"]);
    assert_eq!(authored(&bundle), original);
    let recovered = n.json(&["accepted", text(&home)]);
    assert_eq!(recovered["status"], "ready");
    assert_eq!(recovered["accepted"], restored["accepted"]);
    assert_eq!(recovered["home_advanced"], false);
    assert_eq!(
        authored(Path::new(recovered["view"].as_str().unwrap())),
        original
    );
    let empty = evidence.join("empty-home");
    assert_eq!(
        n.json(&["init", text(&empty), "--empty"])["starter_nodes"],
        0
    );
    assert_eq!(
        n.json(&["init", text(&empty), "--bundle", text(&bundle)])["created"],
        false
    );
    assert_eq!(n.json(&["index", text(&empty)])["nodes"], 0);
    let first = n.json(&["worktree", text(&empty)]);
    let d = Path::new(first["worktree"].as_str().unwrap());
    n.json(&["create", text(d), "First tree", "Starts an empty home."]);
    n.cli(&["check", text(d), text(&empty)]);
    n.git(d, &["add", "--", "nodes", "trees"]);
    n.git(d, &["commit", "-m", "knowledge: first tree"]);
    let c = n.git(d, &["rev-parse", "HEAD"]);
    assert_eq!(
        n.json(&[
            "submit",
            text(&empty),
            text(d),
            &c,
            first["base"].as_str().unwrap()
        ])["status"],
        "saved"
    );
    let prefix = evidence.join("prefix");
    fs::create_dir(&prefix).unwrap();
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR"));
    for name in [
        "Cargo.toml",
        "Cargo.lock",
        "build.rs",
        "install-local.sh",
        "README.md",
        "AGENTS.md",
        ".gitignore",
        "rust-toolchain.toml",
        "src",
        "laws",
        "scripts",
        "tests",
        "fixtures",
        "docs",
        "nodes",
        "trees",
        "build",
    ] {
        n.run(
            "cp",
            &["-R", text(&checkout.join(name)), text(&prefix.join(name))],
            true,
        );
    }
    fs::copy(env!("CARGO_BIN_EXE_sifttext"), prefix.join("sifttext")).unwrap();
    let bin = prefix.join("sifttext");
    n.run(text(&bin), &["check", text(&restored_view)], true);
    let s = sifttext_file_graph::outlines::decode_lean(&n.run(
        text(&bin),
        &["sql", text(&restored_view), "SELECT count(*) FROM nodes"],
        true,
    ))
    .unwrap();
    assert_eq!(s["rows"][0]["c0"], count.to_string());
    assert_eq!(
        n.run(text(&bin), &["read", text(&restored_view), ENTRY], true)
            .as_bytes(),
        bytes
    );
    fs::write(prefix.join("README.md"), "tampered source stamp").unwrap();
    n.run(text(&bin), &["check", text(&restored_view)], false);
    fs::copy(checkout.join("README.md"), prefix.join("README.md")).unwrap();
    fs::write(evidence.join("result.json"),serde_json::to_vec_pretty(&json!({"status":"passed","bundle":bundle,"count":count,"home":home,"pipeline":pipeline,"allocation":allocation,"created":created,"receipt":receipt,"recovery":recovery,"restore":restored,"restoreExplicit":explicit,"recovered":recovered,"prefix":prefix,"commands":n.records.len()})).unwrap()).unwrap();
    println!(
        "local onramp: {} exact native observations passed",
        n.records.len()
    );
}

#[test]
fn ordinary_git_acceleration_is_explicit_and_preserves_dirty_visibility() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let repo = root.join("repo");
    fs::create_dir(&repo).unwrap();
    let run = |bin: &str, args: &[&str]| {
        Command::new(bin)
            .args(args)
            .current_dir(&repo)
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap())
            .env("HOME", &root)
            .env("XDG_CONFIG_HOME", root.join(".config"))
            .output()
            .unwrap()
    };
    let git = |args: &[&str]| {
        let out = run("git", args);
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8(out.stdout).unwrap()
    };
    git(&["init"]);
    fs::write(repo.join("tracked.md"), "original\n").unwrap();
    git(&["add", "tracked.md"]);
    git(&["-c", "user.name=Fixture", "-c", "user.email=fixture@example.invalid", "-c", "commit.gpgSign=false", "-c", "core.hooksPath=/dev/null", "commit", "-m", "fixture: original"]);
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/git-acceleration.sh");
    let opt_in = || run("sh", &[text(&script), "enable", text(&repo), "--all-git-clients-reviewed"]);
    let before = fs::read(repo.join(".git/config")).unwrap();
    assert!(!run("sh", &[text(&script), "enable", text(&repo)]).status.success());
    assert_eq!(fs::read(repo.join(".git/config")).unwrap(), before);
    // Unsupported hosts/builds must refuse without modifying the repository.
    if !cfg!(target_os = "macos") || !git(&["version", "--build-options"]).contains("feature: fsmonitor--daemon") {
        assert!(!opt_in().status.success());
        assert_eq!(fs::read(repo.join(".git/config")).unwrap(), before);
        return;
    }
    git(&["config", "core.fsmonitor", "/nonexistent-custom-monitor"]);
    let custom = fs::read(repo.join(".git/config")).unwrap();
    assert!(!opt_in().status.success());
    assert_eq!(fs::read(repo.join(".git/config")).unwrap(), custom);
    git(&["config", "--unset", "core.fsmonitor"]);
    // Stop only this fixture's daemon, including on assertion failure.
    struct Stop(PathBuf);
    impl Drop for Stop {
        fn drop(&mut self) {
            let _ = Command::new("git").arg("-C").arg(&self.0)
                .args(["-c", "core.fsmonitor=false", "fsmonitor--daemon", "stop"])
                .env_clear()
                .env("PATH", std::env::var_os("PATH").unwrap())
                .env("HOME", &self.0)
                .output();
        }
    }
    let _stop = Stop(repo.clone());
    fs::write(repo.join("tracked.md"), "real unfinished edit\n").unwrap();
    fs::write(repo.join("sibling.md"), "new authored file\n").unwrap();
    let staged = git(&["ls-files", "--stage"]);
    let enabled = opt_in();
    assert!(enabled.status.success(), "{}", String::from_utf8_lossy(&enabled.stderr));
    let status = String::from_utf8(enabled.stdout).unwrap();
    assert!(status.contains(" M tracked.md\n") && status.contains("?? sibling.md\n"), "{status}");
    assert_eq!(git(&["ls-files", "--stage"]), staged);
    assert_eq!(fs::read_to_string(repo.join("tracked.md")).unwrap(), "real unfinished edit\n");
    let configured = fs::read(repo.join(".git/config")).unwrap();
    assert!(opt_in().status.success());
    assert_eq!(fs::read(repo.join(".git/config")).unwrap(), configured);
}
