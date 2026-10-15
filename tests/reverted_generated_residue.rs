//! Checked Git publication/revert/refresh regression, using only synthetic knowledge.
//! Build checked assets first, then cargo test --locked --test reverted_generated_residue.
//! SIFTTEXT_TEST_EVIDENCE optionally retains receipts in a new directory;
//! SIFTTEXT_TEST_NATIVE_BIN_DIR selects an independently checked native binary set.
#[path = "common/native_resources.rs"]
mod native_resources;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    os::unix::{
        fs::{MetadataExt, symlink},
        process::ExitStatusExt,
    },
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

fn id(n: usize) -> String {
    format!("71000000-0000-4000-8000-{n:012}")
}
fn directory(repo: &Path, n: usize) -> PathBuf {
    repo.join("nodes").join(id(n))
}
fn node(repo: &Path, n: usize, parent: Option<usize>) {
    let dir = directory(repo, n);
    fs::create_dir_all(dir.join("children")).unwrap();
    fs::write(dir.join("children/.keep"), "").unwrap();
    fs::write(dir.join("children.order"), "").unwrap();
    let fields = json!({"id": id(n), "name": format!("Synthetic {n}"),
        "micro_summary": format!("Summary {n}"), "status": "in_progress", "see_also": []});
    let metadata: String = fields
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| format!("{k}: {v}\n"))
        .collect();
    fs::write(
        dir.join("node.md"),
        format!("---\n{metadata}---\nAuthored body {n}.\n"),
    )
    .unwrap();
    match parent {
        None => symlink(
            format!("../nodes/{}", id(n)),
            repo.join("trees").join(id(n)),
        )
        .unwrap(),
        Some(parent) => {
            symlink(
                format!("../../{}", id(n)),
                directory(repo, parent).join("children").join(id(n)),
            )
            .unwrap();
            let order = directory(repo, parent).join("children.order");
            let previous = fs::read_to_string(&order).unwrap();
            fs::write(order, previous + &id(n) + "\n").unwrap();
        }
    }
}
struct Commands {
    evidence: PathBuf,
    bin: PathBuf,
    sequence: usize,
}
impl Commands {
    fn run(&mut self, command: &Path, args: &[&str], input: Option<&Value>) -> String {
        let mut child = Command::new(command);
        // Fixture Git must not inherit an agent's alternate index/worktree or signing policy.
        for (key, _) in std::env::vars_os() {
            if key.as_encoded_bytes().starts_with(b"GIT_") {
                child.env_remove(key);
            }
        }
        child
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut process = child.spawn().unwrap();
        if let Some(input) = input {
            process
                .stdin
                .take()
                .unwrap()
                .write_all(&serde_json::to_vec(input).unwrap())
                .unwrap();
        } else {
            drop(process.stdin.take());
        }
        let output = process.wait_with_output().unwrap();
        let stdout = String::from_utf8(output.stdout).unwrap();
        let stderr = String::from_utf8(output.stderr).unwrap();
        let receipt = json!({"command": command, "args": args, "input": input,
            "exit": output.status.code(), "signal": output.status.signal(), "stdout": stdout, "stderr": stderr});
        fs::write(
            self.evidence
                .join(format!("command-{}.json", self.sequence)),
            serde_json::to_vec_pretty(&receipt).unwrap(),
        )
        .unwrap();
        self.sequence += 1;
        assert!(output.status.success(), "{receipt:#}");
        stdout
    }
    fn git(&mut self, repo: &Path, args: &[&str]) -> String {
        let mut all = vec![
            "-C",
            repo.to_str().unwrap(),
            "-c",
            "user.name=Synthetic Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgSign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ];
        all.extend_from_slice(args);
        self.run(Path::new("git"), &all, None).trim().into()
    }
    fn observe(&mut self, request: Value) -> Value {
        let bin = self.bin.join("core-observe");
        serde_json::from_str(&self.run(&bin, &[], Some(&request))).unwrap()
    }
}
fn residue(repo: &Path, n: usize) -> BTreeMap<PathBuf, Vec<u8>> {
    fs::read_dir(directory(repo, n))
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            let meta = fs::symlink_metadata(&path).unwrap();
            assert!(
                meta.is_file() && meta.nlink() == 1,
                "unsafe residue {path:?}"
            );
            let digest = Sha256::digest(fs::read(&path).unwrap()).to_vec();
            (path.file_name().unwrap().into(), digest)
        })
        .collect()
}
#[test]
fn checked_publication_revert_recovers_current_views_without_relocating_residue() {
    let temporary = tempfile::tempdir().unwrap();
    let root = match std::env::var_os("SIFTTEXT_TEST_EVIDENCE") {
        Some(path) => {
            let path = PathBuf::from(path);
            assert!(path.is_absolute());
            fs::create_dir_all(&path).unwrap();
            path
        }
        None => temporary.path().into(),
    };
    // Sibling cases share the root, but never their fixtures or receipts.
    let evidence = root.join("checked-publication");
    fs::create_dir(&evidence).unwrap();
    let bin = std::env::var_os("SIFTTEXT_TEST_NATIVE_BIN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_BIN_EXE_core-observe"))
                .parent()
                .unwrap()
                .into()
        });
    assert!(bin.is_absolute());
    let mut commands = Commands {
        evidence: evidence.clone(),
        bin,
        sequence: 0,
    };
    let repo = evidence.join("repository");
    let draft = evidence.join("draft");
    fs::create_dir_all(repo.join("trees")).unwrap();
    node(&repo, 1, None);
    node(&repo, 2, Some(1));
    fs::write(
        repo.join(".gitignore"),
        ".sifttext/\n*.lean\n*.meta.json\nnode.meta.lock\n",
    )
    .unwrap();
    commands.git(&repo, &["init", "-q", "-b", "main"]);
    commands.git(&repo, &["add", "."]);
    commands.git(&repo, &["commit", "-qm", "fixture: baseline"]);
    let base = commands.git(&repo, &["rev-parse", "HEAD"]);
    let baseline_tree = commands.git(&repo, &["rev-parse", "HEAD^{tree}"]);
    let authored: BTreeMap<_, _> = [1, 2]
        .into_iter()
        .flat_map(|n| {
            ["node.md", "children.order"].map(|name| {
                let path = PathBuf::from("nodes").join(id(n)).join(name);
                let bytes = fs::read(repo.join(&path)).unwrap();
                (path, bytes)
            })
        })
        .collect();
    let baseline = commands.observe(json!({"op": "index", "store": repo, "mode": "full"}));
    assert_eq!(baseline["status"], "current");
    assert!(baseline["version"].as_str().is_some_and(|v| !v.is_empty()));
    commands.git(&repo, &["checkout", "--detach", "-q", &base]);
    commands.git(
        &repo,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "synthetic-author",
            draft.to_str().unwrap(),
            &base,
        ],
    );
    node(&draft, 3, None);
    node(&draft, 4, Some(3));
    commands.git(&draft, &["add", "nodes", "trees"]);
    commands.git(
        &draft,
        &["commit", "-qm", "fixture: create scratch subtree"],
    );
    let created = commands.git(&draft, &["rev-parse", "HEAD"]);
    let created_index = commands.observe(json!({"op": "index", "store": draft, "mode": "full"}));
    assert_eq!(created_index["status"], "current");
    for n in [3, 4] {
        let refreshed = commands
            .observe(json!({"op": "token_refresh", "path": directory(&draft, n).join("node.md")}));
        assert_eq!(refreshed["status"], "current");
    }
    let bin = commands.bin.join("git-submit-observe");
    let publication: Value = serde_json::from_str(&commands.run(&bin, &[], Some(&json!({
        "repo": repo, "accepted_ref": "refs/heads/main", "draft": draft, "commit": created, "base": base,
    })))).unwrap();
    assert_eq!(publication["status"], "current");
    assert_eq!(publication["published"], true);
    assert_eq!(
        commands.git(&repo, &["rev-parse", "main"]),
        publication["candidate"].as_str().unwrap()
    );
    commands.git(&draft, &["revert", "--no-edit", &created]);
    assert_eq!(
        commands.git(&draft, &["rev-parse", "HEAD^{tree}"]),
        baseline_tree
    );
    assert_eq!(commands.git(&draft, &["status", "--porcelain"]), "");
    let mut generated = BTreeMap::new();
    for n in [3, 4] {
        assert!(!directory(&draft, n).join("node.md").exists());
        assert!(!directory(&draft, n).join("children").exists());
        let files = residue(&draft, n);
        assert!(
            !files.is_empty(),
            "control requires actual generated residue"
        );
        generated.insert(n, files);
    }
    let recovery = commands.observe(json!({"op": "index", "store": draft, "mode": "incremental"}));
    assert_eq!(recovery, baseline);
    let candidate = commands.observe(json!({"op": "validate_candidate", "store": draft, "expected_version": baseline["version"]}));
    assert_eq!(candidate["status"], "current");
    let search = commands.observe(
        json!({"op": "search", "store": draft, "query": "Synthetic", "under_node_id": null,
        "limit": 10, "offset": 0, "readable_ids": [id(1), id(2), id(3), id(4)]}),
    );
    assert_eq!(search["status"], "current");
    assert_eq!(search["version"], baseline["version"]);
    assert_eq!(search["total_matches"], 2);
    let outlines = commands.observe(json!({"op": "outline_inspect", "store": draft}));
    assert_eq!(outlines["status"], "current");
    assert_eq!(outlines["version"], baseline["version"]);
    let files = outlines["files"].as_array().unwrap();
    for name in ["outline.lean", "outline.full.lean"] {
        assert!(
            files
                .iter()
                .any(|f| f["path"].as_str() == directory(&draft, 1).join(name).to_str())
        );
    }
    assert!(!files.iter().any(|f| {
        [id(3), id(4)]
            .iter()
            .any(|id| f["path"].as_str().unwrap().contains(id))
    }));
    let sqlite = draft.join(".sifttext/index.sqlite");
    {
        let connection = rusqlite::Connection::open_with_flags(
            &sqlite,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let mut statement = connection
            .prepare("SELECT id FROM nodes ORDER BY id")
            .unwrap();
        let ids: Vec<String> = statement
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert_eq!(ids, [id(1), id(2)]);
    }
    fs::remove_file(sqlite).unwrap();
    let rebuilt = commands.observe(json!({"op": "index", "store": draft, "mode": "full"}));
    assert_eq!(rebuilt, baseline);
    assert_eq!(
        commands.observe(json!({"op": "outline_inspect", "store": draft}))["status"],
        "current"
    );
    for n in [3, 4] {
        assert_eq!(residue(&draft, n), generated[&n]);
    }
    for (path, bytes) in &authored {
        assert_eq!(&fs::read(draft.join(path)).unwrap(), bytes);
    }
    assert_eq!(
        commands.git(&repo, &["rev-parse", "main"]),
        publication["candidate"].as_str().unwrap()
    );
    assert_eq!(
        commands.git(&draft, &["rev-parse", "HEAD^{tree}"]),
        baseline_tree
    );
    assert_eq!(commands.git(&draft, &["status", "--porcelain"]), "");
    fs::write(evidence.join("result.json"), serde_json::to_vec_pretty(&json!({
        "base": base, "baselineTree": baseline_tree, "baselineVersion": baseline["version"],
        "created": created, "publication": publication, "authored": authored, "residue": generated,
        "recovery": recovery, "candidate": candidate, "search": search, "outlines": outlines,
        "rebuilt": rebuilt, "acceptedRefUnchanged": true, "noResidueRelocation": true,
    })).unwrap()).unwrap();
}

/// Live-process controls retained from the capture-residue runner: a real refresh must refuse
/// an unknown or forged generated residue, refuse same-byte replacement of authored source or
/// residue at its pre-accept barrier, and survive SIGKILL there without publishing a view,
/// disturbing residue bytes or moving the accepted ref.
/// Runs one native attempt under `/usr/bin/time` when the harness provides it, so each
/// refusal receipt carries the same per-operation peak-RSS record the legacy runner retained.
fn observe(request: &Value, bin: &Path, evidence: &Path, label: &str) -> std::process::Output {
    let timed = Path::new("/usr/bin/time");
    let mut child = if timed.exists() {
        let mut wrapper = Command::new(timed);
        #[cfg(target_os = "macos")]
        wrapper.arg("-l");
        #[cfg(target_os = "linux")]
        wrapper.arg("-v");
        wrapper.arg(bin);
        wrapper
    } else {
        Command::new(bin)
    };
    let timer = std::time::Instant::now();
    for (key, _) in std::env::vars_os() {
        if key.as_encoded_bytes().starts_with(b"GIT_") {
            child.env_remove(key);
        }
    }
    child.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut process = child.spawn().unwrap();
    process.stdin.take().unwrap().write_all(&serde_json::to_vec(request).unwrap()).unwrap();
    let output = process.wait_with_output().unwrap();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let receipt = json!({"request": request, "exit": output.status.code(), "signal": output.status.signal(),
        "elapsed_ms": timer.elapsed().as_secs_f64() * 1000.0,
        "native_rss_bytes": if timed.exists() { Value::from(native_resources::native_rss(&stderr)) } else { Value::Null },
        "stdout": String::from_utf8_lossy(&output.stdout).into_owned(), "stderr": stderr});
    fs::write(evidence.join(format!("{label}.json")), serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();
    assert!(!output.stdout.is_empty() || !output.stderr.is_empty(), "{label}: silent native attempt");
    output
}

/// Parks a real refresh at its pre-accept barrier so a mutation can land between inventory and
/// acceptance, which no in-process hook can show.
struct Barrier {
    child: std::process::Child,
    release: PathBuf,
    label: String,
    evidence: PathBuf,
    request: Value,
    resource: Value,
}

impl Barrier {
    fn start(observer: &Path, evidence: &Path, label: &str, mut request: Value) -> Self {
        let ready = evidence.join(format!("{label}.ready"));
        let release = evidence.join(format!("{label}.release"));
        request["pause_before_accept"] = json!({"ready": ready, "release": release});
        let mut child = Command::new(observer);
        for (key, _) in std::env::vars_os() {
            if key.as_encoded_bytes().starts_with(b"GIT_") {
                child.env_remove(key);
            }
        }
        child.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut process = child.spawn().unwrap();
        process.stdin.take().unwrap().write_all(&serde_json::to_vec(&request).unwrap()).unwrap();
        let timer = std::time::Instant::now();
        while !ready.exists() {
            assert!(timer.elapsed() < std::time::Duration::from_secs(9), "{label}: native barrier never reached");
            assert!(process.try_wait().unwrap().is_none(), "{label}: refresh exited before the barrier: {request}");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        // Sampled while parked: the operation holds a real native process tree at the barrier.
        let pid = process.id();
        let (rss, descriptors, rows) = native_resources::sample(pid);
        Self {
            child: process,
            release,
            label: label.into(),
            evidence: evidence.into(),
            request,
            resource: json!({"pid": pid, "sampled_rss_bytes": rss, "descriptors": descriptors, "rows": rows}),
        }
    }
    fn finish(mut self, kill: bool) -> std::process::Output {
        if kill {
            self.child.kill().unwrap();
        } else {
            fs::write(&self.release, "release\n").unwrap();
        }
        let output = self.child.wait_with_output().unwrap();
        let receipt = json!({"request": self.request, "resource": self.resource,
            "exit": output.status.code(),
            "signal": output.status.signal(),
            "stdout": String::from_utf8_lossy(&output.stdout).into_owned(),
            "stderr": String::from_utf8_lossy(&output.stderr).into_owned()});
        fs::write(self.evidence.join(format!("{}.json", self.label)), serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();
        output
    }
}

/// Same bytes, new inode: the only way to show acceptance keyed physical identity, not content.
fn replace(path: &Path, bytes: &[u8]) {
    let staged = path.with_extension("staged");
    fs::write(&staged, bytes).unwrap();
    fs::rename(&staged, path).unwrap();
}

#[test]
fn live_refresh_refuses_barrier_replacement_and_survives_interruption_with_residue() {
    let temporary = tempfile::tempdir().unwrap();
    // A dedicated subdirectory keeps this case runnable beside the sibling test that
    // receives the same retained-evidence root.
    let root = match std::env::var_os("SIFTTEXT_TEST_EVIDENCE") {
        Some(path) => { let path = PathBuf::from(path); assert!(path.is_absolute()); fs::create_dir_all(&path).unwrap(); path }
        None => temporary.path().into(),
    };
    let evidence = root.join("live-process");
    fs::create_dir(&evidence).unwrap();
    let bin = std::env::var_os("SIFTTEXT_TEST_NATIVE_BIN_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_BIN_EXE_core-observe")).parent().unwrap().into());
    assert!(bin.is_absolute());
    let observer = bin.join("core-observe");
    let mut commands = Commands { evidence: evidence.clone(), bin: bin.clone(), sequence: 0 };
    let store = evidence.join("live-residue");
    fs::create_dir_all(store.join("trees")).unwrap();
    node(&store, 1, None);
    fs::write(store.join(".gitignore"), ".sifttext/\n*.lean\n*.meta.json\nnode.meta.lock\n").unwrap();
    commands.git(&store, &["init", "-q", "-b", "main"]);
    commands.git(&store, &["add", "."]);
    commands.git(&store, &["commit", "-qm", "fixture: baseline"]);
    let refresh = json!({"op": "index", "store": store, "mode": "incremental"});
    let rebuild = json!({"op": "index", "store": store, "mode": "full"});
    let baseline = commands.observe(rebuild.clone());
    assert_eq!(baseline["status"], "current");
    assert_eq!(commands.observe(json!({"op": "outline_inspect", "store": store}))["status"], "current");
    node(&store, 2, None);
    commands.git(&store, &["add", "nodes", "trees"]);
    commands.git(&store, &["commit", "-qm", "fixture: creation"]);
    assert_eq!(commands.observe(json!({"op": "token_refresh", "path": directory(&store, 2).join("node.md")}))["status"], "current");
    commands.observe(refresh.clone());
    commands.git(&store, &["revert", "--no-edit", "HEAD"]);
    let accepted_ref = commands.git(&store, &["rev-parse", "refs/heads/main"]);
    let source = directory(&store, 1).join("node.md");
    let authored = fs::read(&source).unwrap();
    assert!(!directory(&store, 2).join("node.md").exists());
    let retained = residue(&store, 2);
    for name in ["node.meta.json", "outline.lean", "outline.full.lean"] {
        assert!(retained.contains_key(Path::new(name)), "control requires generated residue {name}");
    }

    // An orphan inside the residue directory is refused by the real process and preserved.
    let unknown = directory(&store, 2).join("protected.txt");
    fs::write(&unknown, "protected resource").unwrap();
    let refused = observe(&rebuild, &observer, &evidence, "orphan-residue");
    assert!(!refused.status.success(), "live capture accepted an unknown orphan residue");
    assert_eq!(fs::read(&unknown).unwrap(), b"protected resource");
    fs::remove_file(&unknown).unwrap();

    // A forged residue body is refused even though its name is a known generated surface.
    let outline = directory(&store, 2).join("outline.lean");
    let saved = fs::read(&outline).unwrap();
    fs::write(&outline, "forged content").unwrap();
    assert!(!observe(&rebuild, &observer, &evidence, "forged-residue").status.success(), "live capture accepted forged residue bytes");
    fs::write(&outline, &saved).unwrap();

    let mut refused_at_barrier = 0;
    for (label, path, bytes) in [("residue-replacement", &outline, &saved), ("source-replacement", &source, &authored)] {
        let barrier = Barrier::start(&observer, &evidence, label, refresh.clone());
        replace(path, bytes);
        assert!(!barrier.finish(false).status.success(), "{label}: refresh accepted a same-byte replacement at its barrier");
        refused_at_barrier += 1;
        assert_eq!(commands.observe(refresh.clone()), baseline, "{label}: recovery");
    }

    // SIGKILL at the same barrier must emit no view and leave every retained surface intact.
    let killed = Barrier::start(&observer, &evidence, "interrupted-refresh", refresh.clone()).finish(true);
    assert_eq!(killed.status.signal(), Some(9), "interrupted refresh did not die by SIGKILL");
    assert!(killed.stdout.is_empty(), "interrupted refresh published a view");
    refused_at_barrier += 1;
    assert_eq!(commands.observe(refresh.clone()), baseline);
    assert_eq!(commands.observe(json!({"op": "outline_inspect", "store": store}))["status"], "current");
    assert_eq!(commands.git(&store, &["rev-parse", "refs/heads/main"]), accepted_ref);
    assert_eq!(fs::read(&source).unwrap(), authored);
    assert_eq!(residue(&store, 2), retained, "residue bytes were disturbed");
    assert_eq!(commands.observe(rebuild), baseline, "cache rebuild after interruption");
    fs::write(evidence.join("live-process-result.json"), serde_json::to_vec_pretty(&json!({
        "baselineVersion": baseline["version"], "acceptedRef": accepted_ref,
        "refusedOrAttempts": refused_at_barrier,
        "residueFiles": retained.keys().map(|name| name.to_str().unwrap_or("<non-utf8>")).collect::<Vec<_>>(),
        "limits": ["One pre-accept barrier per case; not universal crash or interleaving proof",
            "Synthetic two-root store only; no private knowledge or predecessor source consumed",
            "No residue relocation or deletion; authored bytes and accepted ref stay unchanged"]
    })).unwrap()).unwrap();
}
